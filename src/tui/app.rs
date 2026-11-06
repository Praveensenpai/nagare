//! Application state machine for the nagare interactive TUI.

use crate::config::Config;
use crate::db::{Db, Event, Show};
use crate::feed::{self, Item};
use crate::matcher;
use crate::pipeline::{self, Report};
use anyhow::Result;
use std::time::Instant;

/// Active top-level view tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Tracked anime shows table.
    Tracked = 0,
    /// Live SubsPlease release stream.
    Feed = 1,
    /// Seedr cloud and recent audit log.
    Activity = 2,
}

impl Tab {
    /// Cycles to the next tab.
    pub fn next(self) -> Self {
        match self {
            Self::Tracked => Self::Feed,
            Self::Feed => Self::Activity,
            Self::Activity => Self::Tracked,
        }
    }

    /// Cycles to the previous tab.
    pub fn prev(self) -> Self {
        match self {
            Self::Tracked => Self::Activity,
            Self::Feed => Self::Tracked,
            Self::Activity => Self::Feed,
        }
    }
}

/// Keyboard focus and input modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Standard keybindings active.
    Normal,
    /// Live filter input active on Feed tab.
    Filtering,
    /// Modal dialog open to add a custom show.
    AddingShow,
}

/// Focused field inside the Add Show dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Title,
    Season,
    Episode,
}

impl AddField {
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Season,
            Self::Season => Self::Episode,
            Self::Episode => Self::Title,
        }
    }
}

/// Core application state passed to UI renderers and event handlers.
pub struct App {
    pub config: Config,
    pub db: Db,
    pub active_tab: Tab,
    pub input_mode: InputMode,
    pub shows: Vec<Show>,
    pub selected_show: usize,
    pub feed_items: Vec<Item>,
    pub filtered_feed: Vec<usize>,
    pub selected_feed: usize,
    pub feed_loading: bool,
    pub events: Vec<Event>,
    pub seedr_space: Option<(u64, u64)>,
    pub filter_input: String,
    pub add_title: String,
    pub add_season: String,
    pub add_episode: String,
    pub add_focus: AddField,
    pub show_help: bool,
    pub status_msg: Option<(String, Instant)>,
    pub should_quit: bool,
    pub is_running_pipeline: bool,
}

impl App {
    /// Creates and initializes the app state.
    pub fn new(config: Config, db: Db) -> Self {
        let mut app = Self {
            config,
            db,
            active_tab: Tab::Tracked,
            input_mode: InputMode::Normal,
            shows: Vec::new(),
            selected_show: 0,
            feed_items: Vec::new(),
            filtered_feed: Vec::new(),
            selected_feed: 0,
            feed_loading: false,
            events: Vec::new(),
            seedr_space: None,
            filter_input: String::new(),
            add_title: String::new(),
            add_season: "1".into(),
            add_episode: "0".into(),
            add_focus: AddField::Title,
            show_help: false,
            status_msg: None,
            should_quit: false,
            is_running_pipeline: false,
        };
        app.reload_shows();
        app.reload_events();
        app
    }

    /// Reloads tracked shows from SQLite.
    pub fn reload_shows(&mut self) {
        if let Ok(shows) = self.db.shows() {
            self.shows = shows;
            if self.selected_show >= self.shows.len() && !self.shows.is_empty() {
                self.selected_show = self.shows.len() - 1;
            }
        }
    }

    /// Reloads recent audit log events from SQLite.
    pub fn reload_events(&mut self) {
        if let Ok(events) = self.db.recent_events(30) {
            self.events = events;
        }
    }

    /// Sets a temporary status toast with auto-expiry.
    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status_msg = Some((text.into(), Instant::now()));
    }

    /// Returns active status message if within its 5-second display window.
    pub fn active_status(&self) -> Option<&str> {
        let (msg, when) = self.status_msg.as_ref()?;
        if when.elapsed().as_secs() < 5 {
            Some(msg.as_str())
        } else {
            None
        }
    }

    /// Updates the filtered indices when feed items or filter text changes.
    pub fn update_filter(&mut self) {
        let needle = matcher::normalize(&self.filter_input);
        self.filtered_feed = self
            .feed_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                if needle.is_empty() || matcher::normalize(&item.show).contains(&needle) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        if self.selected_feed >= self.filtered_feed.len() && !self.filtered_feed.is_empty() {
            self.selected_feed = self.filtered_feed.len() - 1;
        }
    }

    /// Removes currently selected show from database.
    pub fn remove_selected_show(&mut self) -> Result<()> {
        if let Some(show) = self.shows.get(self.selected_show) {
            let id = show.id;
            let name = show.title.clone();
            self.db.remove_show(id)?;
            self.db.log_event("untrack", &format!("removed {name}"))?;
            self.set_status(format!("Untracked {name}"));
            self.reload_shows();
            self.reload_events();
        }
        Ok(())
    }

    /// Tracks the show corresponding to currently selected feed item.
    pub fn track_selected_feed(&mut self) -> Result<()> {
        let Some(&idx) = self.filtered_feed.get(self.selected_feed) else {
            return Ok(());
        };
        let Some(item) = self.feed_items.get(idx) else {
            return Ok(());
        };
        let name = item.show.clone();
        let season = item.season;
        let episode = item.episode.unwrap_or(0.0);

        self.db.add_show(&name, &name, season, episode)?;
        self.db.log_event(
            "track",
            &format!("{name} after S{season:02}E{:02}", episode as u32),
        )?;
        self.set_status(format!("Tracking {name} (S{season:02}E{episode:02})"));
        self.reload_shows();
        self.reload_events();
        Ok(())
    }

    /// Submits the Add Show modal form.
    pub fn submit_add_modal(&mut self) -> Result<()> {
        let title = self.add_title.trim();
        if title.is_empty() {
            self.set_status("Show title cannot be empty");
            return Ok(());
        }
        let season: u32 = self.add_season.trim().parse().unwrap_or(1);
        let episode: f64 = self.add_episode.trim().parse().unwrap_or(0.0);

        self.db.add_show(title, title, season, episode)?;
        self.db.log_event(
            "track",
            &format!("{title} after S{season:02}E{:02}", episode as u32),
        )?;
        self.set_status(format!("Added {title}"));
        self.add_title.clear();
        self.add_season = "1".into();
        self.add_episode = "0".into();
        self.input_mode = InputMode::Normal;
        self.reload_shows();
        self.reload_events();
        Ok(())
    }

    /// Triggers a manual pipeline scan and grab run.
    pub async fn run_pipeline(&mut self) -> Result<Report> {
        self.is_running_pipeline = true;
        self.set_status("Scanning feed & running pipeline...");
        let res = pipeline::run(&self.config, &mut self.db, false).await;
        self.is_running_pipeline = false;
        self.reload_shows();
        self.reload_events();
        res
    }

    /// Fetches the live feed items.
    pub async fn fetch_feed(&mut self) -> Result<()> {
        self.feed_loading = true;
        let outcome = feed::fetch(&self.config.feed_url, None, None).await;
        self.feed_loading = false;
        match outcome {
            Ok(res) => {
                self.feed_items = res.items;
                self.update_filter();
                self.set_status(format!("Feed refreshed ({} items)", self.feed_items.len()));
            }
            Err(err) => {
                self.set_status(format!("Feed fetch failed: {err}"));
            }
        }
        Ok(())
    }

    /// Fetches Seedr storage capacity.
    pub async fn fetch_space(&mut self) {
        if let Ok(Some(auth)) = seedr_dl::load_auth() {
            let client = seedr_dl::SeedrClient::new(auth.access_token);
            if let Ok(space) = client.space().await {
                self.seedr_space = Some((space.max, space.used));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn test_app() -> App {
        let db = Db::open_at(Path::new(":memory:")).expect("memory db");
        let config = Config {
            library_dir: PathBuf::from("/tmp/library"),
            feed_url: "https://example.com/rss".into(),
            staging_dir: PathBuf::from("/tmp/staging"),
            min_free_bytes: 1024,
            max_wait_secs: 60,
            download_threads: 1,
            jellyfin: Default::default(),
            notify: false,
        };
        App::new(config, db)
    }

    #[test]
    fn tab_navigation_cycles() {
        assert_eq!(Tab::Tracked.next(), Tab::Feed);
        assert_eq!(Tab::Feed.next(), Tab::Activity);
        assert_eq!(Tab::Activity.next(), Tab::Tracked);

        assert_eq!(Tab::Tracked.prev(), Tab::Activity);
        assert_eq!(Tab::Activity.prev(), Tab::Feed);
        assert_eq!(Tab::Feed.prev(), Tab::Tracked);
    }

    #[test]
    fn add_field_cycles() {
        assert_eq!(AddField::Title.next(), AddField::Season);
        assert_eq!(AddField::Season.next(), AddField::Episode);
        assert_eq!(AddField::Episode.next(), AddField::Title);
    }

    #[test]
    fn app_add_and_remove_show() {
        let mut app = test_app();
        assert!(app.shows.is_empty());

        app.add_title = "Spy x Family".into();
        app.add_season = "2".into();
        app.add_episode = "12".into();
        app.submit_add_modal().expect("submit add");

        assert_eq!(app.shows.len(), 1);
        assert_eq!(app.shows[0].title, "Spy x Family");
        assert_eq!(app.shows[0].season, 2);
        assert_eq!(app.shows[0].max_episode, 12.0);

        app.remove_selected_show().expect("remove show");
        assert!(app.shows.is_empty());
    }

    #[test]
    fn filter_updates_indices() {
        let mut app = test_app();
        app.feed_items = vec![
            Item {
                title: "[SubsPlease] One Piece - 1089 (1080p).mkv".into(),
                show: "One Piece".into(),
                season: 1,
                episode: Some(1089.0),
                bytes: 1_000_000,
                infohash: "hash1".into(),
                is_batch: false,
                magnet: "magnet:?".into(),
                released: chrono::Utc::now(),
            },
            Item {
                title: "[SubsPlease] Bleach - 24 (1080p).mkv".into(),
                show: "Bleach".into(),
                season: 1,
                episode: Some(24.0),
                bytes: 1_000_000,
                infohash: "hash2".into(),
                is_batch: false,
                magnet: "magnet:?".into(),
                released: chrono::Utc::now(),
            },
        ];

        app.filter_input = "piece".into();
        app.update_filter();
        assert_eq!(app.filtered_feed.len(), 1);
        assert_eq!(app.filtered_feed[0], 0);

        app.filter_input = "bleach".into();
        app.update_filter();
        assert_eq!(app.filtered_feed.len(), 1);
        assert_eq!(app.filtered_feed[0], 1);

        app.filter_input.clear();
        app.update_filter();
        assert_eq!(app.filtered_feed.len(), 2);
    }
}
