//! Tab view renderers for Tracked Shows and Live Feed.

use crate::db::Show;
use crate::feed::Item;
use crate::tui::app::{App, InputMode};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Row, Table},
    Frame,
};

/// Renders the Tracked Shows table and metadata.
pub fn render_tracked(frame: &mut Frame, area: Rect, app: &App) {
    if app.shows.is_empty() {
        render_empty_tracked(frame, area);
        return;
    }

    let header_cells = [
        "",
        "ID",
        "Anime Title",
        "Watermark",
        "Match Pattern",
        "Status",
    ]
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

    let rows = app
        .shows
        .iter()
        .enumerate()
        .map(|(idx, show)| build_show_row(show, idx == app.selected_show));

    let title = format!(" Tracked Shows ({}) ", app.shows.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Percentage(40),
            Constraint::Length(12),
            Constraint::Percentage(25),
            Constraint::Min(14),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title),
    );

    frame.render_widget(table, area);
}

fn render_empty_tracked(frame: &mut Frame, area: Rect) {
    let empty_msg = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No anime shows tracked yet",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("Press [a] to add a show manually, or press [2] to browse the live feed."),
        Line::from("Tracked shows are automatically monitored for new episodes each week."),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Tracked Shows (0) ")
        .title_alignment(Alignment::Left);
    let widget = Paragraph::new(empty_msg)
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn build_show_row<'a>(show: &Show, is_selected: bool) -> Row<'a> {
    let prefix = if is_selected { "▶ " } else { "  " };
    let watermark = format!("S{:02}E{:02}", show.season, show.max_episode as u32);
    let status = if show.enabled {
        Span::styled("● Monitoring", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ Paused", Style::default().fg(Color::DarkGray))
    };

    let style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(26, 42, 74))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Row::new(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::raw(show.id.to_string()),
        Span::raw(show.title.clone()),
        Span::styled(watermark, Style::default().fg(Color::LightBlue)),
        Span::styled(
            show.match_title.clone(),
            Style::default().fg(Color::DarkGray),
        ),
        status,
    ])
    .style(style)
    .height(1)
}

/// Renders the Live SubsPlease Feed table and filter bar.
pub fn render_feed(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    render_feed_filter(frame, chunks[0], app);
    if app.feed_loading {
        render_feed_loading(frame, chunks[1]);
    } else {
        render_feed_table(frame, chunks[1], app);
    }
}

fn render_feed_loading(frame: &mut Frame, area: Rect) {
    let loading_widget = Paragraph::new("\n ⟳ Fetching SubsPlease 1080p RSS feed...")
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(loading_widget, area);
}

fn render_feed_table(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["", "Show Name", "Release", "Size", "Type", "Status"]
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

    let rows = app
        .filtered_feed
        .iter()
        .enumerate()
        .map(|(display_idx, &feed_idx)| {
            let item = &app.feed_items[feed_idx];
            let is_tracked = app.shows.iter().any(|s| {
                s.title.eq_ignore_ascii_case(&item.show)
                    || s.match_title.eq_ignore_ascii_case(&item.show)
            });
            build_feed_row(item, display_idx == app.selected_feed, is_tracked)
        });

    let title = format!(
        " Live Feed ({}/{} items) — [Enter/Space] Track/Untrack ",
        app.filtered_feed.len(),
        app.feed_items.len()
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Percentage(45),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title),
    );

    frame.render_widget(table, area);
}

fn render_feed_filter(frame: &mut Frame, area: Rect, app: &App) {
    let filter_style = if app.input_mode == InputMode::Filtering {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let filter_text = if app.filter_input.is_empty() {
        if app.input_mode == InputMode::Filtering {
            " _"
        } else {
            " Press [/] to filter releases by show name..."
        }
    } else {
        &app.filter_input
    };

    let search_bar = Paragraph::new(format!(" 🔍 {filter_text}")).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(filter_style)
            .title(" Live Release Filter "),
    );
    frame.render_widget(search_bar, area);
}

fn build_feed_row<'a>(item: &Item, is_selected: bool, is_tracked: bool) -> Row<'a> {
    let prefix = if is_selected { "▶ " } else { "  " };
    let size_str = format_bytes(item.bytes);
    let ep_tag = item.episode_label();
    let batch_label = if item.is_batch {
        Span::styled("Batch", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("Single", Style::default().fg(Color::DarkGray))
    };

    let tracked_badge = if is_tracked {
        Span::styled("✓ Tracked", Style::default().fg(Color::Green))
    } else {
        Span::styled("+ New", Style::default().fg(Color::DarkGray))
    };

    let style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(26, 42, 74))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Row::new(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::raw(item.show.clone()),
        Span::styled(ep_tag, Style::default().fg(Color::LightBlue)),
        Span::styled(size_str, Style::default().fg(Color::DarkGray)),
        batch_label,
        tracked_badge,
    ])
    .style(style)
    .height(1)
}

fn format_bytes(bytes: u64) -> String {
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gib >= 1.0 {
        return format!("{gib:.2} GiB");
    }
    let mib = bytes as f64 / (1024.0 * 1024.0);
    format!("{mib:.0} MiB")
}
