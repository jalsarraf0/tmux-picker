use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
};

use crate::app::{App, Mode, PreviewMode};
use crate::config::Theme;

/// Render-time context: theme, future render-only flags. Borrowed so the UI
/// owns nothing.
pub struct UiContext<'a> {
    /// Theme used for this render.
    pub theme: &'a Theme,
}

fn format_name_display(session: &crate::session::Session) -> String {
    let base = match session.metadata.as_ref().and_then(|m| m.label.as_deref()) {
        Some(label) => format!("{label} ({})", session.name),
        None => session.name.clone(),
    };
    match session.marker.as_deref() {
        Some(glyph) => format!("{glyph} {base}"),
        // Pad with two spaces so unmarked rows align with marked rows.
        None => format!("  {base}"),
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

/// Render the picker into a Ratatui frame.
pub fn draw(frame: &mut Frame, app: &App, ctx: &UiContext<'_>) {
    let area = frame.area();

    let detail_height: u16 = app
        .selected_session()
        .and_then(format_detail_line)
        .map_or(0, |_| 1);

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

    draw_sessions(frame, app, ctx, chunks[0]);
    if detail_height > 0 {
        draw_detail(frame, app, chunks[1]);
    }
    if preview_height > 0 {
        draw_preview(frame, app, chunks[2]);
    }
    draw_actions(frame, app, ctx, chunks[3]);
    draw_help(frame, app, chunks[4]);

    if app.mode == Mode::Help {
        draw_help_overlay(frame, ctx, area);
    }
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    match app.preview_mode {
        PreviewMode::Summary => draw_preview_summary(frame, app, area),
        PreviewMode::WindowsList => draw_preview_windows(frame, app, area),
    }
}

fn draw_preview_summary(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" preview (Tab: windows) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let body = match app.preview.as_deref() {
        Some(text) if !text.is_empty() => text.to_owned(),
        Some(_) => String::from("(empty)"),
        None => String::from("(unavailable)"),
    };
    let para = Paragraph::new(body)
        .style(Style::default().fg(Color::DarkGray))
        .block(block);
    frame.render_widget(para, area);
}

fn draw_preview_windows(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" windows (Tab: preview) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let lines: Vec<Line> = match app.preview_windows.as_deref() {
        Some(snaps) if !snaps.is_empty() => snaps
            .iter()
            .map(|s| {
                Line::from(vec![
                    Span::styled(
                        format!(" {:<14}", truncate(&s.name, 14)),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" \u{203A} ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        truncate(&s.last_line, area.width.saturating_sub(20) as usize),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect(),
        Some(_) => vec![Line::from(Span::styled(
            "(no windows)",
            Style::default().fg(Color::DarkGray),
        ))],
        None => vec![Line::from(Span::styled(
            "(unavailable)",
            Style::default().fg(Color::DarkGray),
        ))],
    };
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

/// Map a screen-y coordinate from a mouse event to a 0-indexed session
/// row in the visible list. Returns None if the click was outside the
/// rows region. The sessions block occupies the top of the screen with
/// one border row above the first session row.
pub fn row_for_y(y: u16) -> Option<usize> {
    if y == 0 {
        return None;
    }
    Some((y - 1) as usize)
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(s) = app.selected_session()
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

fn draw_sessions(frame: &mut Frame, app: &App, ctx: &UiContext<'_>, area: Rect) {
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
        .filtered_indices
        .iter()
        .enumerate()
        .filter_map(|(visible_idx, &session_idx)| {
            let session = app.sessions.get(session_idx)?;
            let is_selected = visible_idx == app.selected;

            // Column 1: selector + 1-indexed number within the visible list
            // (6 wide). Numbers are 1..=N over the filtered set so digit
            // selection lines up with what the user sees.
            let selector = if is_selected {
                format!(" \u{25b8} {}", visible_idx + 1)
            } else {
                format!("   {}", visible_idx + 1)
            };

            // Column 2: session name styling (18 wide)
            let name_style = if session.is_stale() {
                Style::default().fg(Color::DarkGray)
            } else if session.is_claude() {
                Style::default()
                    .fg(ctx.theme.accent)
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
                Style::default().bg(ctx.theme.selection_bg)
            } else {
                Style::default()
            };

            let (dot_char, dot_color) = session.activity_dot();
            Some(
                Row::new(vec![
                    Span::raw(selector),
                    Span::styled(format_name_display(session), name_style),
                    Span::raw(windows),
                    Span::styled(
                        session.current_command.as_str(),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(attached, Style::default().fg(Color::Green)),
                    Span::styled(dot_char.to_string(), Style::default().fg(dot_color)),
                    Span::styled(session.activity_display(), activity_style),
                ])
                .style(row_style),
            )
        })
        .collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Length(38),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).block(block);
    frame.render_widget(table, area);
}

// ---------------------------------------------------------------------------
// Actions bar
// ---------------------------------------------------------------------------

fn draw_actions(frame: &mut Frame, app: &App, ctx: &UiContext<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let accent = ctx.theme.accent;
    let warning = ctx.theme.warning;

    let line = match app.mode {
        Mode::Pick => Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "n",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" new   "),
            Span::styled(
                "/",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" filter   "),
            Span::styled(
                "K",
                Style::default().fg(warning).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" kill   "),
            Span::styled(
                "s",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" shell   "),
            Span::styled(
                "?",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" help"),
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
                    Style::default().fg(warning).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    target,
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
        Mode::Help => Line::from(vec![Span::styled(
            "  help — press esc, ?, or q to close",
            Style::default().fg(Color::DarkGray),
        )]),
        Mode::Rename => {
            if let Some(ref err) = app.input_error {
                Line::from(vec![
                    Span::raw("  rename: "),
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
                    Span::raw("  rename: "),
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

    let line = if let Some(ref msg) = app.flash {
        Line::from(vec![Span::styled(
            format!("  {msg}"),
            Style::default().fg(Color::DarkGray),
        )])
    } else {
        match app.mode {
            Mode::Pick => {
                let secs = app.timeout_remaining.as_secs();
                let countdown = if app.timeout_secs == 0 {
                    String::from("auto-attach off")
                } else {
                    format!("auto-attach in {secs}s")
                };
                Line::from(vec![Span::styled(
                    format!(
                        "  \u{2191}\u{2193} navigate  \u{00b7}  enter/# select  \u{00b7}  r rename  \u{00b7}  o sort  \u{00b7}  y yank  \u{00b7}  ? help  \u{00b7}  {countdown}"
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
            Mode::Help => Line::from(vec![Span::styled(
                "  esc / ? / q to close",
                Style::default().fg(Color::DarkGray),
            )]),
            Mode::Rename => Line::from(vec![Span::styled(
                "  enter confirm  \u{00b7}  esc cancel",
                Style::default().fg(Color::DarkGray),
            )]),
        }
    };

    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Help overlay
// ---------------------------------------------------------------------------

fn draw_help_overlay(frame: &mut Frame, ctx: &UiContext<'_>, area: Rect) {
    let lines = help_overlay_lines(ctx);
    let width = lines
        .iter()
        .map(|l| visible_width(l))
        .max()
        .unwrap_or(40)
        .saturating_add(4) as u16;
    let height = (lines.len() as u16).saturating_add(2);

    let overlay = centered_rect(width, height, area);

    // Clear what's underneath so the overlay is opaque.
    frame.render_widget(ratatui::widgets::Clear, overlay);

    let block = Block::default()
        .title(" help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ctx.theme.accent));

    let body: Vec<Line> = lines.into_iter().map(Line::from).collect();
    let para = Paragraph::new(body).block(block);
    frame.render_widget(para, overlay);
}

fn help_overlay_lines(ctx: &UiContext<'_>) -> Vec<String> {
    let _ = ctx;
    // Plain strings — colourised inline rendering is overkill here. Each
    // entry is "<keys>  <description>". Keep keys to <= 10 columns so the
    // descriptions line up.
    vec![
        String::from("  Pick mode"),
        String::from("    \u{2191} \u{2193} / j k    move selection"),
        String::from("    1-9          jump to row N"),
        String::from("    enter        attach to highlighted session"),
        String::from("    n            new session"),
        String::from("    /            filter sessions"),
        String::from("    K            kill highlighted session"),
        String::from("    s            drop to shell"),
        String::from("    q / esc      drop to shell"),
        String::from("    r            rename selected session"),
        String::from("    o            cycle sort modes"),
        String::from("    y            yank session name to clipboard"),
        String::from("    Tab          toggle preview / windows view"),
        String::from("    click        select row"),
        String::from("    double-click attach to row"),
        String::from("    ?            show this help"),
        String::new(),
        String::from("  Filter mode"),
        String::from("    type         narrow the visible list"),
        String::from("    backspace    widen the list"),
        String::from("    \u{2191} \u{2193}          navigate the matches"),
        String::from("    enter        attach to highlighted match"),
        String::from("    esc          clear filter and exit mode"),
        String::new(),
        String::from("  New session"),
        String::from("    type         enter session name"),
        String::from("    enter        create the session"),
        String::from("    esc          cancel"),
        String::new(),
        String::from("  Kill confirm"),
        String::from("    y / Y        kill the session"),
        String::from("    any other    cancel"),
    ]
}

fn visible_width(s: &str) -> usize {
    s.chars().count()
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
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
            marker: None,
        }
    }

    #[test]
    fn format_name_with_label() {
        let s = make_session("claude-app", Some("Refactoring auth"));
        // Two-space marker pad keeps unmarked rows aligned with marked rows.
        assert_eq!(format_name_display(&s), "  Refactoring auth (claude-app)");
    }

    #[test]
    fn format_name_without_label() {
        let s = make_session("main", None);
        assert_eq!(format_name_display(&s), "  main");
    }

    #[test]
    fn format_name_with_empty_metadata_no_label() {
        let mut s = make_session("main", None);
        s.metadata = Some(Metadata::default());
        assert_eq!(format_name_display(&s), "  main");
    }

    #[test]
    fn format_name_with_marker_renders_glyph() {
        let mut s = make_session("claude-app", Some("Refactoring auth"));
        s.marker = Some("\u{1F916}".into());
        assert_eq!(
            format_name_display(&s),
            "\u{1F916} Refactoring auth (claude-app)"
        );
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
            marker: None,
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
            marker: None,
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
            marker: None,
        };
        assert!(format_detail_line(&s).is_none());
    }

    // -----------------------------------------------------------------------
    // Help overlay rendering
    // -----------------------------------------------------------------------

    #[test]
    fn help_overlay_lists_every_keybinding_section() {
        let theme = crate::config::Theme::default();
        let ctx = UiContext { theme: &theme };
        let lines = help_overlay_lines(&ctx);
        let joined = lines.join("\n");
        assert!(joined.contains("Pick mode"));
        assert!(joined.contains("Filter mode"));
        assert!(joined.contains("New session"));
        assert!(joined.contains("Kill confirm"));
    }

    #[test]
    fn help_overlay_mentions_the_question_mark_binding() {
        let theme = crate::config::Theme::default();
        let ctx = UiContext { theme: &theme };
        let joined = help_overlay_lines(&ctx).join("\n");
        assert!(joined.contains('?'));
        assert!(joined.contains("show this help"));
    }

    #[test]
    fn help_overlay_renders_through_test_backend() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let theme = crate::config::Theme::default();
        let ctx = UiContext { theme: &theme };
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                draw_help_overlay(f, &ctx, area);
            })
            .unwrap();

        // The overlay should have rendered the title and at least one of
        // the section headers somewhere on the buffer.
        let buffer = terminal.backend().buffer().clone();
        let dump: String = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(dump.contains("help"));
        assert!(dump.contains("Pick mode"));
    }

    // -----------------------------------------------------------------------
    // Filtered-row rendering: regression for bug where the highlight tracked
    // `app.selected` against the *unfiltered* list.
    // -----------------------------------------------------------------------

    #[test]
    fn filtered_table_renders_only_matching_rows() {
        use crate::app::App;
        use crate::config::Config;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let sessions = vec![
            make_session("alpha", None),
            make_session("beta", None),
            make_session("gamma", None),
        ];
        let mut app = App::new(sessions, &Config::default());
        app.enter_filter_mode();
        for c in "be".chars() {
            app.filter_char(c);
        }
        // Only "beta" matches.
        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.selected_name(), Some("beta"));

        // Render a frame and confirm the buffer contains "beta" but not
        // "alpha" / "gamma" — those should be filtered out, not just hidden
        // behind the highlight.
        let theme = crate::config::Theme::default();
        let ctx = UiContext { theme: &theme };
        let backend = TestBackend::new(80, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app, &ctx)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let dump: String = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(dump.contains("beta"));
        assert!(!dump.contains("alpha"));
        assert!(!dump.contains("gamma"));
    }
}
