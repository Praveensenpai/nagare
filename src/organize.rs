//! Library placement and Jellyfin refresh.

use crate::config::Jellyfin;
use crate::feed::Item;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const VIDEO_EXTENSIONS: [&str; 4] = ["mkv", "mp4", "avi", "m4v"];

/// Destination metadata for one episode.
#[derive(Debug, Clone)]
pub struct Placement {
    /// Sanitized show directory name.
    pub show: String,
    /// Season number.
    pub season: u32,
    /// Episode number.
    pub episode: u32,
}

/// Computes the placement for a feed item.
#[must_use]
pub fn plan(item: &Item) -> Placement {
    Placement {
        show: sanitize(&item.show),
        season: item.season,
        episode: item.episode.unwrap_or(0.0) as u32,
    }
}

/// Moves the largest video file into the library and cleans staging.
///
/// # Errors
/// Returns an error when no video exists, the target cannot be created, or the
/// move fails.
pub async fn place(
    library: &Path,
    staging: &Path,
    placement: &Placement,
    files: &[PathBuf],
) -> Result<PathBuf> {
    let source = pick_video(files).context("no video file in download")?;
    let directory = library
        .join(&placement.show)
        .join(format!("Season {:02}", placement.season));
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mkv");
    let target = directory.join(format!(
        "{} - S{:02}E{:02}.{extension}",
        placement.show, placement.season, placement.episode
    ));
    move_file(source, &target).await?;
    cleanup(source, staging);
    Ok(target)
}

fn pick_video(files: &[PathBuf]) -> Option<&PathBuf> {
    files
        .iter()
        .filter(|path| is_video(path))
        .max_by_key(|path| std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0))
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_lowercase)
        .is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext.as_str()))
}

async fn move_file(source: &Path, target: &Path) -> Result<()> {
    if std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    std::fs::copy(source, target)
        .with_context(|| format!("failed to copy to {}", target.display()))?;
    std::fs::remove_file(source).ok();
    Ok(())
}

fn cleanup(source: &Path, staging: &Path) {
    if let Some(parent) = source.parent() {
        if parent != staging && parent.starts_with(staging) {
            std::fs::remove_dir_all(parent).ok();
        }
    }
}

/// Asks Jellyfin to rescan its libraries when configured.
///
/// # Errors
/// Returns an error when the configured server is unreachable.
pub async fn refresh_jellyfin(config: &Jellyfin) -> Result<()> {
    let (Some(url), Some(key)) = (&config.url, &config.api_key) else {
        return Ok(());
    };
    let endpoint = format!("{}/Library/Refresh", url.trim_end_matches('/'));
    reqwest::Client::new()
        .post(endpoint)
        .header("X-Emby-Token", key)
        .send()
        .await
        .context("failed to trigger Jellyfin refresh")?;
    Ok(())
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch == '\0' {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Unknown Show".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_path_separators() {
        assert_eq!(sanitize("A/B"), "A_B");
        assert_eq!(sanitize("  "), "Unknown Show");
    }

    #[test]
    fn plan_uses_parsed_numbering() {
        let item = Item {
            infohash: "H".into(),
            title: "t".into(),
            show: "Frieren S2".into(),
            season: 2,
            episode: Some(5.0),
            bytes: 1,
            magnet: "magnet:?xt=urn:btih:H".into(),
            released: chrono::Utc::now(),
            is_batch: false,
        };
        let placement = plan(&item);
        assert_eq!(placement.show, "Frieren S2");
        assert_eq!(placement.season, 2);
        assert_eq!(placement.episode, 5);
    }
}
