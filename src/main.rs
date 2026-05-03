use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use std::io::stderr;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tmux_picker::action::Action;
use tmux_picker::app::{App, Mode};
use tmux_picker::cli::{Cli, Command};
use tmux_picker::config::Config;
use tmux_picker::{input, metadata, tmux, ui};

const TICK_RATE: Duration = Duration::from_millis(250);

/// RAII guard that restores the terminal on drop — even on panic or early error.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(stderr(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.check_config {
        return run_check_config();
    }
    match cli.command {
        None => run_picker(),
        Some(Command::Label {
            session,
            label,
            project,
            purpose,
            clear,
        }) => run_label(&session, label, project, purpose, clear),
        Some(Command::Show { session }) => run_show(&session),
        Some(Command::Auto { session }) => run_auto(&session),
    }
}

fn run_check_config() -> ExitCode {
    let (cfg, warnings) = Config::load_with_warnings();
    println!("# tmux-picker config check");
    println!();
    println!("# warnings");
    if warnings.is_empty() {
        println!("(none)");
    } else {
        for w in &warnings {
            println!("{w}");
        }
    }
    println!();
    println!("# effective config");
    print!("{}", cfg.to_toml());
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Picker (existing TUI behavior)
// ---------------------------------------------------------------------------

fn run_picker() -> ExitCode {
    let action = match picker_loop() {
        Ok(action) => action,
        Err(e) => {
            eprintln!("tmux-picker error: {e}");
            Action::Shell
        }
    };
    println!("{action}");
    ExitCode::SUCCESS
}

fn picker_loop() -> Result<Action, Box<dyn std::error::Error>> {
    // Load user config (silent fall-back to defaults on any error).
    let mut config = Config::load();

    // Query tmux — single call, no TOCTOU race
    let mut sessions = match tmux::list_sessions() {
        Ok(s) if s.is_empty() => return Ok(Action::New("main".into())),
        Ok(s) => s,
        // tmux server not running (e.g. early boot race) — create a session
        Err(_) => return Ok(Action::New("main".into())),
    };
    tmux::populate_markers(&mut sessions, &config.markers);

    // SIGHUP → reload config in place. Best-effort: failure to install
    // the handler is logged but does not abort startup.
    let reload_flag = Arc::new(AtomicBool::new(false));
    if let Err(e) =
        signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&reload_flag))
    {
        eprintln!("tmux-picker: SIGHUP handler not installed: {e}");
    }

    // Init terminal on stderr (stdout is for the protocol)
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard; // cleanup on any exit path
    crossterm::execute!(stderr(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sessions, &config);
    let mut last_tick = Instant::now();
    refresh_preview_if_needed(&mut app);

    // Main loop
    loop {
        let theme = config.theme.clone();
        let ui_ctx = ui::UiContext { theme: &theme };
        terminal.draw(|f| ui::draw(f, &app, &ui_ctx))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    input::handle_key(&mut app, key);
                    if let Some(target) = app.take_pending_kill() {
                        let _ = tmux::kill_session(&target);
                    }
                    if let Some((old, new)) = app.take_pending_rename()
                        && let Err(e) = tmux::rename_session(&old, &new)
                    {
                        app.set_flash(format!("rename failed: {e}"));
                    }
                }
                Event::Mouse(m) => {
                    if matches!(app.mode, Mode::Pick | Mode::Filter)
                        && let MouseEventKind::Down(MouseButton::Left) = m.kind
                        && let Some(row) = ui::row_for_y(m.row)
                    {
                        app.handle_mouse_click(row, Instant::now());
                    }
                }
                _ => {}
            }
        }

        if reload_flag.swap(false, Ordering::SeqCst) {
            let (new_cfg, warnings) = Config::load_with_warnings();
            app.timeout_secs = new_cfg.timeout_secs;
            config = new_cfg;
            tmux::populate_markers(&mut app.sessions, &config.markers);
            app.preview = None;
            app.preview_for = None;
            app.preview_windows = None;
            if warnings.is_empty() {
                app.set_flash("[config reloaded]".into());
            } else {
                app.set_flash(format!(
                    "[config reloaded with {} warning(s)]",
                    warnings.len()
                ));
                for w in &warnings {
                    eprintln!("tmux-picker config: {w}");
                }
            }
        }

        // If the session list went dirty (e.g., post-kill), re-fetch.
        if app.sessions_dirty
            && let Ok(mut fresh) = tmux::list_sessions()
        {
            if fresh.is_empty() {
                // No sessions left — drop out and let the shell stub create one.
                app.action = Some(Action::New("main".into()));
                break;
            }
            tmux::populate_markers(&mut fresh, &config.markers);
            app.replace_sessions(fresh);
        }

        if last_tick.elapsed() >= TICK_RATE {
            app.tick(last_tick.elapsed());
            refresh_preview_if_needed(&mut app);
            last_tick = Instant::now();
        }

        if app.should_quit() {
            break;
        }
    }

    // _guard Drop handles terminal cleanup
    Ok(app.action.unwrap_or(Action::Shell))
}

/// If the preview cache is missing or stale, fetch a fresh one for the
/// currently-selected session. On capture failure we cache None so the UI
/// renders "(unavailable)" without re-trying every tick. Honours
/// `app.preview_mode`: Summary uses pane_capture, WindowsList uses
/// list_windows.
fn refresh_preview_if_needed(app: &mut App) {
    let Some(name) = app.selected_name().map(String::from) else {
        return;
    };
    match app.preview_mode {
        tmux_picker::app::PreviewMode::Summary => {
            if app.preview.is_some() && app.preview_is_current() {
                return;
            }
            let captured = tmux::pane_capture(&name, 6).ok();
            app.set_preview(captured);
        }
        tmux_picker::app::PreviewMode::WindowsList => {
            if app.preview_windows.is_some() && app.preview_is_current() {
                return;
            }
            let snaps = tmux::list_windows(&name, 8).ok();
            app.preview_windows = snaps;
            app.preview_for = Some(name);
        }
    }
}

// ---------------------------------------------------------------------------
// label / show / auto
// ---------------------------------------------------------------------------

fn run_label(
    session: &str,
    label: Option<String>,
    project: Option<String>,
    purpose: Option<String>,
    clear: bool,
) -> ExitCode {
    if clear {
        return match metadata::clear(session) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("tmux-picker label: {e}");
                ExitCode::FAILURE
            }
        };
    }

    if label.is_none() && project.is_none() && purpose.is_none() {
        eprintln!(
            "tmux-picker label: no fields to set; \
             pass --label, --project, --purpose, or --clear"
        );
        return ExitCode::FAILURE;
    }

    for (name, val) in [
        ("--label", &label),
        ("--project", &project),
        ("--purpose", &purpose),
    ] {
        if let Some(v) = val {
            if v.is_empty() {
                eprintln!("tmux-picker label: {name} value must not be empty");
                return ExitCode::FAILURE;
            }
            if v.contains('|') {
                eprintln!("tmux-picker label: {name} value must not contain '|'");
                return ExitCode::FAILURE;
            }
        }
    }

    let m = metadata::Metadata {
        label,
        project,
        purpose,
        label_at: None,
    };
    match metadata::write(session, &m) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tmux-picker label: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_show(session: &str) -> ExitCode {
    match metadata::read(session) {
        Ok(m) => {
            print!("{}", m.to_toml(session));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tmux-picker show: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_auto(session: &str) -> ExitCode {
    match metadata::auto_detect(session) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tmux-picker auto: {e}");
            ExitCode::FAILURE
        }
    }
}
