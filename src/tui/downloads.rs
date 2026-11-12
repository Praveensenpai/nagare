//! Downloads view renderer for the nagare TUI.

use crate::db::GrabbedRelease;
use crate::organize;
use crate::pipeline::TransferProgress;
use crate::tui::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph, Row, Table},
    Frame,
};

/// Renders the Downloads tab.
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if app.active_transfer.is_some() {
            vec![Constraint::Length(5), Constraint::Min(8)]
        } else {
            vec![Constraint::Length(3), Constraint::Min(8)]
        })
        .split(area);

    if let Some(transfer) = &app.active_transfer {
        render_active_card(frame, chunks[0], transfer);
    } else {
        render_idle_card(frame, chunks[0], app);
    }

    render_downloads_table(frame, chunks[1], app);
}

fn render_active_card(frame: &mut Frame, area: Rect, transfer: &TransferProgress) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" ⏬ Active Download ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let ratio = (transfer.percent / 100.0).clamp(0.0, 1.0);
    let speed_mib = transfer.speed_bps as f64 / (1024.0 * 1024.0);
    let label = format!(
        "{:.1}% • {:.1} MB/s • ETA {}s",
        transfer.percent, speed_mib, transfer.eta_seconds
    );
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, rows[0]);

    let sub =
        Paragraph::new(format!("File: {}", transfer.show)).style(Style::default().fg(Color::Gray));
    frame.render_widget(sub, rows[1]);
}

fn render_idle_card(frame: &mut Frame, area: Rect, app: &App) {
    let next_str = app
        .next_scan
        .as_deref()
        .map(|s| format!("• Next scan: {s} "))
        .unwrap_or_else(|| "• Checks every 30m ".into());
    let line = Line::from(vec![
        Span::styled("● ", Style::default().fg(Color::Green)),
        Span::styled(
            "Daemon Idle ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{next_str}• Press [r] to poll immediately"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Background Service ");
    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

fn downloads_header<'a>() -> Row<'a> {
    Row::new(vec![
        Span::raw(""),
        Span::styled("Show Title", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("S/E", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("Size", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("Grabbed At", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("Status", Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(Color::Cyan))
    .bottom_margin(1)
}

fn render_downloads_table(frame: &mut Frame, area: Rect, app: &mut App) {
    let title = format!(" Grabbed Releases ({}) ", app.releases.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title);

    if app.releases.is_empty() {
        let empty =
            Paragraph::new("No episodes downloaded yet. Track a show and press [r] to grab.")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
        frame.render_widget(empty, area);
        return;
    }

    app.downloads_state.select(Some(app.selected_release));

    let rows: Vec<Row> = app
        .releases
        .iter()
        .enumerate()
        .map(|(idx, rel)| make_release_row(app, rel, idx == app.selected_release))
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Percentage(40),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(18),
        Constraint::Length(14),
    ];
    let table = Table::new(rows, widths)
        .header(downloads_header())
        .block(block);
    frame.render_stateful_widget(table, area, &mut app.downloads_state);
    crate::tui::views::render_table_scrollbar(
        frame,
        area,
        app.releases.len(),
        app.selected_release,
    );
}

fn make_release_row<'a>(app: &App, rel: &'a GrabbedRelease, selected: bool) -> Row<'a> {
    let on_disk = organize::exists_on_disk(
        &app.config.library_dir,
        &rel.show_title,
        rel.season,
        rel.episode,
    );
    let disk_status = if on_disk {
        Span::styled("✔ On Disk", Style::default().fg(Color::Green))
    } else {
        Span::styled("✖ Missing", Style::default().fg(Color::Red))
    };

    let prefix = if selected { "▶ " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(26, 42, 74))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let se = format!("S{:02}E{:02}", rel.season, rel.episode as u32);
    let size_str = format_bytes(rel.bytes);
    let date_str = rel.grabbed_at.chars().take(16).collect::<String>();

    Row::new(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::raw(rel.show_title.as_str()),
        Span::raw(se),
        Span::raw(size_str),
        Span::raw(date_str),
        disk_status,
    ])
    .style(style)
}

fn format_bytes(bytes: u64) -> String {
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gib >= 1.0 {
        format!("{gib:.2} GiB")
    } else {
        let mib = bytes as f64 / (1024.0 * 1024.0);
        format!("{mib:.1} MiB")
    }
}
