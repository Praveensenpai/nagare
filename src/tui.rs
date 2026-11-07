//! Interactive terminal user interface for nagare.

pub mod activity;
pub mod app;
pub mod downloads;
pub mod modals;
pub mod transfer;
pub mod ui;
pub mod views;

use crate::config::Config;
use crate::db::Db;
use anyhow::{bail, Context, Result};
use app::{AddField, App, InputMode, Tab};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io::{stdout, IsTerminal, Stdout};
use std::time::{Duration, Instant};

/// RAII guard to ensure the terminal is restored even during panics or early returns.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

/// Runs the interactive TUI until the user quits.
pub async fn run(config: Config, db: Db) -> Result<()> {
    if !stdout().is_terminal() {
        bail!("interactive TUI requires a TTY terminal");
    }

    enable_raw_mode().context("failed to enable terminal raw mode")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let _guard = TerminalGuard;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fullscreen,
        },
    )
    .context("failed to create ratatui terminal")?;

    let mut app = App::new(config, db);
    app.fetch_space().await;
    let _ = app.fetch_feed().await;

    event_loop(&mut terminal, &mut app).await
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut tracker = transfer::TransferTracker::default();
    let mut last_db_poll = Instant::now();
    let mut last_space_poll = Instant::now();
    let (space_tx, mut space_rx) = tokio::sync::mpsc::unbounded_channel();

    while !app.should_quit {
        if let Ok(space) = space_rx.try_recv() {
            app.seedr_space = space;
        }

        let prev_active = app.active_transfer.is_some();
        app.active_transfer = tracker.poll();
        let now = Instant::now();

        if (prev_active && app.active_transfer.is_none())
            || (!prev_active && app.active_transfer.is_some())
            || now.duration_since(last_db_poll).as_secs() >= 1
        {
            app.reload_shows();
            app.reload_releases();
            app.reload_events();
            last_db_poll = now;
        }

        if now.duration_since(last_space_poll).as_secs() >= 10 {
            spawn_space_check(&space_tx);
            last_space_poll = now;
        }

        terminal.draw(|f| ui::draw(f, app))?;
        poll_and_handle_input(app).await?;
    }
    Ok(())
}

fn spawn_space_check(tx: &tokio::sync::mpsc::UnboundedSender<Option<(u64, u64)>>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Ok(Some(auth)) = seedr_dl::load_auth() {
            let client = seedr_dl::SeedrClient::new(auth.access_token);
            if let Ok(space) = client.space().await {
                let _ = tx.send(Some((space.max, space.used)));
            }
        }
    });
}

async fn poll_and_handle_input(app: &mut App) -> Result<()> {
    if !event::poll(Duration::from_millis(100))? {
        return Ok(());
    }
    let Event::Key(key) = event::read()? else {
        return Ok(());
    };
    if key.kind == KeyEventKind::Press {
        handle_key(app, key).await?;
    }
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match app.input_mode {
        InputMode::AddingShow => handle_add_modal_key(app, key),
        InputMode::TrackPrompt => handle_track_prompt_key(app, key),
        InputMode::Filtering => handle_filter_key(app, key),
        InputMode::Normal => handle_normal_key(app, key).await,
    }
}

fn handle_track_prompt_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('1') => app.apply_track_prompt(0)?,
        KeyCode::Char('2') => app.apply_track_prompt(1)?,
        KeyCode::Char('3') => app.apply_track_prompt(2)?,
        KeyCode::Char('4') => app.apply_track_prompt(3)?,
        KeyCode::Up | KeyCode::Char('k') => adjust_prompt_selection(app, -1),
        KeyCode::Down | KeyCode::Char('j') => adjust_prompt_selection(app, 1),
        KeyCode::Enter => {
            let sel = app.track_prompt.as_ref().map(|p| p.selected).unwrap_or(0);
            app.apply_track_prompt(sel)?;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.track_prompt = None;
            app.input_mode = InputMode::Normal;
        }
        _ => {}
    }
    Ok(())
}

fn adjust_prompt_selection(app: &mut App, delta: isize) {
    let Some(prompt) = &mut app.track_prompt else {
        return;
    };
    if delta < 0 {
        prompt.selected = prompt.selected.saturating_sub(1);
    } else if prompt.selected < 3 {
        prompt.selected += 1;
    }
}

fn handle_add_modal_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.input_mode = InputMode::Normal,
        KeyCode::Tab => app.add_focus = app.add_focus.next(),
        KeyCode::Enter => app.submit_add_modal()?,
        KeyCode::Backspace => match app.add_focus {
            AddField::Title => {
                app.add_title.pop();
            }
            AddField::Season => {
                app.add_season.pop();
            }
            AddField::Episode => {
                app.add_episode.pop();
            }
        },
        KeyCode::Char(c) => match app.add_focus {
            AddField::Title => app.add_title.push(c),
            AddField::Season if c.is_ascii_digit() => app.add_season.push(c),
            AddField::Episode if c.is_ascii_digit() || c == '.' => app.add_episode.push(c),
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

fn handle_filter_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.filter_input.clear();
            app.update_filter();
        }
        KeyCode::Enter => {
            app.open_track_prompt()?;
        }
        KeyCode::Down => move_selection(app, 1),
        KeyCode::Up => move_selection(app, -1),
        KeyCode::Backspace => {
            app.filter_input.pop();
            app.update_filter();
        }
        KeyCode::Char(c) => {
            app.filter_input.push(c);
            app.update_filter();
        }
        _ => {}
    }
    Ok(())
}

async fn handle_normal_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.show_help {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
        ) {
            app.show_help = false;
        }
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return Ok(());
    }

    if handle_tab_navigation(app, key.code) {
        return Ok(());
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('j') | KeyCode::Down => move_selection(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_selection(app, -1),
        KeyCode::Char('g') | KeyCode::Home => jump_selection(app, false),
        KeyCode::Char('G') | KeyCode::End => jump_selection(app, true),
        KeyCode::Char('a') => app.input_mode = InputMode::AddingShow,
        KeyCode::Char('d') | KeyCode::Delete if app.active_tab == Tab::Tracked => {
            app.remove_selected_show()?;
        }
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('t') if app.active_tab == Tab::Feed => {
            app.open_track_prompt()?;
        }
        KeyCode::Enter if app.active_tab == Tab::Tracked => {
            if let Some(show) = app.shows.get(app.selected_show) {
                app.filter_input = show.title.clone();
                app.update_filter();
                app.active_tab = Tab::Feed;
            }
        }
        KeyCode::Char('/') => {
            app.active_tab = Tab::Feed;
            app.input_mode = InputMode::Filtering;
        }
        KeyCode::Char('r') => {
            app.trigger_background_pipeline();
        }
        _ => {}
    }
    Ok(())
}

fn handle_tab_navigation(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Tab => app.active_tab = app.active_tab.next(),
        KeyCode::BackTab => app.active_tab = app.active_tab.prev(),
        KeyCode::Char('1') => app.active_tab = Tab::Tracked,
        KeyCode::Char('2') => app.active_tab = Tab::Feed,
        KeyCode::Char('3') => app.active_tab = Tab::Downloads,
        KeyCode::Char('4') => app.active_tab = Tab::Activity,
        _ => return false,
    }
    app.reload_shows();
    app.reload_releases();
    app.reload_events();
    true
}

fn move_selection(app: &mut App, delta: isize) {
    match app.active_tab {
        Tab::Tracked => {
            if !app.shows.is_empty() {
                let next = (app.selected_show as isize + delta)
                    .clamp(0, app.shows.len() as isize - 1) as usize;
                app.selected_show = next;
            }
        }
        Tab::Feed => {
            if !app.filtered_feed.is_empty() {
                let next = (app.selected_feed as isize + delta)
                    .clamp(0, app.filtered_feed.len() as isize - 1)
                    as usize;
                app.selected_feed = next;
            }
        }
        Tab::Downloads => {
            if !app.releases.is_empty() {
                let next = (app.selected_release as isize + delta)
                    .clamp(0, app.releases.len() as isize - 1) as usize;
                app.selected_release = next;
            }
        }
        Tab::Activity => {}
    }
}

fn jump_selection(app: &mut App, to_end: bool) {
    match app.active_tab {
        Tab::Tracked => {
            if !app.shows.is_empty() {
                app.selected_show = if to_end { app.shows.len() - 1 } else { 0 };
            }
        }
        Tab::Feed => {
            if !app.filtered_feed.is_empty() {
                app.selected_feed = if to_end {
                    app.filtered_feed.len() - 1
                } else {
                    0
                };
            }
        }
        Tab::Downloads => {
            if !app.releases.is_empty() {
                app.selected_release = if to_end {
                    app.releases.len() - 1
                } else {
                    0
                };
            }
        }
        Tab::Activity => {}
    }
}
