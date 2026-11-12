//! Application state machine for the nagare interactive TUI.

use crate::config::Config;
use crate::db::{Db, Event, Show};
use crate::feed::{self, Item};
use crate::matcher;
use crate::pipeline::TransferProgress;
use anyhow::Result;
use ratatui::widgets::TableState;
use std::time::Instant;

/// Active top-level view tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Tracked anime shows table.
    Tracked = 0,
    /// Live SubsPlease release stream.
    Feed = 1,
    /// Downloaded episodes and active transfer.
    Downloads = 2,
    /// Seedr cloud and recent audit log.
    Activity = 3,
}

impl Tab {
    /// Cycles to the next tab.
    pub fn next(self) -> Self {
        match self {
            Self::Tracked => Self::Feed,
            Self::Feed => Self::Downloads,
            Self::Downloads => Self::Activity,
            Self::Activity => Self::Tracked,
        }
    }

    /// Cycles to the previous tab.
    pub fn prev(self) -> Self {
        match self {
            Self::Tracked => Self::Activity,
            Self::Feed => Self::Tracked,
            Self::Downloads => Self::Feed,
            Self::Activity => Self::Downloads,
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
    /// Modal dialog open to confirm tracking a feed release.
    TrackPrompt,
}

/// State for the Track Show confirmation popup.
#[derive(Debug, Clone)]
pub struct TrackPrompt {
    pub show: String,
    pub season: u32,
    pub episode: f64,
    pub selected: usize,
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
    pub releases: Vec<crate::db::GrabbedRelease>,
    pub selected_release: usize,
    pub selected_event: usize,
    pub tracked_state: TableState,
    pub feed_state: TableState,
    pub downloads_state: TableState,
    pub activity_state: TableState,
    pub seedr_space: Option<(u64, u64)>,
    pub filter_input: String,
    pub add_title: String,
    pub add_season: String,
    pub add_episode: String,
    pub add_focus: AddField,
    pub track_prompt: Option<TrackPrompt>,
    pub active_transfer: Option<TransferProgress>,
    pub next_scan: Option<String>,
    pub show_help: bool,
    pub status_msg: Option<(String, Instant)>,
    pub should_quit: bool,
}

fn clamp_idx(idx: &mut usize, len: usize) {
    if len == 0 {
        *idx = 0;
    } else if *idx >= len {
        *idx = len - 1;
    }
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
            releases: Vec::new(),
            selected_release: 0,
            selected_event: 0,
            tracked_state: TableState::default(),
            feed_state: TableState::default(),
            downloads_state: TableState::default(),
            activity_state: TableState::default(),
            seedr_space: None,
            filter_input: String::new(),
            add_title: String::new(),
            add_season: "1".into(),
            add_episode: "0".into(),
            add_focus: AddField::Title,
            track_prompt: None,
            active_transfer: None,
            next_scan: crate::tui::transfer::timer_next_run(),
            show_help: false,
            status_msg: None,
            should_quit: false,
        };
        app.reload_shows();
        app.reload_releases();
        app.reload_events();
        app
    }

    /// Reloads tracked shows from SQLite.
    pub fn reload_shows(&mut self) {
        if let Ok(shows) = self.db.shows() {
            clamp_idx(&mut self.selected_show, shows.len());
            self.shows = shows;
        }
    }

    /// Reloads grabbed releases from SQLite.
    pub fn reload_releases(&mut self) {
        if let Ok(releases) = self.db.releases(100) {
            clamp_idx(&mut self.selected_release, releases.len());
            self.releases = releases;
        }
    }

    /// Reloads recent audit log events from SQLite.
    pub fn reload_events(&mut self) {
        if let Ok(events) = self.db.recent_events(50) {
            clamp_idx(&mut self.selected_event, events.len());
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
        (when.elapsed().as_secs() < 5).then_some(msg.as_str())
    }

    /// Updates the filtered indices when feed items or filter text changes.
    pub fn update_filter(&mut self) {
        let needle = matcher::normalize(&self.filter_input);
        self.filtered_feed = self
            .feed_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                (needle.is_empty() || matcher::normalize(&item.show).contains(&needle))
                    .then_some(idx)
            })
            .collect();
        clamp_idx(&mut self.selected_feed, self.filtered_feed.len());
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

    /// Opens the track episode prompt for currently selected feed item.
    pub fn open_track_prompt(&mut self) -> Result<()> {
        let Some(&idx) = self.filtered_feed.get(self.selected_feed) else {
            return Ok(());
        };
        let Some(item) = self.feed_items.get(idx) else {
            return Ok(());
        };
        let name = item.show.clone();

        if let Some(existing) = self.shows.iter().find(|s| {
            s.title.eq_ignore_ascii_case(&name) || s.match_title.eq_ignore_ascii_case(&name)
        }) {
            let id = existing.id;
            self.db.remove_show(id)?;
            self.db.log_event("untrack", &format!("untracked {name}"))?;
            self.set_status(format!("Untracked {name}"));
            self.reload_shows();
            self.reload_events();
            return Ok(());
        }

        let season = item.season;
        let episode = item.episode.unwrap_or(1.0);
        self.track_prompt = Some(TrackPrompt {
            show: name,
            season,
            episode,
            selected: 0,
        });
        self.input_mode = InputMode::TrackPrompt;
        Ok(())
    }

    /// Executes the selected option from the track episode prompt.
    pub fn apply_track_prompt(&mut self, choice_idx: usize) -> Result<()> {
        let Some(prompt) = self.track_prompt.take() else {
            self.input_mode = InputMode::Normal;
            return Ok(());
        };

        let name = prompt.show;
        let season = prompt.season;
        let episode = prompt.episode;

        match choice_idx {
            0 => {
                let wm = (episode - 1.0).max(0.0);
                self.db.add_show(&name, &name, season, wm)?;
                self.db.log_event(
                    "track",
                    &format!("{name} including S{season:02}E{:02}", episode as u32),
                )?;
                self.set_status(format!(
                    "Tracking {name} (from S{season:02}E{:02})",
                    episode as u32
                ));
                self.input_mode = InputMode::Normal;
            }
            1 => {
                self.db.add_show(&name, &name, season, episode)?;
                self.db.log_event(
                    "track",
                    &format!("{name} after S{season:02}E{:02}", episode as u32),
                )?;
                self.set_status(format!(
                    "Tracking {name} (after S{season:02}E{:02})",
                    episode as u32
                ));
                self.input_mode = InputMode::Normal;
            }
            2 => {
                self.db.add_show(&name, &name, season, 0.0)?;
                self.db
                    .log_event("track", &format!("{name} full season {season}"))?;
                self.set_status(format!("Tracking {name} (S{season:02} complete)"));
                self.input_mode = InputMode::Normal;
            }
            3 => {
                self.add_title = name;
                self.add_season = season.to_string();
                self.add_episode = ((episode - 1.0).max(0.0) as u32).to_string();
                self.add_focus = AddField::Episode;
                self.input_mode = InputMode::AddingShow;
            }
            _ => self.input_mode = InputMode::Normal,
        }
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

    /// Triggers a non-blocking background pipeline scan and grab run.
    pub fn trigger_background_pipeline(&mut self) {
        if let Err(err) = crate::tui::transfer::trigger_background_download() {
            self.set_status(format!("Failed to trigger background daemon: {err}"));
        } else {
            self.set_status("Started background scheduler (nagare.service)");
        }
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
#[path = "app_tests.rs"]
mod tests;
