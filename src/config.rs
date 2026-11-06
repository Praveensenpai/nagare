//! User configuration loaded from `~/.config/nagare/config.toml`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default SubsPlease 1080p feed.
pub const DEFAULT_FEED: &str = "https://subsplease.org/rss/?r=1080";

/// Where finished episodes land and how the cloud behaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Jellyfin library root for anime.
    #[serde(default = "default_library")]
    pub library_dir: PathBuf,
    /// RSS feed polled for releases.
    #[serde(default = "default_feed")]
    pub feed_url: String,
    /// Staging directory for downloads before they are moved.
    #[serde(default = "default_staging")]
    pub staging_dir: PathBuf,
    /// Minimum free Seedr bytes before a new magnet is accepted.
    #[serde(default = "default_min_free")]
    pub min_free_bytes: u64,
    /// Ceiling on how long Seedr may spend caching one torrent.
    #[serde(default = "default_max_wait")]
    pub max_wait_secs: u64,
    /// Parallel download threads used inside Seedr transfers.
    #[serde(default = "default_threads")]
    pub download_threads: usize,
    /// Optional Jellyfin server used to trigger a library scan.
    #[serde(default)]
    pub jellyfin: Jellyfin,
    /// Whether to send Telegram events through tayori.
    #[serde(default = "default_true")]
    pub notify: bool,
}

/// Jellyfin scan trigger settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Jellyfin {
    /// Base URL, for example `http://127.0.0.1:8096`.
    pub url: Option<String>,
    /// API key with library refresh permission.
    pub api_key: Option<String>,
}

fn default_library() -> PathBuf {
    home().join("Videos/Anime")
}

fn default_staging() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| home().join(".cache"))
        .join("nagare/incoming")
}

fn default_feed() -> String {
    DEFAULT_FEED.to_string()
}

const fn default_min_free() -> u64 {
    2 * 1024 * 1024 * 1024
}

const fn default_max_wait() -> u64 {
    1_800
}

const fn default_threads() -> usize {
    2
}

const fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_dir: default_library(),
            feed_url: default_feed(),
            staging_dir: default_staging(),
            min_free_bytes: default_min_free(),
            max_wait_secs: default_max_wait(),
            download_threads: default_threads(),
            jellyfin: Jellyfin::default(),
            notify: true,
        }
    }
}

/// `~/.config/nagare` directory.
#[must_use]
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join("nagare")
}

/// `~/.local/state/nagare` directory.
#[must_use]
pub fn state_dir() -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| home().join(".local/state"))
        .join("nagare")
}

/// Loads the config file, falling back to defaults when absent.
///
/// # Errors
/// Returns an error if the file exists but cannot be read or parsed.
pub fn load() -> Result<Config> {
    let path = config_dir().join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
}
