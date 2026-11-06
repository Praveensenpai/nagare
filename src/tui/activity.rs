//! Activity and Seedr Cloud status view renderer.

use crate::tui::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph, Row, Table},
    Frame,
};

/// Renders the Activity & Seedr Cloud status view.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_cloud_card(frame, chunks[0], app);
    render_events_log(frame, chunks[1], app);
}

fn render_cloud_card(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Seedr Cloud & Storage ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .split(inner);

    render_storage_gauge(frame, rows[0], app.seedr_space);
    render_storage_details(frame, rows[2], app);
}

fn render_storage_gauge(frame: &mut Frame, area: Rect, space: Option<(u64, u64)>) {
    let (max_bytes, used_bytes) = space.unwrap_or((5 * 1024 * 1024 * 1024, 0));
    let ratio = if max_bytes > 0 {
        (used_bytes as f64 / max_bytes as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let gauge_label = format!(
        "{:.1} / {:.1} GiB ({:.0}%)",
        used_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        max_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        ratio * 100.0
    );

    let gauge_color = if ratio > 0.85 {
        Color::Red
    } else if ratio > 0.65 {
        Color::Yellow
    } else {
        Color::Green
    };

    let gauge = Gauge::default()
        .block(Block::default().title(" Cloud Quota "))
        .gauge_style(Style::default().fg(gauge_color).bg(Color::Rgb(20, 25, 35)))
        .ratio(ratio)
        .label(gauge_label);
    frame.render_widget(gauge, area);
}

fn render_storage_details(frame: &mut Frame, area: Rect, app: &App) {
    let jelly_status = match &app.config.jellyfin.url {
        Some(url) => format!("✓ Connected ({url})"),
        None => "○ Disabled (no URL configured)".into(),
    };

    let details = vec![
        Line::from(Span::styled(
            "Paths & Configuration",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Library:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.config.library_dir.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Staging:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.config.staging_dir.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Jellyfin: ", Style::default().fg(Color::DarkGray)),
            Span::raw(jelly_status),
        ]),
        Line::from(vec![
            Span::styled("Feed:     ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.config.feed_url.clone()),
        ]),
    ];
    frame.render_widget(Paragraph::new(details), area);
}

fn render_events_log(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["Timestamp", "Level", "Event Message"]
        .into_iter()
        .map(|h| {
            Span::styled(
                h,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.events.iter().map(|event| {
        let level_color = match event.level.as_str() {
            "grab" => Color::Green,
            "track" => Color::Cyan,
            "error" => Color::Red,
            _ => Color::DarkGray,
        };
        let level_pill = Span::styled(
            format!("[{}]", event.level),
            Style::default().fg(level_color),
        );

        Row::new(vec![
            Span::styled(&event.at, Style::default().fg(Color::DarkGray)),
            level_pill,
            Span::raw(&event.message),
        ])
        .height(1)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" Activity Audit Log ({}) ", app.events.len())),
    );

    frame.render_widget(table, area);
}
