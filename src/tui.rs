//! Interactive terminal user interface for nagare.

pub mod activity;
pub mod app;
pub mod modals;
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
use std::time::Duration;

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
    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, app))?;
        poll_and_handle_input(app).await?;
    }
    Ok(())
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
        InputMode::Filtering => handle_filter_key(app, key),
        InputMode::Normal => handle_normal_key(app, key).await,
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
            app.input_mode = InputMode::Normal;
        }
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

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Tab => app.active_tab = app.active_tab.next(),
        KeyCode::BackTab => app.active_tab = app.active_tab.prev(),
        KeyCode::Char('1') => app.active_tab = Tab::Tracked,
        KeyCode::Char('2') => app.active_tab = Tab::Feed,
        KeyCode::Char('3') => app.active_tab = Tab::Activity,
        KeyCode::Char('j') | KeyCode::Down => move_selection(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_selection(app, -1),
        KeyCode::Char('g') | KeyCode::Home => jump_selection(app, false),
        KeyCode::Char('G') | KeyCode::End => jump_selection(app, true),
        KeyCode::Char('a') => app.input_mode = InputMode::AddingShow,
        KeyCode::Char('d') if app.active_tab == Tab::Tracked => app.remove_selected_show()?,
        KeyCode::Char('t') if app.active_tab == Tab::Feed => app.track_selected_feed()?,
        KeyCode::Char('/') => {
            app.active_tab = Tab::Feed;
            app.input_mode = InputMode::Filtering;
        }
        KeyCode::Char('r') => {
            let _ = app.run_pipeline().await;
        }
        _ => {}
    }
    Ok(())
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
        Tab::Activity => {}
    }
}
