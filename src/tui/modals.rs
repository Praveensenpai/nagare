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
        Line::from("  Tab / Shift-Tab  Cycle between views (Tracked, Feed, Downloads, Activity)"),
        Line::from("  1 / 2 / 3 / 4    Jump directly to view"),
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

/// Renders the Track Episode confirmation popup.
pub fn render_track_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(prompt) = &app.track_prompt else {
        return;
    };
    let popup_area = centered_rect(68, 45, area);
    frame.render_widget(Clear, popup_area);

    let ep_str = format!("S{:02}E{:02}", prompt.season, prompt.episode as u32);
    let title = format!(" Track: {} ({ep_str}) ", prompt.show);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title)
        .title_alignment(Alignment::Center);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Min(1),
        ])
        .split(inner);

    let lines = build_track_modal_lines(prompt);
    frame.render_widget(Paragraph::new(lines), chunks[1]);

    let footer_text = Line::from(vec![
        Span::styled("[1-4] ", Style::default().fg(Color::Green)),
        Span::raw("Quick Select  "),
        Span::styled("[↑/↓] ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled("[Enter] ", Style::default().fg(Color::Green)),
        Span::raw("Confirm  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Yellow)),
        Span::raw("Cancel"),
    ]);
    frame.render_widget(
        Paragraph::new(footer_text).alignment(Alignment::Center),
        chunks[2],
    );
}

fn build_track_modal_lines(prompt: &crate::tui::app::TrackPrompt) -> Vec<Line<'static>> {
    let ep = prompt.episode as u32;
    let prev = ((prompt.episode - 1.0).max(0.0)) as u32;
    let s = prompt.season;

    let items = [
        format!("[1] Include this episode (S{s:02}E{ep:02})       -> Starts at S{s:02}E{prev:02} (picks up current on next run)"),
        format!("[2] Start after this episode (S{s:02}E{:02}+) -> Starts at S{s:02}E{ep:02} (monitors future only)", ep + 1),
        format!("[3] Track full season (from S{s:02}E01)     -> Starts at S{s:02}E00 (catches entire season)"),
        "[4] Custom parameters...                -> Pre-fills Add Show dialog with show details".to_string(),
    ];

    items
        .into_iter()
        .enumerate()
        .map(|(idx, text)| {
            let is_sel = prompt.selected == idx;
            let style = if is_sel {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(26, 42, 74))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let prefix = if is_sel { " ▶ " } else { "   " };
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(text, style),
            ])
        })
        .collect()
}
