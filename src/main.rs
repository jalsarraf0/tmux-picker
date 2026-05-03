use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use std::io::stderr;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use tmux_picker::action::Action;
use tmux_picker::app::App;
use tmux_picker::cli::{Cli, Command};
use tmux_picker::{input, metadata, tmux, ui};

const TICK_RATE: Duration = Duration::from_millis(250);

/// RAII guard that restores the terminal on drop — even on panic or early error.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(stderr(), LeaveAlternateScreen);
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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
    // Query tmux — single call, no TOCTOU race
    let sessions = match tmux::list_sessions() {
        Ok(s) if s.is_empty() => return Ok(Action::New("main".into())),
        Ok(s) => s,
        // tmux server not running (e.g. early boot race) — create a session
        Err(_) => return Ok(Action::New("main".into())),
    };

    // Init terminal on stderr (stdout is for the protocol)
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard; // cleanup on any exit path
    crossterm::execute!(stderr(), EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sessions);
    let mut last_tick = Instant::now();

    // Main loop
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            input::handle_key(&mut app, key);
        }

        if last_tick.elapsed() >= TICK_RATE {
            app.tick(last_tick.elapsed());
            last_tick = Instant::now();
        }

        if app.should_quit() {
            break;
        }
    }

    // _guard Drop handles terminal cleanup
    Ok(app.action.unwrap_or(Action::Shell))
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
