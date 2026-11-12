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
    assert_eq!(Tab::Feed.next(), Tab::Downloads);
    assert_eq!(Tab::Downloads.next(), Tab::Activity);
    assert_eq!(Tab::Activity.next(), Tab::Tracked);

    assert_eq!(Tab::Tracked.prev(), Tab::Activity);
    assert_eq!(Tab::Activity.prev(), Tab::Downloads);
    assert_eq!(Tab::Downloads.prev(), Tab::Feed);
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

#[test]
fn track_prompt_options() {
    let mut app = test_app();
    app.feed_items = vec![Item {
        title: "[SubsPlease] Satou-san - 05 (1080p).mkv".into(),
        show: "Satou-san".into(),
        season: 1,
        episode: Some(5.0),
        bytes: 1_000_000,
        infohash: "hash1".into(),
        is_batch: false,
        magnet: "magnet:?".into(),
        released: chrono::Utc::now(),
    }];
    app.filtered_feed = vec![0];
    app.selected_feed = 0;

    // Open prompt
    app.open_track_prompt().expect("open track prompt");
    assert_eq!(app.input_mode, InputMode::TrackPrompt);
    let prompt = app.track_prompt.as_ref().expect("has prompt");
    assert_eq!(prompt.show, "Satou-san");
    assert_eq!(prompt.episode, 5.0);

    // Option 0: Include current -> watermark = 4.0
    app.apply_track_prompt(0).expect("apply option 0");
    assert_eq!(app.input_mode, InputMode::Normal);
    assert_eq!(app.shows.len(), 1);
    assert_eq!(app.shows[0].title, "Satou-san");
    assert_eq!(app.shows[0].max_episode, 4.0);

    // Calling again untracks the show
    app.open_track_prompt().expect("toggle untrack");
    assert!(app.shows.is_empty());
}

#[test]
fn feed_state_initializes_with_none_or_selected() {
    let app = test_app();
    assert_eq!(app.selected_feed, 0);
    assert_eq!(app.selected_show, 0);
    assert_eq!(app.selected_release, 0);
    assert_eq!(app.selected_event, 0);
    assert_eq!(app.feed_state.selected(), None);
}

#[test]
fn events_reload_clamps_selected_event() {
    let mut app = test_app();
    app.selected_event = 10;
    app.reload_events();
    assert_eq!(app.selected_event, 0);
}

#[test]
fn feed_scrolling_tracks_deep_selection() {
    let mut app = test_app();
    app.active_tab = Tab::Feed;
    app.feed_items = (0..50)
        .map(|i| Item {
            title: format!("[SubsPlease] Show {i} - 01 (1080p).mkv"),
            show: format!("Show {i}"),
            season: 1,
            episode: Some(1.0),
            bytes: 1_000_000,
            infohash: format!("hash{i}"),
            is_batch: false,
            magnet: "magnet:?".into(),
            released: chrono::Utc::now(),
        })
        .collect();
    app.filtered_feed = (0..50).collect();
    app.selected_feed = 35;

    let backend = ratatui::backend::TestBackend::new(100, 25);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| crate::tui::ui::draw(f, &mut app))
        .expect("draw scrolled feed");
    assert_eq!(app.feed_state.selected(), Some(35));
    assert!(app.feed_state.offset() > 0);
}
