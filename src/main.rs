mod action;
mod app;
mod input;
mod session;
mod tmux;
mod ui;

use action::Action;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use std::io::stderr;
use std::time::{Duration, Instant};

const TICK_RATE: Duration = Duration::from_millis(250);

fn main() {
    let action = match run() {
        Ok(action) => action,
        Err(e) => {
            eprintln!("tmux-picker error: {e}");
            Action::Shell
        }
    };
    println!("{action}");
}

fn run() -> Result<Action, Box<dyn std::error::Error>> {
    // Query tmux
    if !tmux::server_running() {
        return Ok(Action::Shell);
    }

    let sessions = tmux::list_sessions().unwrap_or_default();
    if sessions.is_empty() {
        return Ok(Action::New("main".into()));
    }

    // Init terminal on stderr (stdout is for the protocol)
    terminal::enable_raw_mode()?;
    let mut stderr_handle = stderr();
    crossterm::execute!(stderr_handle, EnterAlternateScreen)?;
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

    // Cleanup terminal
    terminal::disable_raw_mode()?;
    crossterm::execute!(stderr(), LeaveAlternateScreen)?;

    Ok(app.action.unwrap_or(Action::Shell))
}
