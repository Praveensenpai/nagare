//! Root layout rendering for the nagare TUI.

use crate::tui::activity;
use crate::tui::app::{App, InputMode, Tab};
use crate::tui::modals;
use crate::tui::views;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Tabs},
    Frame,
};

/// Draws the complete frame layout.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();
    if size.width < 60 || size.height < 14 {
        render_too_small(frame, size);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(size);

    render_header(frame, chunks[0], app);
    render_body(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);

    if app.show_help {
        modals::render_help(frame, size);
    } else if app.input_mode == InputMode::AddingShow {
        modals::render_add_modal(frame, size, app);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(16),
            Constraint::Min(36),
            Constraint::Length(24),
        ])
        .split(area);

    let brand = Paragraph::new(" 流れ nagare ")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(brand, header_chunks[0]);

    let tab_titles = vec!["[1] Tracked", "[2] Live Feed", "[3] Activity"];
    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(app.active_tab as usize)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, header_chunks[1]);

    render_space_badge(frame, header_chunks[2], app.seedr_space);
}

fn render_space_badge(frame: &mut Frame, area: Rect, space: Option<(u64, u64)>) {
    let space_text = match space {
        Some((max, used)) => {
            let used_gib = used as f64 / (1024.0 * 1024.0 * 1024.0);
            let max_gib = max as f64 / (1024.0 * 1024.0 * 1024.0);
            format!(" Cloud: {used_gib:.1}/{max_gib:.1} GiB ")
        }
        None => " Cloud: --/-- GiB ".into(),
    };
    let space_badge = Paragraph::new(space_text)
        .alignment(Alignment::Right)
        .style(Style::default().fg(Color::LightBlue))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(space_badge, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    match app.active_tab {
        Tab::Tracked => views::render_tracked(frame, area, app),
        Tab::Feed => views::render_feed(frame, area, app),
        Tab::Activity => activity::render(frame, area, app),
    }
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let footer_text = if let Some(msg) = app.active_status() {
        Line::from(vec![
            Span::styled(" ● ", Style::default().fg(Color::Green)),
            Span::styled(
                msg,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        match app.input_mode {
            InputMode::Normal => Line::from(vec![
                Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
                Span::raw("View  "),
                Span::styled("[r] ", Style::default().fg(Color::Cyan)),
                Span::raw("Scan/Run  "),
                Span::styled("[a] ", Style::default().fg(Color::Cyan)),
                Span::raw("Add  "),
                Span::styled("[d] ", Style::default().fg(Color::Cyan)),
                Span::raw("Delete  "),
                Span::styled("[/] ", Style::default().fg(Color::Cyan)),
                Span::raw("Filter  "),
                Span::styled("[?] ", Style::default().fg(Color::Cyan)),
                Span::raw("Help  "),
                Span::styled("[q] ", Style::default().fg(Color::Yellow)),
                Span::raw("Quit"),
            ]),
            InputMode::Filtering => Line::from(vec![
                Span::styled(
                    "FILTER MODE: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Type title pattern  "),
                Span::styled("[Enter] ", Style::default().fg(Color::Green)),
                Span::raw("Apply  "),
                Span::styled("[Esc] ", Style::default().fg(Color::Yellow)),
                Span::raw("Cancel"),
            ]),
            InputMode::AddingShow => Line::from(vec![
                Span::styled(
                    "ADD SHOW: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Fill details in modal dialog"),
            ]),
        }
    };

    let paragraph = Paragraph::new(footer_text).alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

fn render_too_small(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("Terminal window too small for nagare TUI."),
        Line::from(format!(
            "Current: {}x{}, Minimum: 60x14",
            area.width, area.height
        )),
        Line::from("Please resize your terminal."),
    ];
    let p = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Db;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
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
    fn renders_all_tabs_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut app = test_app();

        app.active_tab = Tab::Tracked;
        terminal.draw(|f| draw(f, &app)).expect("draw tracked");

        app.active_tab = Tab::Feed;
        terminal.draw(|f| draw(f, &app)).expect("draw feed");

        app.active_tab = Tab::Activity;
        terminal.draw(|f| draw(f, &app)).expect("draw activity");

        app.show_help = true;
        terminal.draw(|f| draw(f, &app)).expect("draw help");
        app.show_help = false;

        app.input_mode = InputMode::AddingShow;
        terminal.draw(|f| draw(f, &app)).expect("draw add modal");
    }
}
