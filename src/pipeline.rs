//! Scheduler pass: fetch feed, match tracked shows, deliver through Seedr, purge.

use crate::config::Config;
use crate::db::Db;
use crate::feed::{self, Item};
use crate::matcher;
use crate::notify;
use crate::organize;
use anyhow::{bail, Context, Result};
use seedr_dl::{load_auth, SeedrClient};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

/// Transfer stage for visual progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TransferStage {
    Caching,
    Downloading,
    Organizing,
}

/// Real-time transfer statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferProgress {
    pub show: String,
    pub file_name: String,
    pub stage: TransferStage,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub percent: f64,
}

/// Summary of one scheduler pass.
#[derive(Debug, Default)]
pub struct Report {
    /// Episodes placed in the library.
    pub grabbed: Vec<String>,
    /// Episodes that a dry run would have grabbed.
    pub planned: Vec<String>,
    /// Reasons the pass did no work.
    pub skipped: Vec<String>,
    /// Non-fatal per-release failures.
    pub failed: Vec<String>,
    /// True when the feed answered 304.
    pub not_modified: bool,
}

impl Report {
    /// True when nothing was grabbed, planned, or failed.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.grabbed.is_empty() && self.planned.is_empty() && self.failed.is_empty()
    }
}

/// Runs one scheduler pass.
pub async fn run(cfg: &Config, db: &mut Db, dry_run: bool) -> Result<Report> {
    run_with_progress(cfg, db, dry_run, None).await
}

/// Runs one scheduler pass with optional progress reporting.
pub async fn run_with_progress(
    cfg: &Config,
    db: &mut Db,
    dry_run: bool,
    progress: Option<UnboundedSender<TransferProgress>>,
) -> Result<Report> {
    let mut report = Report::default();
    let shows = db.shows()?;
    if shows.is_empty() {
        report.skipped.push("no tracked shows".into());
        return Ok(report);
    }

    let etag = db.get_state("feed_etag")?;
    let last_modified = db.get_state("feed_last_modified")?;
    let outcome = feed::fetch(&cfg.feed_url, etag.as_deref(), last_modified.as_deref()).await?;
    if outcome.not_modified {
        report.not_modified = true;
        return Ok(report);
    }
    if let Some(value) = &outcome.etag {
        db.set_state("feed_etag", value)?;
    }
    if let Some(value) = &outcome.last_modified {
        db.set_state("feed_last_modified", value)?;
    }

    let mut client: Option<SeedrClient> = None;
    for show in shows.iter().filter(|show| show.enabled) {
        let Some(item) = matcher::pick(show, &outcome.items) else {
            continue;
        };
        if db.has_release(&item.infohash)? {
            continue;
        }
        if dry_run {
            report.planned.push(item.title.clone());
            continue;
        }
        if client.is_none() {
            let auth = load_auth()?.context("no Seedr auth; run `seedr-dl auth` first")?;
            client = Some(SeedrClient::new(auth.access_token));
        }
        let Some(seedr) = client.as_ref() else {
            continue;
        };
        match deliver(cfg, db, seedr, show.id, item, progress.as_ref()).await {
            Ok(path) => report.grabbed.push(path.display().to_string()),
            Err(err) => {
                let msg = handle_deliver_error(cfg, db, item, &err).await?;
                report.failed.push(msg);
            }
        }
    }
    Ok(report)
}

async fn handle_deliver_error(
    cfg: &Config,
    db: &Db,
    item: &Item,
    err: &anyhow::Error,
) -> Result<String> {
    let message = format!("{}: {err:#}", item.title);
    if cfg.notify {
        notify::alert("nagare failed to grab an episode", &message).await;
    }
    db.log_event("error", &message)?;
    Ok(message)
}

async fn deliver(
    cfg: &Config,
    db: &mut Db,
    client: &SeedrClient,
    show_id: i64,
    item: &Item,
    progress: Option<&UnboundedSender<TransferProgress>>,
) -> Result<PathBuf> {
    send_stage_update(progress, item, TransferStage::Caching, 0.0);
    let (folder, purged) = cache_folder(client, cfg, item, progress).await?;
    if purged > 0 {
        db.log_event(
            "purge",
            &format!("freed cloud space by deleting {purged} completed folder(s)"),
        )?;
    }

    let notifier = progress.map(|tx| {
        let show = item.show.clone();
        let tx = tx.clone();
        seedr_dl::Notifier::new(seedr_dl::NotifierConfig {
            callback_fn: Some(Arc::new(move |event| {
                forward_seedr_progress(event, &show, &tx);
            })),
            ..Default::default()
        })
    });

    let transfer = seedr_dl::TransferOptions {
        output_dir: cfg.staging_dir.clone(),
        delete_cloud: true,
        notifier: notifier.as_ref(),
    };
    let seedr_cfg = seedr_dl::Config {
        download_dir: cfg.staging_dir.clone(),
        delete_after_download: true,
        download_threads: cfg.download_threads,
    };
    let files = seedr_dl::transfer_download_folder(client, &seedr_cfg, &folder, &transfer).await?;

    send_stage_update(progress, item, TransferStage::Organizing, 100.0);
    let placement = organize::plan(item);
    let placed = organize::place(&cfg.library_dir, &cfg.staging_dir, &placement, &files).await?;
    organize::refresh_jellyfin(&cfg.jellyfin).await?;

    record_and_notify(cfg, db, show_id, item, &placed).await?;
    Ok(placed)
}

fn send_stage_update(
    progress: Option<&UnboundedSender<TransferProgress>>,
    item: &Item,
    stage: TransferStage,
    percent: f64,
) {
    if let Some(tx) = progress {
        let _ = tx.send(TransferProgress {
            show: item.show.clone(),
            file_name: item.title.clone(),
            stage,
            downloaded_bytes: (item.bytes as f64 * (percent / 100.0)) as u64,
            total_bytes: item.bytes,
            speed_bps: 0,
            eta_seconds: 0,
            percent,
        });
    }
}

fn forward_seedr_progress(
    event: seedr_dl::NotificationEvent,
    show: &str,
    tx: &UnboundedSender<TransferProgress>,
) {
    if let seedr_dl::NotificationEvent::Progress {
        file_name,
        downloaded_bytes,
        total_bytes,
        speed_bps,
        eta_seconds,
        percentage,
        ..
    } = event
    {
        let _ = tx.send(TransferProgress {
            show: show.to_string(),
            file_name,
            stage: TransferStage::Downloading,
            downloaded_bytes,
            total_bytes,
            speed_bps,
            eta_seconds,
            percent: percentage,
        });
    }
}

async fn record_and_notify(
    cfg: &Config,
    db: &mut Db,
    show_id: i64,
    item: &Item,
    placed: &std::path::Path,
) -> Result<()> {
    db.record_release(
        &item.infohash,
        show_id,
        item.season,
        item.episode.unwrap_or(0.0),
        &item.title,
        item.bytes,
    )?;
    db.log_event("grab", &format!("{} -> {}", item.title, placed.display()))?;

    if cfg.notify {
        notify::done(
            "nagare grabbed an episode",
            &format!(
                "{} {}\n{}",
                item.show,
                item.episode_label(),
                placed.display()
            ),
        )
        .await;
    }
    Ok(())
}

async fn cache_folder(
    client: &SeedrClient,
    cfg: &Config,
    item: &Item,
    progress: Option<&UnboundedSender<TransferProgress>>,
) -> Result<(seedr_dl::SeedrFolder, usize)> {
    let needed = needed_free_bytes(item.bytes, cfg.min_free_bytes);
    let policy = seedr_dl::CachePolicy {
        max_wait: Duration::from_secs(cfg.max_wait_secs),
        min_free_bytes: needed,
        ..seedr_dl::CachePolicy::default()
    };
    let on_progress_cb = progress.map(|tx| {
        let show = item.show.clone();
        let total = item.bytes;
        let tx = tx.clone();
        move |torrent: &seedr_dl::SeedrTorrent| {
            let prog = torrent.progress.unwrap_or(0.0);
            let _ = tx.send(TransferProgress {
                show: show.clone(),
                file_name: torrent.name.clone(),
                stage: TransferStage::Caching,
                downloaded_bytes: (total as f64 * (prog / 100.0)) as u64,
                total_bytes: total,
                speed_bps: 0,
                eta_seconds: 0,
                percent: prog,
            });
        }
    });
    let on_prog_ref = on_progress_cb
        .as_ref()
        .map(|cb| cb as &(dyn Fn(&seedr_dl::SeedrTorrent) + Send + Sync));

    let opts = seedr_dl::EnsureOpts {
        policy,
        on_progress: on_prog_ref,
    };
    match client.ensure_cached(&item.magnet, &opts).await? {
        seedr_dl::CacheOutcome::Ready(folder) => Ok((folder, 0)),
        seedr_dl::CacheOutcome::Timeout { progress, .. } => {
            bail!("Seedr cache timed out at {progress:.0}%")
        }
        seedr_dl::CacheOutcome::NotEnoughSpace { free, needed } => {
            let purged = purge_completed_cloud(client, &item.magnet).await?;
            if purged > 0 {
                let folder = retry_ensure_cached(client, &item.magnet, &opts).await?;
                Ok((folder, purged))
            } else {
                bail!("Seedr free space {free} below required {needed}")
            }
        }
    }
}

async fn retry_ensure_cached(
    client: &SeedrClient,
    magnet: &str,
    opts: &seedr_dl::EnsureOpts<'_>,
) -> Result<seedr_dl::SeedrFolder> {
    match client.ensure_cached(magnet, opts).await? {
        seedr_dl::CacheOutcome::Ready(folder) => Ok(folder),
        seedr_dl::CacheOutcome::Timeout { progress, .. } => {
            bail!("Seedr cache timed out at {progress:.0}%")
        }
        seedr_dl::CacheOutcome::NotEnoughSpace { free, needed } => {
            bail!("Seedr free space {free} below required {needed} after cloud purge")
        }
    }
}

async fn purge_completed_cloud(client: &SeedrClient, magnet: &str) -> Result<usize> {
    let target_name = seedr_dl::extract_magnet_name(magnet);
    let root = client.list_root().await?;
    let to_delete: Vec<_> = root
        .folders
        .into_iter()
        .filter(|f| target_name.as_deref() != Some(&f.name))
        .collect();
    if to_delete.is_empty() {
        return Ok(0);
    }
    let count = to_delete.len();
    client.delete_all_folders(&to_delete).await?;
    Ok(count)
}

const fn needed_free_bytes(item_bytes: u64, min_free_bytes: u64) -> u64 {
    if item_bytes > 0 {
        item_bytes.saturating_add(50 * 1024 * 1024)
    } else {
        min_free_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needed_free_bytes_zero() {
        assert_eq!(needed_free_bytes(0, 2048), 2048);
    }

    #[test]
    fn test_needed_free_bytes_with_size() {
        let size = 1_000_000_000;
        let margin = 50 * 1024 * 1024;
        assert_eq!(needed_free_bytes(size, 2048), size + margin);
    }
}
