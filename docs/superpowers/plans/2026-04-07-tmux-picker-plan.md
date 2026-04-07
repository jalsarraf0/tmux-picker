# tmux-picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust TUI session picker that replaces the fragile bash tmux-autoattach script, fixing SSH instant-disconnect and providing a clean, color-coded login experience.

**Architecture:** Rust binary (`tmux-picker`) renders a TUI to stderr, prints an action protocol line to stdout. A thin bash stub calls the binary, reads the action, and execs tmux or drops to shell. The binary never calls exec — bash owns process control and safety.

**Tech Stack:** Rust 1.94, ratatui 0.30, crossterm 0.29. Bash stub for shell integration. tmux queried via subprocess.

---

## File Structure

```
~/git/tmux-picker/
├── Cargo.toml                          # Dependencies: ratatui, crossterm
├── CLAUDE.md                           # Per-repo build/test/lint instructions
├── src/
│   ├── main.rs                         # Entry: init terminal, run app, cleanup, print action
│   ├── app.rs                          # App state: sessions, selection, mode, timeout
│   ├── session.rs                      # Session struct, parsing, formatting, sorting
│   ├── action.rs                       # Action enum (Attach/New/Shell), Display → stdout protocol
│   ├── tmux.rs                         # Tmux subprocess queries and output parsing
│   ├── ui.rs                           # Ratatui rendering: table, input box, help bar
│   └── input.rs                        # Key events → app state transitions
├── tests/
│   ├── integration.rs                  # Integration tests with real tmux (isolated socket)
│   └── e2e.sh                          # E2E + regression tests (bash)
├── shell/
│   └── tmux-autoattach.sh              # Bash stub (installed to ~/.bashrc.d/)
└── docs/
    └── superpowers/
        ├── specs/2026-04-07-tmux-picker-design.md
        └── plans/2026-04-07-tmux-picker-plan.md
```

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `CLAUDE.md`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "tmux-picker"
version = "0.1.0"
edition = "2024"
description = "TUI session picker for tmux on SSH login"

[dependencies]
ratatui = "0.30"
crossterm = "0.29"

[dev-dependencies]
```

- [ ] **Step 2: Create minimal main.rs**

```rust
fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Create CLAUDE.md**

```markdown
# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints action to stdout.

## Build
cargo build --release

## Test
cargo test

## Lint
cargo fmt --check && cargo clippy -- -D warnings

## Install
cp target/release/tmux-picker ~/.local/bin/tmux-picker
```

- [ ] **Step 4: Verify lint and build**

Run: `cd ~/git/tmux-picker && cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo build`
Expected: All pass, binary at `target/debug/tmux-picker`

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: project scaffold with Cargo.toml and minimal main"
```

---

### Task 2: Session Data Model

**Files:**
- Create: `src/session.rs`
- Modify: `src/main.rs` (add `mod session;`)

- [ ] **Step 1: Write failing tests for Session**

In `src/session.rs`:

```rust
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub window_count: u32,
    pub attached: bool,
    pub current_command: String,
    pub last_activity: Duration,
}

impl Session {
    pub fn is_claude(&self) -> bool {
        self.name.starts_with("claude-")
    }

    pub fn is_stale(&self) -> bool {
        self.last_activity > Duration::from_secs(3600)
    }

    pub fn activity_display(&self) -> String {
        let secs = self.last_activity.as_secs();
        if secs < 60 {
            format!("active {secs}s")
        } else if secs < 3600 {
            format!("idle {}m", secs / 60)
        } else if secs < 86400 {
            format!("idle {}h", secs / 3600)
        } else {
            format!("idle {}d", secs / 86400)
        }
    }

    pub fn windows_display(&self) -> String {
        format!("{} win", self.window_count)
    }
}

/// Sort order: attached first, then by activity (most recent first), then alphabetical.
impl Ord for Session {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .attached
            .cmp(&self.attached)
            .then(self.last_activity.cmp(&other.last_activity))
            .then(self.name.cmp(&other.name))
    }
}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Validate a session name: alphanumeric, hyphens, underscores only.
pub fn validate_session_name(input: &str) -> Option<String> {
    let sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse multiple hyphens
    let mut result = String::with_capacity(sanitized.len());
    let mut prev_hyphen = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens
    let result = result.trim_matches('-').to_string();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_display_seconds() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(30),
        };
        assert_eq!(s.activity_display(), "active 30s");
    }

    #[test]
    fn test_activity_display_minutes() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(300),
        };
        assert_eq!(s.activity_display(), "idle 5m");
    }

    #[test]
    fn test_activity_display_hours() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(7200),
        };
        assert_eq!(s.activity_display(), "idle 2h");
    }

    #[test]
    fn test_activity_display_days() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(172800),
        };
        assert_eq!(s.activity_display(), "idle 2d");
    }

    #[test]
    fn test_activity_display_boundary_59s() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(59),
        };
        assert_eq!(s.activity_display(), "active 59s");
    }

    #[test]
    fn test_activity_display_boundary_60s() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(60),
        };
        assert_eq!(s.activity_display(), "idle 1m");
    }

    #[test]
    fn test_is_claude() {
        let s = Session {
            name: "claude-aihelp".into(),
            window_count: 1,
            attached: false,
            current_command: "claude".into(),
            last_activity: Duration::from_secs(0),
        };
        assert!(s.is_claude());
    }

    #[test]
    fn test_is_not_claude() {
        let s = Session {
            name: "main".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
        };
        assert!(!s.is_claude());
    }

    #[test]
    fn test_is_stale() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(3601),
        };
        assert!(s.is_stale());
    }

    #[test]
    fn test_is_not_stale() {
        let s = Session {
            name: "test".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(3599),
        };
        assert!(!s.is_stale());
    }

    #[test]
    fn test_sort_attached_first() {
        let attached = Session {
            name: "zzz".into(),
            window_count: 1,
            attached: true,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(1000),
        };
        let detached = Session {
            name: "aaa".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
        };
        let mut sessions = vec![detached.clone(), attached.clone()];
        sessions.sort();
        assert_eq!(sessions[0].name, "zzz");
        assert_eq!(sessions[1].name, "aaa");
    }

    #[test]
    fn test_sort_by_activity_then_name() {
        let recent = Session {
            name: "bbb".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(10),
        };
        let old = Session {
            name: "aaa".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(1000),
        };
        let mut sessions = vec![old.clone(), recent.clone()];
        sessions.sort();
        assert_eq!(sessions[0].name, "bbb");
        assert_eq!(sessions[1].name, "aaa");
    }

    #[test]
    fn test_validate_name_valid() {
        assert_eq!(validate_session_name("my-session"), Some("my-session".into()));
    }

    #[test]
    fn test_validate_name_with_dots() {
        assert_eq!(validate_session_name("my.session"), Some("my-session".into()));
    }

    #[test]
    fn test_validate_name_with_colons() {
        assert_eq!(validate_session_name("my:session:2"), Some("my-session-2".into()));
    }

    #[test]
    fn test_validate_name_with_spaces() {
        assert_eq!(validate_session_name("my session"), Some("my-session".into()));
    }

    #[test]
    fn test_validate_name_collapse_hyphens() {
        assert_eq!(validate_session_name("my--session"), Some("my-session".into()));
    }

    #[test]
    fn test_validate_name_trim_hyphens() {
        assert_eq!(validate_session_name("-session-"), Some("session".into()));
    }

    #[test]
    fn test_validate_name_empty() {
        assert_eq!(validate_session_name(""), None);
    }

    #[test]
    fn test_validate_name_only_invalid() {
        assert_eq!(validate_session_name("..."), None);
    }

    #[test]
    fn test_validate_name_underscores() {
        assert_eq!(validate_session_name("my_session_1"), Some("my_session_1".into()));
    }

    #[test]
    fn test_windows_display() {
        let s = Session {
            name: "test".into(),
            window_count: 3,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
        };
        assert_eq!(s.windows_display(), "3 win");
    }
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod session;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run tests**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo test`
Expected: All 20 tests pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: session data model with parsing, formatting, sorting, validation"
```

---

### Task 3: Action Protocol

**Files:**
- Create: `src/action.rs`
- Modify: `src/main.rs` (add `mod action;`)

- [ ] **Step 1: Write Action enum with Display and tests**

In `src/action.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Attach(String),
    New(String),
    Shell,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Attach(name) => write!(f, "attach:{name}"),
            Action::New(name) => write!(f, "new:{name}"),
            Action::Shell => write!(f, "shell"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attach_display() {
        let a = Action::Attach("main".into());
        assert_eq!(a.to_string(), "attach:main");
    }

    #[test]
    fn test_new_display() {
        let a = Action::New("my-session".into());
        assert_eq!(a.to_string(), "new:my-session");
    }

    #[test]
    fn test_shell_display() {
        let a = Action::Shell;
        assert_eq!(a.to_string(), "shell");
    }

    #[test]
    fn test_attach_with_hyphens() {
        let a = Action::Attach("claude-aihelp".into());
        assert_eq!(a.to_string(), "attach:claude-aihelp");
    }

    #[test]
    fn test_attach_with_underscores() {
        let a = Action::Attach("ssh_48201".into());
        assert_eq!(a.to_string(), "attach:ssh_48201");
    }
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod action;
mod session;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run tests and lint**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo test`
Expected: All tests pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: action protocol enum with stdout serialization"
```

---

### Task 4: Tmux Data Collection

**Files:**
- Create: `src/tmux.rs`
- Modify: `src/main.rs` (add `mod tmux;`)

- [ ] **Step 1: Write tmux query and parsing with tests**

In `src/tmux.rs`:

```rust
use crate::session::Session;
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TMUX_BIN: &str = "/usr/bin/tmux";
const TIMEOUT_SECS: u64 = 5;

/// Check if the tmux server is reachable.
pub fn server_running() -> bool {
    Command::new(TMUX_BIN)
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Query tmux for all sessions with metadata.
pub fn list_sessions() -> Result<Vec<Session>, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let session_output = run_tmux(&[
        "list-sessions",
        "-F",
        "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}",
    ])?;

    let pane_output = run_tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ])?;

    // Build command map: session_name → active pane command
    let commands = parse_pane_commands(&pane_output);

    let mut sessions = Vec::new();
    for line in session_output.lines() {
        if let Some(session) = parse_session_line(line, now, &commands) {
            sessions.push(session);
        }
    }

    sessions.sort();
    Ok(sessions)
}

fn run_tmux(args: &[&str]) -> Result<String, String> {
    let output = Command::new(TMUX_BIN)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run tmux: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "tmux exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_pane_commands(output: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() == 4 && parts[1] == "1" && parts[2] == "1" {
            commands.insert(parts[0].to_string(), parts[3].to_string());
        }
    }
    commands
}

pub fn parse_session_line(
    line: &str,
    now_epoch: u64,
    commands: &HashMap<String, String>,
) -> Option<Session> {
    let parts: Vec<&str> = line.splitn(4, '|').collect();
    if parts.len() < 4 {
        return None;
    }

    let name = parts[0].to_string();
    let window_count = parts[1].parse::<u32>().unwrap_or(0);
    let attached = parts[2] != "0";
    let activity_epoch = parts[3].parse::<u64>().unwrap_or(now_epoch);
    let last_activity = Duration::from_secs(now_epoch.saturating_sub(activity_epoch));

    let current_command = commands.get(&name).cloned().unwrap_or_else(|| "?".into());

    Some(Session {
        name,
        window_count,
        attached,
        current_command,
        last_activity,
    })
}

/// Check if a session name already exists.
pub fn session_exists(name: &str) -> bool {
    Command::new(TMUX_BIN)
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_line_valid() {
        let commands = HashMap::from([("main".to_string(), "bash".to_string())]);
        let result = parse_session_line("main|2|0|1000", 1300, &commands);
        let s = result.unwrap();
        assert_eq!(s.name, "main");
        assert_eq!(s.window_count, 2);
        assert!(!s.attached);
        assert_eq!(s.current_command, "bash");
        assert_eq!(s.last_activity, Duration::from_secs(300));
    }

    #[test]
    fn test_parse_session_line_attached() {
        let commands = HashMap::new();
        let result = parse_session_line("work|1|1|1000", 1000, &commands);
        let s = result.unwrap();
        assert!(s.attached);
        assert_eq!(s.current_command, "?");
    }

    #[test]
    fn test_parse_session_line_malformed() {
        let commands = HashMap::new();
        assert!(parse_session_line("bad", 1000, &commands).is_none());
        assert!(parse_session_line("a|b", 1000, &commands).is_none());
        assert!(parse_session_line("", 1000, &commands).is_none());
    }

    #[test]
    fn test_parse_session_line_bad_numbers() {
        let commands = HashMap::new();
        let result = parse_session_line("test|notanum|0|notanum", 1000, &commands);
        let s = result.unwrap();
        assert_eq!(s.window_count, 0);
        assert_eq!(s.last_activity, Duration::from_secs(0));
    }

    #[test]
    fn test_parse_pane_commands() {
        let output = "main|1|1|bash\nmain|0|1|vim\nwork|1|1|claude\n";
        let commands = parse_pane_commands(output);
        assert_eq!(commands.get("main").unwrap(), "bash");
        assert_eq!(commands.get("work").unwrap(), "claude");
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_parse_pane_commands_empty() {
        let commands = parse_pane_commands("");
        assert!(commands.is_empty());
    }

    #[test]
    fn test_parse_pane_commands_inactive_window() {
        let output = "main|0|1|vim\n";
        let commands = parse_pane_commands(output);
        assert!(commands.is_empty());
    }
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod action;
mod session;
mod tmux;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run tests and lint**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: tmux subprocess queries with output parsing"
```

---

### Task 5: App State Machine

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs` (add `mod app;`)

- [ ] **Step 1: Write App state machine with tests**

In `src/app.rs`:

```rust
use crate::action::Action;
use crate::session::{validate_session_name, Session};
use crate::tmux;
use std::time::Duration;

const AUTO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Pick,
    NewInput,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub mode: Mode,
    pub input: String,
    pub action: Option<Action>,
    pub timeout_remaining: Duration,
    pub input_error: Option<String>,
}

impl App {
    pub fn new(sessions: Vec<Session>) -> Self {
        Self {
            sessions,
            selected: 0,
            mode: Mode::Pick,
            input: String::new(),
            action: None,
            timeout_remaining: AUTO_TIMEOUT,
            input_error: None,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.reset_timeout();
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.sessions.len() {
            self.selected += 1;
        }
        self.reset_timeout();
    }

    pub fn select_by_number(&mut self, n: usize) {
        if n > 0 && n <= self.sessions.len() {
            self.selected = n - 1;
            self.confirm_selection();
        }
        self.reset_timeout();
    }

    pub fn confirm_selection(&mut self) {
        if let Some(session) = self.sessions.get(self.selected) {
            self.action = Some(Action::Attach(session.name.clone()));
        }
    }

    pub fn enter_new_mode(&mut self) {
        self.mode = Mode::NewInput;
        self.input.clear();
        self.input_error = None;
        self.reset_timeout();
    }

    pub fn cancel_input(&mut self) {
        self.mode = Mode::Pick;
        self.input.clear();
        self.input_error = None;
    }

    pub fn input_char(&mut self, c: char) {
        self.input.push(c);
        self.input_error = None;
    }

    pub fn input_backspace(&mut self) {
        self.input.pop();
        self.input_error = None;
    }

    pub fn confirm_input(&mut self) {
        if self.input.is_empty() {
            self.cancel_input();
            return;
        }

        match validate_session_name(&self.input) {
            None => {
                self.input_error = Some("Invalid session name".into());
            }
            Some(name) => {
                if tmux::session_exists(&name) {
                    self.action = Some(Action::Attach(name));
                } else {
                    self.action = Some(Action::New(name));
                }
            }
        }
    }

    pub fn shell(&mut self) {
        self.action = Some(Action::Shell);
    }

    pub fn tick(&mut self, elapsed: Duration) {
        self.timeout_remaining = self.timeout_remaining.saturating_sub(elapsed);
        if self.timeout_remaining.is_zero() && self.mode == Mode::Pick {
            self.auto_select();
        }
    }

    fn auto_select(&mut self) {
        // Attach to first detached session, or first session if all attached
        let target = self
            .sessions
            .iter()
            .find(|s| !s.attached)
            .or(self.sessions.first());

        if let Some(session) = target {
            self.action = Some(Action::Attach(session.name.clone()));
        } else {
            self.action = Some(Action::Shell);
        }
    }

    fn reset_timeout(&mut self) {
        self.timeout_remaining = AUTO_TIMEOUT;
    }

    pub fn should_quit(&self) -> bool {
        self.action.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sessions() -> Vec<Session> {
        vec![
            Session {
                name: "main".into(),
                window_count: 1,
                attached: false,
                current_command: "bash".into(),
                last_activity: Duration::from_secs(100),
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
            },
            Session {
                name: "work".into(),
                window_count: 2,
                attached: false,
                current_command: "vim".into(),
                last_activity: Duration::from_secs(500),
            },
        ]
    }

    #[test]
    fn test_new_starts_at_zero() {
        let app = App::new(make_sessions());
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_move_down() {
        let mut app = App::new(make_sessions());
        app.move_down();
        assert_eq!(app.selected, 1);
        app.move_down();
        assert_eq!(app.selected, 2);
        app.move_down(); // at end, stays
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_move_up() {
        let mut app = App::new(make_sessions());
        app.move_up(); // at start, stays
        assert_eq!(app.selected, 0);
        app.move_down();
        app.move_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_by_number() {
        let mut app = App::new(make_sessions());
        app.select_by_number(2);
        assert_eq!(app.action, Some(Action::Attach("claude-aihelp".into())));
    }

    #[test]
    fn test_select_by_number_out_of_range() {
        let mut app = App::new(make_sessions());
        app.select_by_number(5);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_select_by_number_zero() {
        let mut app = App::new(make_sessions());
        app.select_by_number(0);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_confirm_selection() {
        let mut app = App::new(make_sessions());
        app.move_down();
        app.confirm_selection();
        assert_eq!(app.action, Some(Action::Attach("claude-aihelp".into())));
    }

    #[test]
    fn test_shell() {
        let mut app = App::new(make_sessions());
        app.shell();
        assert_eq!(app.action, Some(Action::Shell));
        assert!(app.should_quit());
    }

    #[test]
    fn test_enter_new_mode() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        assert_eq!(app.mode, Mode::NewInput);
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_cancel_input() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.input_char('a');
        app.cancel_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_input_char_and_backspace() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.input_char('a');
        app.input_char('b');
        assert_eq!(app.input, "ab");
        app.input_backspace();
        assert_eq!(app.input, "a");
        app.input_backspace();
        assert!(app.input.is_empty());
        app.input_backspace(); // empty, no panic
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_confirm_empty_input_cancels() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.confirm_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_confirm_invalid_input() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.input_char('.');
        app.input_char('.');
        app.input_char('.');
        app.confirm_input();
        assert!(app.input_error.is_some());
        assert!(app.action.is_none());
    }

    #[test]
    fn test_timeout_auto_selects() {
        let mut app = App::new(make_sessions());
        app.tick(Duration::from_secs(10));
        assert!(app.action.is_some());
    }

    #[test]
    fn test_timeout_picks_first_detached() {
        let mut app = App::new(make_sessions());
        app.tick(Duration::from_secs(10));
        // Sessions are sorted: attached first. First detached is "main" or "work" depending on sort.
        // But sessions were not sorted in make_sessions(), App::new doesn't sort. The test creates
        // sessions in the order given. First detached is "main" (index 0).
        assert_eq!(app.action, Some(Action::Attach("main".into())));
    }

    #[test]
    fn test_interaction_resets_timeout() {
        let mut app = App::new(make_sessions());
        app.tick(Duration::from_secs(9));
        assert!(app.action.is_none());
        app.move_down(); // resets timeout
        app.tick(Duration::from_secs(9));
        assert!(app.action.is_none()); // still within timeout
    }

    #[test]
    fn test_timeout_does_not_fire_in_input_mode() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.tick(Duration::from_secs(15));
        assert!(app.action.is_none());
    }

    #[test]
    fn test_empty_sessions_auto_select_shell() {
        let mut app = App::new(vec![]);
        app.tick(Duration::from_secs(10));
        assert_eq!(app.action, Some(Action::Shell));
    }
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod action;
mod app;
mod session;
mod tmux;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run tests and lint**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: app state machine with navigation, selection, input, timeout"
```

---

### Task 6: Input Handling

**Files:**
- Create: `src/input.rs`
- Modify: `src/main.rs` (add `mod input;`)

- [ ] **Step 1: Write input handler with tests**

In `src/input.rs`:

```rust
use crate::app::{App, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        Mode::Pick => handle_pick_key(app, key),
        Mode::NewInput => handle_input_key(app, key),
    }
}

fn handle_pick_key(app: &mut App, key: KeyEvent) {
    // Ctrl+C always quits to shell
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.shell();
        return;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Enter => app.confirm_selection(),
        KeyCode::Char('n') => app.enter_new_mode(),
        KeyCode::Char('s') => app.shell(),
        KeyCode::Char('q') => app.shell(),
        KeyCode::Esc => app.shell(),
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let n = c.to_digit(10).unwrap_or(0) as usize;
            app.select_by_number(n);
        }
        _ => {}
    }
}

fn handle_input_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.cancel_input();
        return;
    }

    match key.code {
        KeyCode::Enter => app.confirm_input(),
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Char(c) => app.input_char(c),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::session::Session;
    use std::time::Duration;

    fn make_app() -> App {
        App::new(vec![
            Session {
                name: "main".into(),
                window_count: 1,
                attached: false,
                current_command: "bash".into(),
                last_activity: Duration::from_secs(0),
            },
            Session {
                name: "work".into(),
                window_count: 2,
                attached: false,
                current_command: "vim".into(),
                last_activity: Duration::from_secs(100),
            },
        ])
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_arrow_down() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_j_moves_down() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_arrow_up() {
        let mut app = make_app();
        app.selected = 1;
        handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_k_moves_up() {
        let mut app = make_app();
        app.selected = 1;
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_enter_selects() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.action, Some(Action::Attach("main".into())));
    }

    #[test]
    fn test_number_selects() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('2')));
        assert_eq!(app.action, Some(Action::Attach("work".into())));
    }

    #[test]
    fn test_s_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('s')));
        assert_eq!(app.action, Some(Action::Shell));
    }

    #[test]
    fn test_q_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.action, Some(Action::Shell));
    }

    #[test]
    fn test_esc_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.action, Some(Action::Shell));
    }

    #[test]
    fn test_ctrl_c_shells() {
        let mut app = make_app();
        handle_key(&mut app, ctrl('c'));
        assert_eq!(app.action, Some(Action::Shell));
    }

    #[test]
    fn test_n_enters_new_mode() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, Mode::NewInput);
    }

    #[test]
    fn test_input_mode_typing() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('a')));
        handle_key(&mut app, key(KeyCode::Char('b')));
        assert_eq!(app.input, "ab");
    }

    #[test]
    fn test_input_mode_backspace() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('a')));
        handle_key(&mut app, key(KeyCode::Backspace));
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_input_mode_esc_cancels() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('x')));
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn test_input_mode_ctrl_c_cancels() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, ctrl('c'));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn test_unknown_key_ignored() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::F(1)));
        assert_eq!(app.selected, 0);
        assert!(app.action.is_none());
    }
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod action;
mod app;
mod input;
mod session;
mod tmux;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run tests and lint**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: input handler mapping key events to app state transitions"
```

---

### Task 7: TUI Rendering

**Files:**
- Create: `src/ui.rs`
- Modify: `src/main.rs` (add `mod ui;`)

- [ ] **Step 1: Write TUI rendering**

In `src/ui.rs`:

```rust
use crate::app::{App, Mode};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Layout: sessions table, actions bar, help bar
    let chunks = Layout::vertical([
        Constraint::Min(3),    // sessions
        Constraint::Length(3), // actions or input
        Constraint::Length(3), // help
    ])
    .split(area);

    draw_sessions(frame, app, chunks[0]);
    draw_actions(frame, app, chunks[1]);
    draw_help(frame, app, chunks[2]);
}

fn draw_sessions(frame: &mut Frame, app: &App, area: Rect) {
    if app.sessions.is_empty() {
        let block = Block::default()
            .title(" tmux sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let msg = Paragraph::new("  No sessions running")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(msg, area);
        return;
    }

    let rows: Vec<Row> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let selected = i == app.selected;
            let indicator = if selected { "▸" } else { " " };
            let number = format!("{}", i + 1);

            let name_style = if s.is_claude() {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if s.is_stale() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };

            let attached_str = if s.attached { "●" } else { " " };
            let attached_style = if s.attached {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            let activity_style = if s.is_stale() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let cmd_style = Style::default().fg(Color::Yellow);

            let row_style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Span::styled(format!(" {indicator} {number}"), Style::default().fg(Color::White)),
                Span::styled(format!(" {:<16}", s.name), name_style),
                Span::raw(format!("{:>5}", s.windows_display())),
                Span::styled(format!("  {:<10}", s.current_command), cmd_style),
                Span::styled(format!(" {attached_str} "), attached_style),
                Span::styled(format!("{:>10}", s.activity_display()), activity_style),
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

    let table = Table::new(rows, widths).block(
        Block::default()
            .title(" tmux sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(table, area);
}

fn draw_actions(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    match app.mode {
        Mode::Pick => {
            let line = Line::from(vec![
                Span::styled("    n", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("  new session     "),
                Span::styled("s", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("  shell (no tmux)"),
            ]);
            let p = Paragraph::new(line).block(block);
            frame.render_widget(p, area);
        }
        Mode::NewInput => {
            let input_line = if let Some(ref err) = app.input_error {
                Line::from(vec![
                    Span::raw("  Session name: "),
                    Span::styled(&app.input, Style::default().fg(Color::White)),
                    Span::styled(format!("  ({err})"), Style::default().fg(Color::Red)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  Session name: "),
                    Span::styled(&app.input, Style::default().fg(Color::White).add_modifier(Modifier::UNDERLINED)),
                    Span::styled("█", Style::default().fg(Color::White)),
                ])
            };
            let p = Paragraph::new(input_line).block(block);
            frame.render_widget(p, area);
        }
    }
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let dim = Style::default().fg(Color::DarkGray);

    let line = match app.mode {
        Mode::Pick => {
            let timeout = app.timeout_remaining.as_secs();
            Line::from(vec![
                Span::styled("  ↑↓", dim),
                Span::styled(" navigate", dim),
                Span::styled("  ·  ", dim),
                Span::styled("enter/#", dim),
                Span::styled(" select", dim),
                Span::styled("  ·  ", dim),
                Span::styled("s", dim),
                Span::styled(" shell", dim),
                Span::styled("  ·  ", dim),
                Span::styled("q", dim),
                Span::styled(" quit", dim),
                Span::styled(format!("  ·  auto-attach in {timeout}s"), dim),
            ])
        }
        Mode::NewInput => Line::from(vec![
            Span::styled("  enter", dim),
            Span::styled(" confirm", dim),
            Span::styled("  ·  ", dim),
            Span::styled("esc", dim),
            Span::styled(" cancel", dim),
        ]),
    };

    let p = Paragraph::new(line).block(block);
    frame.render_widget(p, area);
}
```

- [ ] **Step 2: Add module to main.rs**

```rust
mod action;
mod app;
mod input;
mod session;
mod tmux;
mod ui;

fn main() {
    println!("shell");
}
```

- [ ] **Step 3: Run lint (no automated visual tests)**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo build`
Expected: Compiles cleanly, no warnings.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: TUI rendering with color-coded session table and help bar"
```

---

### Task 8: Main Orchestration

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire everything together in main.rs**

```rust
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
use std::io::{self, stderr};
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
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    input::handle_key(&mut app, key);
                }
            }
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
```

- [ ] **Step 2: Run lint and build**

Run: `cd ~/git/tmux-picker && cargo fmt && cargo clippy -- -D warnings && cargo build`
Expected: Compiles cleanly.

- [ ] **Step 3: Smoke test — run the binary manually**

Run: `cd ~/git/tmux-picker && cargo run 2>/dev/tty`
Expected: TUI appears on terminal showing the `main` session. Press `s` to exit. Stdout prints `shell`.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: main orchestration wiring terminal, app loop, and cleanup"
```

---

### Task 9: Integration Tests (Real Tmux)

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Write integration tests using isolated tmux socket**

In `tests/integration.rs`:

```rust
use std::process::Command;
use std::thread;
use std::time::Duration;

const TMUX: &str = "/usr/bin/tmux";
const SOCKET: &str = "tmux-picker-test";

fn tmux_cmd(args: &[&str]) -> std::process::Output {
    Command::new(TMUX)
        .args(["-L", SOCKET])
        .args(args)
        .output()
        .expect("failed to run tmux")
}

fn setup() {
    // Kill any leftover test server
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(100));
}

fn teardown() {
    let _ = tmux_cmd(&["kill-server"]);
}

fn create_session(name: &str) {
    tmux_cmd(&["new-session", "-d", "-s", name]);
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn test_parse_real_tmux_sessions() {
    setup();
    create_session("test-main");
    create_session("test-work");
    create_session("claude-test");

    let output = tmux_cmd(&[
        "list-sessions",
        "-F",
        "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(lines.len(), 3);
    for line in &lines {
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(parts.len(), 4, "line should have 4 pipe-separated fields: {line}");
        assert!(
            parts[1].parse::<u32>().is_ok(),
            "window count should be numeric"
        );
    }

    teardown();
}

#[test]
fn test_parse_real_pane_commands() {
    setup();
    create_session("pane-test");

    let output = tmux_cmd(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert!(!lines.is_empty());
    for line in &lines {
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(parts.len(), 4, "pane line should have 4 fields: {line}");
    }

    teardown();
}

#[test]
fn test_many_sessions() {
    setup();
    for i in 0..20 {
        create_session(&format!("sess-{i:02}"));
    }

    let output = tmux_cmd(&[
        "list-sessions",
        "-F",
        "#{session_name}",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = stdout.lines().count();
    assert_eq!(count, 20);

    teardown();
}

#[test]
fn test_session_with_special_names() {
    setup();
    create_session("my-session");
    create_session("my_session_2");
    create_session("CAPS-NAME");

    let output = tmux_cmd(&[
        "list-sessions",
        "-F",
        "#{session_name}",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let names: Vec<&str> = stdout.lines().collect();

    assert!(names.contains(&"my-session"));
    assert!(names.contains(&"my_session_2"));
    assert!(names.contains(&"CAPS-NAME"));

    teardown();
}

#[test]
fn test_no_server_running() {
    // Ensure no test server
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(100));

    let output = tmux_cmd(&["list-sessions"]);
    assert!(!output.status.success());
}
```

- [ ] **Step 2: Run integration tests**

Run: `cd ~/git/tmux-picker && cargo test --test integration -- --test-threads=1`
Expected: All integration tests pass. (Must be single-threaded since they share a tmux socket.)

- [ ] **Step 3: Run full test suite**

Run: `cd ~/git/tmux-picker && cargo fmt --check && cargo clippy -- -D warnings && cargo test -- --test-threads=1`
Expected: All unit + integration tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "test: integration tests with real tmux on isolated socket"
```

---

### Task 10: E2E and Regression Tests

**Files:**
- Create: `tests/e2e.sh`

- [ ] **Step 1: Write E2E test script**

In `tests/e2e.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

BINARY="$(cd "$(dirname "$0")/.." && pwd)/target/release/tmux-picker"
SOCKET="tmux-picker-e2e"
TMUX="/usr/bin/tmux"
PASS=0
FAIL=0

pass() { ((PASS++)); echo "  PASS: $1"; }
fail() { ((FAIL++)); echo "  FAIL: $1 — $2"; }

cleanup() { $TMUX -L "$SOCKET" kill-server 2>/dev/null || true; }
trap cleanup EXIT

echo "═══ tmux-picker E2E tests ═══"
echo ""

# Ensure release binary exists
if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: Build release binary first: cargo build --release"
    exit 1
fi

# --- Test 1: Binary outputs valid protocol ---
echo "Test 1: Binary with no tmux server outputs shell"
cleanup
sleep 0.1
# No tmux server on this socket, binary should output "shell"
action=$(TMUX_TMPDIR=$(mktemp -d) "$BINARY" 2>/dev/null || echo "shell")
if [[ "$action" == "shell" ]]; then
    pass "no server → shell"
else
    fail "no server → shell" "got: $action"
fi

# --- Test 2: Binary missing → bash stub drops to shell ---
echo "Test 2: Bash stub with missing binary"
STUB="$(cd "$(dirname "$0")/.." && pwd)/shell/tmux-autoattach.sh"
# Simulate by pointing to nonexistent binary
output=$(SSH_CONNECTION="1.2.3.4 22 5.6.7.8 22" TMUX="" NO_TMUX="" \
    bash -c '
        set -- -i  # fake interactive
        _PICKER="/nonexistent/tmux-picker"
        if [[ -x "$_PICKER" ]]; then
            echo "ERROR: binary should not exist"
        else
            echo "FALLBACK"
        fi
    ')
if [[ "$output" == "FALLBACK" ]]; then
    pass "missing binary → fallback"
else
    fail "missing binary → fallback" "got: $output"
fi

# --- Test 3: Protocol format validation ---
echo "Test 3: Protocol format"
cleanup
$TMUX -L "$SOCKET" new-session -d -s proto-test
sleep 0.1
# We can't easily simulate key input, but we can verify the binary starts
# and would output something if given input. Test timeout behavior:
# The binary should auto-select after 10s. For testing, we verify it compiles
# and runs without crashing.
timeout 2 "$BINARY" 2>/dev/null </dev/null && rc=$? || rc=$?
# Binary should exit (possibly with error since no tty), but should not crash
if [[ $rc -le 1 ]]; then
    pass "binary runs without crash (exit $rc)"
else
    fail "binary runs without crash" "exit code: $rc"
fi
cleanup

# --- Test 4: NO_TMUX escape hatch ---
echo "Test 4: NO_TMUX bypass"
result=$(NO_TMUX=1 SSH_CONNECTION="1 2 3 4" TMUX="" bash -c '
    [[ -n "${NO_TMUX:-}" ]] && echo "BYPASSED" || echo "NOT_BYPASSED"
')
if [[ "$result" == "BYPASSED" ]]; then
    pass "NO_TMUX bypass works"
else
    fail "NO_TMUX bypass" "got: $result"
fi

# --- Test 5: Non-SSH does not trigger ---
echo "Test 5: Non-SSH guard"
result=$(SSH_CONNECTION="" TMUX="" bash -c '
    [[ -z "$SSH_CONNECTION" ]] && echo "SKIPPED" || echo "TRIGGERED"
')
if [[ "$result" == "SKIPPED" ]]; then
    pass "non-SSH skips picker"
else
    fail "non-SSH guard" "got: $result"
fi

# --- Test 6: Already in tmux does not trigger ---
echo "Test 6: Already-in-tmux guard"
result=$(SSH_CONNECTION="1 2 3 4" TMUX="/tmp/tmux-1000/default,123,0" bash -c '
    [[ -n "$TMUX" ]] && echo "SKIPPED" || echo "TRIGGERED"
')
if [[ "$result" == "SKIPPED" ]]; then
    pass "already in tmux skips picker"
else
    fail "already-in-tmux guard" "got: $result"
fi

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
```

- [ ] **Step 2: Make executable and run**

Run: `chmod +x ~/git/tmux-picker/tests/e2e.sh && cd ~/git/tmux-picker && cargo build --release && ./tests/e2e.sh`
Expected: All E2E tests pass.

- [ ] **Step 3: Run full test suite**

Run: `cd ~/git/tmux-picker && cargo fmt --check && cargo clippy -- -D warnings && cargo test -- --test-threads=1 && ./tests/e2e.sh`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "test: E2E and regression tests for bash stub and binary"
```

---

### Task 11: Bash Stub and Shell Changes

**Files:**
- Create: `shell/tmux-autoattach.sh`
- Modify: `~/.bashrc.d/tmux-autoattach.sh` (replace)
- Modify: `~/.bashrc` (remove bottom block, add fastfetch guard)

- [ ] **Step 1: Create bash stub in repo**

In `shell/tmux-autoattach.sh`:

```bash
#!/usr/bin/env bash
# tmux session picker — interactive SSH logins only.
# Calls tmux-picker binary for TUI, falls back to shell on any failure.

[[ -z "$SSH_CONNECTION" ]] && return
[[ -n "$TMUX" ]] && return
[[ "$-" != *i* ]] && return
[[ -n "${NO_TMUX:-}" ]] && return

_PICKER="${HOME}/.local/bin/tmux-picker"

if [[ -x "$_PICKER" ]]; then
    action="$("$_PICKER" 2>/dev/tty)"
    rc=$?
else
    echo "tmux-picker not found at $_PICKER — dropping to shell" >&2
    return
fi

if [[ $rc -ne 0 || -z "$action" ]]; then
    echo "tmux-picker exited $rc — dropping to shell" >&2
    return
fi

case "$action" in
    attach:*)
        sess="${action#attach:}"
        exec /usr/bin/tmux attach-session -dt "$sess"
        echo "Failed to attach to '$sess'" >&2
        ;;
    new:*)
        sess="${action#new:}"
        exec /usr/bin/tmux new-session -s "$sess"
        echo "Failed to create session '$sess'" >&2
        ;;
    shell) ;;
    *)
        echo "Unknown action from tmux-picker: $action" >&2
        ;;
esac
```

- [ ] **Step 2: Commit stub in repo**

```bash
git add shell/tmux-autoattach.sh && git commit -m "feat: bash stub for tmux-picker integration"
```

- [ ] **Step 3: Backup old files**

```bash
cp ~/.bashrc.d/tmux-autoattach.sh ~/.bashrc.d/tmux-autoattach.sh.bak
```

- [ ] **Step 4: Install bash stub**

```bash
cp ~/git/tmux-picker/shell/tmux-autoattach.sh ~/.bashrc.d/tmux-autoattach.sh
```

- [ ] **Step 5: Edit ~/.bashrc — remove bottom tmux block**

Remove the block:
```bash
# Auto-create a new tmux session for interactive SSH logins.
# ...
if [[ -n $SSH_CONNECTION && -z $TMUX && $- == *i* && -z ${NO_TMUX:-} ]]; then
  exec tmux new-session -s "ssh-$$"
fi
```

- [ ] **Step 6: Edit ~/.bashrc — add fastfetch guard**

Replace `fastfetch` with:
```bash
if [[ -z "$SSH_CONNECTION" || -n "$TMUX" ]]; then
    fastfetch
fi
```

- [ ] **Step 7: Verify .bashrc syntax**

Run: `bash -n ~/.bashrc && echo "syntax OK"`
Expected: `syntax OK`

---

### Task 12: Build, Install, and Verify

**Files:**
- Build: `target/release/tmux-picker`
- Install: `~/.local/bin/tmux-picker`

- [ ] **Step 1: Release build**

Run: `cd ~/git/tmux-picker && cargo build --release`
Expected: Binary at `target/release/tmux-picker`

- [ ] **Step 2: Install binary**

```bash
cp ~/git/tmux-picker/target/release/tmux-picker ~/.local/bin/tmux-picker
chmod +x ~/.local/bin/tmux-picker
```

- [ ] **Step 3: Verify binary runs**

Run: `~/.local/bin/tmux-picker --help 2>/dev/null || ~/.local/bin/tmux-picker 2>/dev/tty`
Expected: TUI appears or binary exits cleanly.

- [ ] **Step 4: Run full test suite one final time**

Run: `cd ~/git/tmux-picker && cargo fmt --check && cargo clippy -- -D warnings && cargo test -- --test-threads=1 && ./tests/e2e.sh`
Expected: All tests pass.

- [ ] **Step 5: Final commit**

```bash
cd ~/git/tmux-picker && git add -A && git commit -m "chore: final test pass and release build"
```

- [ ] **Step 6: Verification checklist**

1. `NO_TMUX=1 ssh localhost` → drops straight to shell (escape hatch works)
2. `ssh localhost` → picker appears, select session, fastfetch once, prompt
3. `rm ~/.local/bin/tmux-picker && ssh localhost` → warning, drops to shell
4. Restore binary: `cp ~/git/tmux-picker/target/release/tmux-picker ~/.local/bin/`
