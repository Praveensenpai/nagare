//! Downloads CLI command implementation.

use crate::config::Config;
use crate::db::{Db, GrabbedRelease};
use crate::organize;
use crate::pipeline::TransferProgress;
use crate::tui::transfer::TransferTracker;
use anyhow::Result;

/// `nagare downloads`
pub fn downloads(cfg: &Config, db: &Db, json: bool) -> Result<()> {
    let mut tracker = TransferTracker::default();
    let active = tracker.poll();
    let releases = db.releases(100)?;

    if json {
        print_json(&active, &releases, cfg)?;
    } else {
        print_human(&active, &releases, cfg);
    }
    Ok(())
}

fn print_json(
    active: &Option<TransferProgress>,
    releases: &[GrabbedRelease],
    cfg: &Config,
) -> Result<()> {
    let items: Vec<serde_json::Value> = releases
        .iter()
        .map(|rel| {
            let on_disk = organize::exists_on_disk(
                &cfg.library_dir,
                &rel.show_title,
                rel.season,
                rel.episode,
            );
            let path = organize::expected_path(
                &cfg.library_dir,
                &rel.show_title,
                rel.season,
                rel.episode,
            );
            serde_json::json!({
                "show": rel.show_title,
                "season": rel.season,
                "episode": rel.episode,
                "bytes": rel.bytes,
                "grabbed_at": rel.grabbed_at,
                "on_disk": on_disk,
                "path": path.display().to_string(),
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::json!({
            "active_transfer": active,
            "downloads": items,
        })
    );
    Ok(())
}

fn print_human(
    active: &Option<TransferProgress>,
    releases: &[GrabbedRelease],
    cfg: &Config,
) {
    if let Some(t) = active {
        let speed = t.speed_bps as f64 / (1024.0 * 1024.0);
        let dl = t.downloaded_bytes as f64 / (1024.0 * 1024.0);
        let tot = t.total_bytes as f64 / (1024.0 * 1024.0);
        println!(
            "⏬ DOWNLOADING: {} • {:.1}% ({:.1}/{:.1} MiB) • {:.1} MB/s • ETA {}s",
            t.show, t.percent, dl, tot, speed, t.eta_seconds
        );
        println!();
    }

    if releases.is_empty() {
        println!("No downloaded episodes found.");
        return;
    }

    println!("{:<32} {:<8} {:<10} {:<18} STATUS", "SHOW", "S/E", "SIZE", "GRABBED");
    for rel in releases {
        let on_disk = organize::exists_on_disk(
            &cfg.library_dir,
            &rel.show_title,
            rel.season,
            rel.episode,
        );
        let status = if on_disk { "✔ on disk" } else { "✖ missing" };
        let se = format!("S{:02}E{:02}", rel.season, rel.episode as u32);
        let size = format_size(rel.bytes);
        let date = rel.grabbed_at.chars().take(16).collect::<String>();
        let show = if rel.show_title.len() > 30 {
            format!("{}...", &rel.show_title[..27])
        } else {
            rel.show_title.clone()
        };
        println!("{:<32} {:<8} {:<10} {:<18} {}", show, se, size, date, status);
    }
}

fn format_size(bytes: u64) -> String {
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gib >= 1.0 {
        format!("{gib:.2} GiB")
    } else {
        let mib = bytes as f64 / (1024.0 * 1024.0);
        format!("{mib:.1} MiB")
    }
}
