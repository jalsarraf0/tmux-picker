use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

use crate::app::{App, Mode};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Split into 3 vertical sections: sessions table, actions bar, help bar.
    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);

    draw_sessions(frame, app, chunks[0]);
    draw_actions(frame, app, chunks[1]);
    draw_help(frame, app, chunks[2]);
}

// ---------------------------------------------------------------------------
// Sessions table
// ---------------------------------------------------------------------------

fn draw_sessions(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" tmux sessions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if app.sessions.is_empty() {
        let para = Paragraph::new("  No sessions running")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let rows: Vec<Row> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_selected = i == app.selected;

            // Column 1: selector + 1-indexed number (6 wide)
            let selector = if is_selected {
                format!(" \u{25b8} {}", i + 1)
            } else {
                format!("   {}", i + 1)
            };

            // Column 2: session name styling (18 wide)
            let name_style = if session.is_stale() {
                Style::default().fg(Color::DarkGray)
            } else if session.is_claude() {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };

            // Column 3: windows_display() right-aligned (6 wide)
            let windows = format!("{:>6}", session.windows_display());

            // Column 4: current_command in Yellow (12 wide)
            // Column 5: attached indicator (3 wide)
            let attached = if session.attached { "\u{25cf}" } else { " " };

            // Column 6: activity_display() (12 wide)
            let activity_style = if session.is_stale() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let row_style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Span::raw(selector),
                Span::styled(session.name.clone(), name_style),
                Span::raw(windows),
                Span::styled(
                    session.current_command.clone(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(attached, Style::default().fg(Color::Green)),
                Span::styled(session.activity_display(), activity_style),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(3),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).block(block);
    frame.render_widget(table, area);
}

// ---------------------------------------------------------------------------
// Actions bar
// ---------------------------------------------------------------------------

fn draw_actions(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let line = match app.mode {
        Mode::Pick => Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "n",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  new session     "),
            Span::styled(
                "s",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  shell (no tmux)"),
        ]),
        Mode::NewInput => {
            if let Some(ref err) = app.input_error {
                Line::from(vec![
                    Span::raw("  Session name: "),
                    Span::styled(
                        app.input.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::raw("  ("),
                    Span::styled(err.clone(), Style::default().fg(Color::Red)),
                    Span::raw(")"),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  Session name: "),
                    Span::styled(
                        format!("{}\u{2588}", app.input),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ])
            }
        }
    };

    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Help bar
// ---------------------------------------------------------------------------

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let line = match app.mode {
        Mode::Pick => {
            let secs = app.timeout_remaining.as_secs();
            Line::from(vec![Span::styled(
                format!(
                    "  \u{2191}\u{2193} navigate  \u{00b7}  enter/# select  \u{00b7}  s shell  \u{00b7}  q quit  \u{00b7}  auto-attach in {secs}s"
                ),
                Style::default().fg(Color::DarkGray),
            )])
        }
        Mode::NewInput => Line::from(vec![Span::styled(
            "  enter confirm  \u{00b7}  esc cancel",
            Style::default().fg(Color::DarkGray),
        )]),
    };

    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}
