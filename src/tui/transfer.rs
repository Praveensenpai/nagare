//! Background transfer monitor and tracker.

use crate::pipeline::{TransferProgress, TransferStage};
use serde::Deserialize;
use std::fs;
use std::time::Instant;

#[derive(Debug, Deserialize)]
struct ChunkRange {
    downloaded: u64,
}

#[derive(Debug, Deserialize)]
struct DownloadState {
    total_size: u64,
    chunks: Vec<ChunkRange>,
}

/// Tracks byte throughput to calculate speed and estimated time remaining.
#[derive(Debug)]
pub struct TransferTracker {
    last_file: Option<String>,
    last_bytes: u64,
    last_time: Instant,
    smoothed_speed: u64,
}

impl Default for TransferTracker {
    fn default() -> Self {
        Self {
            last_file: None,
            last_bytes: 0,
            last_time: Instant::now(),
            smoothed_speed: 0,
        }
    }
}

impl TransferTracker {
    /// Polls the filesystem and systemd to determine current background transfer status.
    pub fn poll(&mut self) -> Option<TransferProgress> {
        if let Some((file_name, total_size, downloaded)) = read_active_state() {
            let speed = self.update_speed(&file_name, downloaded);
            let percent = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            let eta = if speed > 0 && total_size > downloaded {
                (total_size - downloaded) / speed
            } else {
                0
            };
            let show = clean_show_name(&file_name);
            Some(TransferProgress {
                show,
                file_name,
                stage: TransferStage::Downloading,
                downloaded_bytes: downloaded,
                total_bytes: total_size,
                speed_bps: speed,
                eta_seconds: eta,
                percent,
            })
        } else if is_service_active() {
            Some(TransferProgress {
                show: "Background Daemon".into(),
                file_name: "Checking feed & Seedr cloud...".into(),
                stage: TransferStage::Caching,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed_bps: 0,
                eta_seconds: 0,
                percent: 0.0,
            })
        } else {
            self.last_file = None;
            self.last_bytes = 0;
            None
        }
    }

    fn update_speed(&mut self, file_name: &str, downloaded: u64) -> u64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_time).as_secs_f64();

        if self.last_file.as_deref() != Some(file_name) || elapsed > 5.0 {
            self.last_file = Some(file_name.to_string());
            self.last_bytes = downloaded;
            self.last_time = now;
            self.smoothed_speed = 0;
            return 0;
        }

        if elapsed >= 0.5 {
            let delta = downloaded.saturating_sub(self.last_bytes);
            let instant_speed = (delta as f64 / elapsed) as u64;
            self.smoothed_speed = if self.smoothed_speed == 0 {
                instant_speed
            } else {
                ((self.smoothed_speed as f64 * 0.7) + (instant_speed as f64 * 0.3)) as u64
            };
            self.last_bytes = downloaded;
            self.last_time = now;
        }
        self.smoothed_speed
    }
}

fn read_active_state() -> Option<(String, u64, u64)> {
    let dir = dirs::cache_dir()?.join("seedr-dl/downloads");
    let entries = fs::read_dir(dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let is_state = path.extension().is_some_and(|ext| ext == "json")
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".state.json"));
        if is_state {
            let content = fs::read_to_string(&path).ok()?;
            let state: DownloadState = serde_json::from_str(&content).ok()?;
            let downloaded: u64 = state.chunks.iter().map(|c| c.downloaded).sum();
            let base_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .trim_end_matches(".state.json")
                .to_string();
            return Some((base_name, state.total_size, downloaded));
        }
    }
    None
}

/// Checks whether the background systemd service is active.
pub fn is_service_active() -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "nagare.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Triggers the background systemd service or detached worker process.
pub fn trigger_background_download() -> std::io::Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["--user", "start", "nagare.service"])
        .status();

    if let Ok(s) = status {
        if s.success() {
            return Ok(());
        }
    }

    std::process::Command::new("nagare")
        .arg("run")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

fn clean_show_name(file_name: &str) -> String {
    let clean = file_name
        .trim_start_matches('[')
        .split(']')
        .nth(1)
        .unwrap_or(file_name)
        .trim();
    clean
        .split(" - ")
        .next()
        .unwrap_or(clean)
        .trim()
        .to_string()
}
