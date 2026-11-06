//! Modal dialogs for the nagare TUI (Help overlay and Add Show popup).

use crate::tui::app::{AddField, App};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// Renders a centered keyboard shortcuts help dialog.
pub fn render_help(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 65, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Keybindings 流れ ")
        .title_alignment(Alignment::Center);

    let help_text = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab / Shift-Tab  Cycle between views (Tracked, Feed, Activity)"),
        Line::from("  1 / 2 / 3        Jump directly to view"),
        Line::from("  j / Down         Move cursor down"),
        Line::from("  k / Up           Move cursor up"),
        Line::from("  g / Home         Jump to top"),
        Line::from("  G / End          Jump to bottom"),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  r                Trigger manual pipeline run / scan"),
        Line::from("  a                Open Add Show dialog"),
        Line::from("  d                Untrack / remove selected show"),
        Line::from("  t                Track highlighted show from Feed view"),
        Line::from("  /                Filter / search feed releases"),
        Line::from("  ?                Toggle this help screen"),
        Line::from("  q / Esc          Close popup / Quit nagare"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, popup_area);
}

/// Renders the Add Show modal dialog.
pub fn render_add_modal(frame: &mut Frame, area: Rect, app: &App) {
    let popup_area = centered_rect(55, 45, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Track New Anime 流れ ")
        .title_alignment(Alignment::Center);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(inner);

    render_input_field(
        frame,
        rows[1],
        " Anime Title ",
        &app.add_title,
        app.add_focus == AddField::Title,
    );
    render_input_field(
        frame,
        rows[2],
        " Starting Season (default 1) ",
        &app.add_season,
        app.add_focus == AddField::Season,
    );
    render_input_field(
        frame,
        rows[3],
        " Starting Episode Watermark (default 0) ",
        &app.add_episode,
        app.add_focus == AddField::Episode,
    );

    let footer_text = Line::from(vec![
        Span::styled("[Tab] ", Style::default().fg(Color::Cyan)),
        Span::raw("Next Field  "),
        Span::styled("[Enter] ", Style::default().fg(Color::Green)),
        Span::raw("Save  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Yellow)),
        Span::raw("Cancel"),
    ]);
    frame.render_widget(
        Paragraph::new(footer_text).alignment(Alignment::Center),
        rows[4],
    );
}

fn render_input_field(frame: &mut Frame, area: Rect, title: &str, value: &str, is_focused: bool) {
    let style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .title(title);
    let widget = Paragraph::new(format!(" {value}_")).block(block);
    frame.render_widget(widget, area);
}

/// Computes a centered rectangle with percentage width and height.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
