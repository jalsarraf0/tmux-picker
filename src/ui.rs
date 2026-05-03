use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

use crate::app::{App, Mode};

fn format_name_display(session: &crate::session::Session) -> String {
    match session.metadata.as_ref().and_then(|m| m.label.as_deref()) {
        Some(label) => format!("{label} ({})", session.name),
        None => session.name.clone(),
    }
}

fn format_detail_line(session: &crate::session::Session) -> Option<String> {
    let m = session.metadata.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    if let Some(ref p) = m.project {
        parts.push(collapse_home(p));
    }
    if let Some(ref pu) = m.purpose {
        parts.push(pu.clone());
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("\u{21B3} {}", parts.join("  \u{00B7}  ")))
    }
}

fn collapse_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let detail_height: u16 = app
        .sessions
        .get(app.selected)
        .and_then(format_detail_line)
        .map(|_| 1)
        .unwrap_or(0);

    let preview_height: u16 = if app.selected_name().is_some() {
        // 5 content rows + 2 border rows; only render if there's room.
        if area.height >= 14 { 7 } else { 0 }
    } else {
        0
    };

    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(detail_height),
        Constraint::Length(preview_height),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);

    draw_sessions(frame, app, chunks[0]);
    if detail_height > 0 {
        draw_detail(frame, app, chunks[1]);
    }
    if preview_height > 0 {
        draw_preview(frame, app, chunks[2]);
    }
    draw_actions(frame, app, chunks[3]);
    draw_help(frame, app, chunks[4]);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" preview ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let body = match app.preview.as_deref() {
        Some(text) if !text.is_empty() => text.to_string(),
        Some(_) => String::from("(empty)"),
        None => String::from("(unavailable)"),
    };
    let para = Paragraph::new(body)
        .style(Style::default().fg(Color::DarkGray))
        .block(block);
    frame.render_widget(para, area);
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(s) = app.sessions.get(app.selected)
        && let Some(line) = format_detail_line(s)
    {
        let para = Paragraph::new(Line::from(Span::styled(
            format!("   {line}"),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(para, area);
    }
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
                Span::styled(format_name_display(session), name_style),
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
        Constraint::Length(36),
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
                "/",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  filter     "),
            Span::styled(
                "K",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  kill     "),
            Span::styled(
                "s",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  shell"),
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
        Mode::Filter => {
            let count = app.filtered_indices.len();
            Line::from(vec![
                Span::raw("  filter: /"),
                Span::styled(
                    format!("{}\u{2588}", app.filter),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(
                    format!("   ({count} match)"),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }
        Mode::ConfirmKill => {
            let target = app.kill_target.as_deref().unwrap_or("?");
            Line::from(vec![
                Span::styled(
                    "  kill ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    target.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  ?  "),
                Span::styled("[y]", Style::default().fg(Color::Green)),
                Span::raw(" yes  "),
                Span::styled("[any]", Style::default().fg(Color::Yellow)),
                Span::raw(" cancel"),
            ])
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
        Mode::Filter => Line::from(vec![Span::styled(
            "  type to filter  \u{00b7}  \u{2191}\u{2193} navigate  \u{00b7}  enter attach  \u{00b7}  esc cancel",
            Style::default().fg(Color::DarkGray),
        )]),
        Mode::ConfirmKill => Line::from(vec![Span::styled(
            "  y to kill  \u{00b7}  any other key cancels",
            Style::default().fg(Color::DarkGray),
        )]),
    };

    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Metadata;
    use crate::session::Session;
    use std::time::Duration;

    fn make_session(name: &str, label: Option<&str>) -> Session {
        Session {
            name: name.into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: label.map(|l| Metadata {
                label: Some(l.into()),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn format_name_with_label() {
        let s = make_session("claude-app", Some("Refactoring auth"));
        assert_eq!(format_name_display(&s), "Refactoring auth (claude-app)");
    }

    #[test]
    fn format_name_without_label() {
        let s = make_session("main", None);
        assert_eq!(format_name_display(&s), "main");
    }

    #[test]
    fn format_name_with_empty_metadata_no_label() {
        let mut s = make_session("main", None);
        s.metadata = Some(Metadata::default());
        assert_eq!(format_name_display(&s), "main");
    }

    #[test]
    fn detail_line_with_project_only() {
        let s = Session {
            name: "main".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: Some(Metadata {
                project: Some("/home/u/git/app".into()),
                ..Default::default()
            }),
        };
        // SAFETY: tests may run in parallel. Best-effort assertion that prefix
        // logic works at all.
        unsafe {
            std::env::set_var("HOME", "/home/u");
        }
        assert_eq!(format_detail_line(&s).unwrap(), "\u{21B3} ~/git/app");
    }

    #[test]
    fn detail_line_with_purpose_only() {
        let s = Session {
            name: "main".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: Some(Metadata {
                purpose: Some("PR #234".into()),
                ..Default::default()
            }),
        };
        assert_eq!(format_detail_line(&s).unwrap(), "\u{21B3} PR #234");
    }

    #[test]
    fn detail_line_none_for_no_metadata() {
        let s = Session {
            name: "main".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: None,
        };
        assert!(format_detail_line(&s).is_none());
    }
}
