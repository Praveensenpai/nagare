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
use std::time::Duration;

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
///
/// # Errors
/// Returns an error only for setup problems (database, feed fetch, missing
/// auth). Per-release failures are collected in the returned report.
pub async fn run(cfg: &Config, db: &mut Db, dry_run: bool) -> Result<Report> {
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
        match deliver(cfg, db, seedr, show.id, item).await {
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
) -> Result<PathBuf> {
    let folder = cache_folder(client, cfg, &item.magnet).await?;

    let transfer = seedr_dl::TransferOptions {
        output_dir: cfg.staging_dir.clone(),
        delete_cloud: true,
        notifier: None,
    };
    let seedr_cfg = seedr_dl::Config {
        download_dir: cfg.staging_dir.clone(),
        delete_after_download: true,
        download_threads: cfg.download_threads,
    };
    let files = seedr_dl::transfer_download_folder(client, &seedr_cfg, &folder, &transfer).await?;

    let placement = organize::plan(item);
    let placed = organize::place(&cfg.library_dir, &cfg.staging_dir, &placement, &files).await?;
    organize::refresh_jellyfin(&cfg.jellyfin).await?;

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
    Ok(placed)
}

async fn cache_folder(
    client: &SeedrClient,
    cfg: &Config,
    magnet: &str,
) -> Result<seedr_dl::SeedrFolder> {
    let policy = seedr_dl::CachePolicy {
        max_wait: Duration::from_secs(cfg.max_wait_secs),
        min_free_bytes: cfg.min_free_bytes,
        ..seedr_dl::CachePolicy::default()
    };
    let opts = seedr_dl::EnsureOpts {
        policy,
        on_progress: None,
    };
    match client.ensure_cached(magnet, &opts).await? {
        seedr_dl::CacheOutcome::Ready(folder) => Ok(folder),
        seedr_dl::CacheOutcome::Timeout { progress, .. } => {
            bail!("Seedr cache timed out at {progress:.0}%")
        }
        seedr_dl::CacheOutcome::NotEnoughSpace { free, needed } => {
            bail!("Seedr free space {free} below required {needed}")
        }
    }
}
