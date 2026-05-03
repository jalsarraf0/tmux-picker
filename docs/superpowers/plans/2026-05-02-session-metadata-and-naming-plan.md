# Session Metadata + Smart Naming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-session metadata (label/project/purpose) stored in tmux user-options, with `label`/`show`/`auto` CLI subcommands and label-aware picker UI.

**Architecture:** Tmux user-options as single source of truth. New `metadata` module wraps tmux option calls. New `clap`-based subcommand router in `main.rs` preserves existing TUI behavior when invoked with no args. `Session` gains an optional `metadata` field populated via an extended 8-field `list-sessions -F` format. UI adds a 1-line detail row under the selected session when metadata exists.

**Tech Stack:** Rust 2024 edition, ratatui 0.30, crossterm 0.29, clap 4 (new dep), tmux 3.5+ on Fedora 43.

**Spec:** `docs/superpowers/specs/2026-05-02-session-metadata-and-naming-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Add `clap` dependency. |
| `src/metadata.rs` | Create | `Metadata` struct + read/write/clear/auto-detect logic. |
| `src/tmux.rs` | Modify | Add `set_user_option`, `unset_user_option`, `pane_current_path`, extend `parse_session_line` to 8 fields. |
| `src/session.rs` | Modify | Add `metadata: Option<Metadata>` field to `Session`. |
| `src/cli.rs` | Create | clap-derived CLI struct with `Label`/`Show`/`Auto` subcommands. |
| `src/main.rs` | Modify | Subcommand routing; existing TUI logic moves into `run_tui()`. |
| `src/ui.rs` | Modify | Label-aware name display + 1-line detail row under selected. |
| `src/lib.rs` | Create | Public re-exports so integration tests can call into the crate. |
| `tests/integration.rs` | Modify | Add tests for label/show/auto round-trip on real tmux. |
| `tests/e2e.sh` | Modify | Add subcommand smoke tests + `--help` check. |
| `README.md` | Modify | Document subcommands, metadata model, claude integration. |

---

## Task 1: Add `clap` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Edit Cargo.toml**

Replace the `[dependencies]` block:

```toml
[dependencies]
ratatui = "0.30"
crossterm = "0.29"
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --release`
Expected: success, new clap crates in `Cargo.lock`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add clap 4 with derive feature for subcommand parsing"
```

---

## Task 2: Convert binary to library + binary

The new `metadata` module needs to be reachable from `tests/integration.rs`. The simplest fix is exposing modules via a library crate.

**Files:**
- Create: `src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/lib.rs`**

```rust
pub mod action;
pub mod app;
pub mod input;
pub mod session;
pub mod tmux;
pub mod ui;
```

- [ ] **Step 2: Update Cargo.toml to declare both targets**

Append to `Cargo.toml` (after `[dependencies]`):

```toml
[lib]
name = "tmux_picker"
path = "src/lib.rs"

[[bin]]
name = "tmux-picker"
path = "src/main.rs"
```

- [ ] **Step 3: Strip module declarations from `src/main.rs`**

Replace the top of `src/main.rs` from line 1-9 with:

```rust
use tmux_picker::action::Action;
use tmux_picker::app::App;
use tmux_picker::{input, tmux, ui};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use std::io::stderr;
use std::time::{Duration, Instant};
```

- [ ] **Step 4: Verify build + tests still pass**

Run: `cargo build --release && cargo test`
Expected: all existing tests pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs
git commit -m "refactor: split into lib + bin so integration tests reach modules"
```

---

## Task 3: Create `Metadata` struct + serializer

**Files:**
- Create: `src/metadata.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/metadata.rs` with the struct + tests**

```rust
//! Per-session metadata stored as tmux user-options.

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Metadata {
    pub label: Option<String>,
    pub project: Option<String>,
    pub purpose: Option<String>,
    pub label_at: Option<u64>,
}

impl Metadata {
    /// True if every field is None.
    pub fn is_empty(&self) -> bool {
        self.label.is_none()
            && self.project.is_none()
            && self.purpose.is_none()
            && self.label_at.is_none()
    }

    /// Serialize as TOML keyed by `session`.
    pub fn to_toml(&self, session: &str) -> String {
        let mut out = format!("session = {}\n", toml_string(session));
        if let Some(ref v) = self.label {
            out.push_str(&format!("label = {}\n", toml_string(v)));
        }
        if let Some(ref v) = self.project {
            out.push_str(&format!("project = {}\n", toml_string(v)));
        }
        if let Some(ref v) = self.purpose {
            out.push_str(&format!("purpose = {}\n", toml_string(v)));
        }
        if let Some(v) = self.label_at {
            out.push_str(&format!("label_at = {}\n", v));
        }
        out
    }
}

/// Quote a string for TOML basic string output.
/// Escapes `"`, `\`, and control chars per TOML spec.
fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_empty_true_for_default() {
        assert!(Metadata::default().is_empty());
    }

    #[test]
    fn is_empty_false_when_label_set() {
        let m = Metadata {
            label: Some("x".into()),
            ..Default::default()
        };
        assert!(!m.is_empty());
    }

    #[test]
    fn toml_with_no_metadata_only_session_line() {
        let m = Metadata::default();
        assert_eq!(m.to_toml("main"), "session = \"main\"\n");
    }

    #[test]
    fn toml_with_full_metadata() {
        let m = Metadata {
            label: Some("Refactoring auth".into()),
            project: Some("/home/u/git/app".into()),
            purpose: Some("PR #234".into()),
            label_at: Some(1_746_201_600),
        };
        let out = m.to_toml("claude-app");
        assert!(out.contains("session = \"claude-app\""));
        assert!(out.contains("label = \"Refactoring auth\""));
        assert!(out.contains("project = \"/home/u/git/app\""));
        assert!(out.contains("purpose = \"PR #234\""));
        assert!(out.contains("label_at = 1746201600"));
    }

    #[test]
    fn toml_string_escapes_quotes() {
        assert_eq!(toml_string("a \"b\" c"), "\"a \\\"b\\\" c\"");
    }

    #[test]
    fn toml_string_escapes_backslash_and_newline() {
        assert_eq!(toml_string("a\\b\nc"), "\"a\\\\b\\nc\"");
    }
}
```

- [ ] **Step 2: Add module to `src/lib.rs`**

```rust
pub mod action;
pub mod app;
pub mod input;
pub mod metadata;
pub mod session;
pub mod tmux;
pub mod ui;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib metadata`
Expected: all 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/metadata.rs
git commit -m "feat(metadata): add Metadata struct with TOML serialization"
```

---

## Task 4: Add tmux user-option helpers

**Files:**
- Modify: `src/tmux.rs`

- [ ] **Step 1: Append helpers to `src/tmux.rs`** (after the existing `session_exists` function)

```rust
/// Set a tmux user-option on a session.
/// `key` must NOT include the `@` prefix; it is added here.
pub fn set_user_option(session: &str, key: &str, value: &str) -> Result<(), String> {
    let opt = format!("@{key}");
    run_tmux(&["set-option", "-t", session, &opt, value]).map(|_| ())
}

/// Unset a tmux user-option on a session.
pub fn unset_user_option(session: &str, key: &str) -> Result<(), String> {
    let opt = format!("@{key}");
    run_tmux(&["set-option", "-t", session, "-u", &opt]).map(|_| ())
}

/// Return the value of a tmux user-option, or None if unset.
pub fn get_user_option(session: &str, key: &str) -> Option<String> {
    let opt = format!("@{key}");
    let out = run_tmux(&["show-options", "-t", session, "-v", &opt]).ok()?;
    let trimmed = out.trim_end_matches('\n').to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

/// Get the current pane working directory for a session.
/// Uses the session's active window's active pane.
pub fn pane_current_path(session: &str) -> Result<String, String> {
    let out = run_tmux(&[
        "display-message",
        "-t",
        session,
        "-p",
        "#{pane_current_path}",
    ])?;
    Ok(out.trim_end_matches('\n').to_string())
}
```

- [ ] **Step 2: Run existing tests to confirm nothing broke**

Run: `cargo test --lib tmux`
Expected: all existing tests still pass.

- [ ] **Step 3: Commit**

```bash
git add src/tmux.rs
git commit -m "feat(tmux): add user-option get/set/unset and pane_current_path helpers"
```

---

## Task 5: Extend `parse_session_line` to 8 fields with backward compat

**Files:**
- Modify: `src/tmux.rs`
- Modify: `src/session.rs`

- [ ] **Step 1: Add `metadata` field to `Session` struct**

In `src/session.rs`, replace the struct definition (lines 4-11) with:

```rust
use crate::metadata::Metadata;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Session {
    pub name: String,
    pub window_count: u32,
    pub attached: bool,
    pub current_command: String,
    pub last_activity: Duration,
    pub metadata: Option<Metadata>,
}
```

- [ ] **Step 2: Update existing `Session` literals in tests**

`src/session.rs` test helper `make()` (around line 119): add `metadata: None,` to the struct literal.

`src/session.rs` `windows_display_one` and `windows_display_many` tests (lines ~395, ~407): add `metadata: None,`.

`src/app.rs` `make_sessions()` (around line 202): add `metadata: None,` to all three Session literals.

`src/input.rs` `make_app()` (around line 76): add `metadata: None,` to both Session literals.

- [ ] **Step 3: Update `parse_session_line` to accept 8 fields, with 4-field backward compat**

In `src/tmux.rs`, replace the existing `parse_session_line` function with:

```rust
/// Parse one line of `list-sessions` output into a `Session`.
///
/// Format (8 fields): `name|windows|attached|activity|label|project|purpose|label_at`
/// Backward-compat: 4-field input (no metadata) also accepted; metadata = None.
///
/// Returns `None` if the line has fewer than 4 `|`-separated fields or empty
/// name. Numeric parse failures fall back to 0 (graceful degradation).
pub fn parse_session_line(
    line: &str,
    now_epoch: u64,
    commands: &HashMap<String, String>,
) -> Option<Session> {
    let parts: Vec<&str> = line.splitn(8, '|').collect();
    if parts.len() < 4 {
        return None;
    }

    let name = parts[0].to_string();
    if name.is_empty() {
        return None;
    }

    let window_count: u32 = parts[1].parse().unwrap_or(0);
    let attached: bool = parts[2] == "1";
    let last_activity = match parts[3].parse::<u64>() {
        Ok(activity_epoch) if now_epoch >= activity_epoch => {
            Duration::from_secs(now_epoch - activity_epoch)
        }
        Ok(_) => Duration::ZERO,
        Err(_) => Duration::ZERO,
    };

    let current_command = commands
        .get(&name)
        .cloned()
        .unwrap_or_else(|| "?".to_string());

    let metadata = if parts.len() >= 8 {
        let label = nonempty(parts[4]);
        let project = nonempty(parts[5]);
        let purpose = nonempty(parts[6]);
        let label_at = parts[7].parse::<u64>().ok();
        let m = crate::metadata::Metadata { label, project, purpose, label_at };
        if m.is_empty() { None } else { Some(m) }
    } else {
        None
    };

    Some(Session {
        name,
        window_count,
        attached,
        current_command,
        last_activity,
        metadata,
    })
}

fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.to_string()) }
}
```

- [ ] **Step 4: Update `list_sessions` to request 8 fields**

In `src/tmux.rs` `list_sessions()`, change the `-F` argument from:

```rust
"#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}",
```

to:

```rust
"#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}|#{@tmux_picker_label}|#{@tmux_picker_project}|#{@tmux_picker_purpose}|#{@tmux_picker_label_at}",
```

- [ ] **Step 5: Add new tests**

In `src/tmux.rs` `mod tests`, add:

```rust
#[test]
fn test_parse_session_line_with_full_metadata() {
    let commands = HashMap::new();
    let s = parse_session_line(
        "main|2|0|1000|My Label|/home/u/git/app|PR #1|1500",
        1300, &commands,
    ).unwrap();
    assert_eq!(s.name, "main");
    let m = s.metadata.unwrap();
    assert_eq!(m.label.as_deref(), Some("My Label"));
    assert_eq!(m.project.as_deref(), Some("/home/u/git/app"));
    assert_eq!(m.purpose.as_deref(), Some("PR #1"));
    assert_eq!(m.label_at, Some(1500));
}

#[test]
fn test_parse_session_line_empty_metadata_fields() {
    let commands = HashMap::new();
    let s = parse_session_line("main|2|0|1000||||", 1300, &commands).unwrap();
    assert!(s.metadata.is_none());
}

#[test]
fn test_parse_session_line_partial_metadata_label_only() {
    let commands = HashMap::new();
    let s = parse_session_line("main|2|0|1000|Hi|||", 1300, &commands).unwrap();
    let m = s.metadata.unwrap();
    assert_eq!(m.label.as_deref(), Some("Hi"));
    assert!(m.project.is_none());
    assert!(m.purpose.is_none());
    assert!(m.label_at.is_none());
}

#[test]
fn test_parse_session_line_4_field_backward_compat() {
    let commands = HashMap::new();
    let s = parse_session_line("main|2|0|1000", 1300, &commands).unwrap();
    assert!(s.metadata.is_none());
}
```

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: all tests pass, including new metadata-aware ones.

- [ ] **Step 7: Commit**

```bash
git add src/tmux.rs src/session.rs src/app.rs src/input.rs
git commit -m "feat(tmux): extend list-sessions parser to 8 fields with metadata"
```

---

## Task 6: Add metadata read/write/clear + auto-detect

**Files:**
- Modify: `src/metadata.rs`

- [ ] **Step 1: Add I/O functions and tests to `src/metadata.rs`**

Append to the file (above the existing `mod tests`):

```rust
use crate::tmux;

const KEYS: &[&str] = &["label", "project", "purpose", "label_at"];

/// Read a session's metadata via per-key tmux show-options calls.
/// Used by `tmux-picker show <session>`. The picker TUI uses the batch
/// list-sessions parse path instead.
pub fn read(session: &str) -> Result<Metadata, String> {
    if !tmux::session_exists(session) {
        return Err(format!("session '{session}' does not exist"));
    }
    Ok(Metadata {
        label: tmux::get_user_option(session, "tmux_picker_label"),
        project: tmux::get_user_option(session, "tmux_picker_project"),
        purpose: tmux::get_user_option(session, "tmux_picker_purpose"),
        label_at: tmux::get_user_option(session, "tmux_picker_label_at")
            .and_then(|s| s.parse().ok()),
    })
}

/// Write any non-None field of `m` to tmux user-options.
/// None fields are left untouched (does NOT clear).
pub fn write(session: &str, m: &Metadata) -> Result<(), String> {
    if !tmux::session_exists(session) {
        return Err(format!("session '{session}' does not exist"));
    }
    if let Some(ref v) = m.label {
        tmux::set_user_option(session, "tmux_picker_label", v)?;
    }
    if let Some(ref v) = m.project {
        tmux::set_user_option(session, "tmux_picker_project", v)?;
    }
    if let Some(ref v) = m.purpose {
        tmux::set_user_option(session, "tmux_picker_purpose", v)?;
    }
    // Update label_at only if any of label/project/purpose was set.
    if m.label.is_some() || m.project.is_some() || m.purpose.is_some() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        tmux::set_user_option(session, "tmux_picker_label_at", &now.to_string())?;
    }
    Ok(())
}

/// Remove every @tmux_picker_* option from a session.
pub fn clear(session: &str) -> Result<(), String> {
    if !tmux::session_exists(session) {
        return Err(format!("session '{session}' does not exist"));
    }
    for key in KEYS {
        let full = format!("tmux_picker_{key}");
        // Ignore unset-on-already-unset errors; tmux returns non-zero for those.
        let _ = tmux::unset_user_option(session, &full);
    }
    Ok(())
}

/// Auto-detect project + label from session's active pane cwd.
/// Never overwrites a manually-set label or purpose.
pub fn auto_detect(session: &str) -> Result<(), String> {
    if !tmux::session_exists(session) {
        return Err(format!("session '{session}' does not exist"));
    }
    let cwd = tmux::pane_current_path(session)?;
    let project = walk_up_to_git_root(&cwd).unwrap_or(cwd);

    tmux::set_user_option(session, "tmux_picker_project", &project)?;

    let existing_label = tmux::get_user_option(session, "tmux_picker_label");
    if existing_label.is_none()
        && let Some(base) = std::path::Path::new(&project).file_name()
            && let Some(s) = base.to_str()
    {
        tmux::set_user_option(session, "tmux_picker_label", s)?;
    }

    if std::path::Path::new(&project).join(".git").exists()
        && tmux::get_user_option(session, "tmux_picker_purpose").is_none()
        && let Some(branch) = git_current_branch(&project)
    {
        tmux::set_user_option(
            session,
            "tmux_picker_purpose",
            &format!("branch:{branch}"),
        )?;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tmux::set_user_option(session, "tmux_picker_label_at", &now.to_string())?;

    Ok(())
}

/// Walk from `start` up the directory tree until a `.git` directory exists.
/// Returns the path containing `.git`, or None if not found before root.
pub fn walk_up_to_git_root(start: &str) -> Option<String> {
    let mut cur = std::path::PathBuf::from(start);
    loop {
        if cur.join(".git").exists() {
            return cur.to_str().map(String::from);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn git_current_branch(repo: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", repo, "branch", "--show-current"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if branch.is_empty() { None } else { Some(branch) }
}
```

- [ ] **Step 2: Add unit tests for `walk_up_to_git_root` (the only pure function)**

Inside the existing `mod tests` block, append:

```rust
#[test]
fn walk_up_finds_repo_root() {
    // This repo has .git; the walk from src/ must find this directory.
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let started_in = format!("{crate_root}/src");
    let found = walk_up_to_git_root(&started_in).unwrap();
    assert_eq!(found, crate_root);
}

#[test]
fn walk_up_returns_none_below_root_without_git() {
    // /tmp typically has no .git anywhere up; walk should hit root and stop.
    let result = walk_up_to_git_root("/tmp");
    // We can't assert None unconditionally (some systems may have /.git),
    // but we can assert the function terminates without panicking.
    let _ = result;
}
```

- [ ] **Step 3: Run lib tests**

Run: `cargo test --lib metadata`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/metadata.rs
git commit -m "feat(metadata): add read/write/clear and git-aware auto_detect"
```

---

## Task 7: Add CLI subcommand definitions

**Files:**
- Create: `src/cli.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "tmux-picker",
    version,
    about = "TUI session picker for tmux on SSH login",
    long_about = "Run with no arguments for the picker TUI. \
                  Subcommands manage per-session metadata (label/project/purpose) \
                  stored as tmux user-options."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Set or clear metadata for a session.
    Label {
        /// Session name.
        session: String,
        /// Human label, e.g., "Refactoring auth middleware".
        #[arg(long)]
        label: Option<String>,
        /// Project root path.
        #[arg(long)]
        project: Option<String>,
        /// Short purpose, e.g., "PR #234".
        #[arg(long)]
        purpose: Option<String>,
        /// Remove all tmux-picker metadata for this session.
        #[arg(long, conflicts_with_all = ["label", "project", "purpose"])]
        clear: bool,
    },
    /// Print metadata for a session as TOML.
    Show {
        /// Session name.
        session: String,
    },
    /// Auto-detect metadata from the session's active pane working directory.
    Auto {
        /// Session name.
        session: String,
    },
}
```

- [ ] **Step 2: Register in `src/lib.rs`**

Add `pub mod cli;` to the list.

- [ ] **Step 3: Verify clap accepts the schema**

Run: `cargo build --release`
Expected: success. (clap derive macros expand at compile time; failure here means a schema error.)

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/lib.rs
git commit -m "feat(cli): add clap-derived subcommand definitions"
```

---

## Task 8: Wire subcommands into `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace the entire `src/main.rs` with subcommand routing**

```rust
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
        Some(Command::Label { session, label, project, purpose, clear }) => {
            run_label(&session, label, project, purpose, clear)
        }
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
    let sessions = match tmux::list_sessions() {
        Ok(s) if s.is_empty() => return Ok(Action::New("main".into())),
        Ok(s) => s,
        Err(_) => return Ok(Action::New("main".into())),
    };

    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    crossterm::execute!(stderr(), EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sessions);
    let mut last_tick = Instant::now();

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

    let m = metadata::Metadata { label, project, purpose, label_at: None };
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
```

- [ ] **Step 2: Build + sanity-check `--help`**

Run: `cargo build --release && ./target/release/tmux-picker --help`
Expected: clap-formatted help text listing `label`, `show`, `auto` subcommands.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): wire label/show/auto subcommands; existing TUI on no-arg"
```

---

## Task 9: UI — label-aware name display

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Replace the name-column rendering inside `draw_sessions`**

In `src/ui.rs`, find the `Span::styled(session.name.clone(), name_style)` line inside the row builder (around line 94). Replace it with:

```rust
Span::styled(format_name_display(session), name_style),
```

Add a free function near the top of the file (after the `use` block):

```rust
fn format_name_display(session: &crate::session::Session) -> String {
    match session.metadata.as_ref().and_then(|m| m.label.as_deref()) {
        Some(label) => format!("{label} ({})", session.name),
        None => session.name.clone(),
    }
}
```

Bump the name-column width from `Constraint::Length(18)` to `Constraint::Length(36)` in the `widths` array (around line 108).

- [ ] **Step 2: Add a UI unit test for `format_name_display`**

At the bottom of `src/ui.rs`, add:

```rust
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
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat(ui): show label as 'label (session-name)' when metadata present"
```

---

## Task 10: UI — detail row under selected session

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add a helper that builds the detail line**

In `src/ui.rs`, add another free function near `format_name_display`:

```rust
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
```

- [ ] **Step 2: Update `draw` to allocate a detail strip when needed**

Replace the entire `pub fn draw` function with:

```rust
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let detail_height = app
        .sessions
        .get(app.selected)
        .and_then(format_detail_line)
        .map(|_| 1u16)
        .unwrap_or(0);

    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(detail_height),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(area);

    draw_sessions(frame, app, chunks[0]);
    if detail_height > 0 {
        draw_detail(frame, app, chunks[1]);
    }
    draw_actions(frame, app, chunks[2]);
    draw_help(frame, app, chunks[3]);
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
```

- [ ] **Step 3: Add unit tests**

Inside the existing `mod tests` block in `src/ui.rs`, append:

```rust
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
    // Force HOME so collapse_home is deterministic.
    // SAFETY: tests run with --test-threads=1 not guaranteed; this is a
    // best-effort assertion that the prefix logic works at all.
    unsafe { std::env::set_var("HOME", "/home/u"); }
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "feat(ui): render project/purpose detail row under selected session"
```

---

## Task 11: Integration tests with real tmux

**Files:**
- Modify: `tests/integration.rs`

- [ ] **Step 1: Add helper for invoking the binary**

Inside the existing helpers block (after `create_session`), add:

```rust
fn binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    // exe is target/<profile>/deps/integration-<hash> — pop twice to reach target/<profile>/
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("tmux-picker");
    p
}

fn run_binary_with_socket(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .env("TMUX_TMPDIR", "/tmp")
        .args(args)
        .output()
        .expect("failed to run tmux-picker binary")
}
```

The binary uses `/usr/bin/tmux` directly (via `tmux_bin()`), which connects to the **default** socket — not the test socket. To make subcommand integration tests work against the isolated socket, we either need the binary to take a `--socket` arg (out of scope for this sub-project) or invoke tmux directly to verify the underlying state. Use the latter approach: invoke the binary with no socket override on the **default** tmux socket, but only in tests we explicitly control.

Update the integration tests file header note (lines 1-13) to add:

```
// Subcommand tests (label/show/auto) hit the default tmux socket because
// the binary does not yet accept a socket flag. They create unique session
// names (prefixed `tmuxpicker-it-`) and clean up after themselves.
```

- [ ] **Step 2: Add label/show round-trip test**

Append to `tests/integration.rs`:

```rust
const IT_PREFIX: &str = "tmuxpicker-it-";

fn cleanup_it_sessions() {
    // Best-effort: kill any leftover IT sessions on the default socket.
    let out = Command::new("/usr/bin/tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();
    if let Ok(out) = out {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with(IT_PREFIX) {
                let _ = Command::new("/usr/bin/tmux")
                    .args(["kill-session", "-t", line])
                    .output();
            }
        }
    }
}

fn create_default_socket_session(name: &str) {
    let _ = Command::new("/usr/bin/tmux")
        .args(["new-session", "-d", "-s", name])
        .output();
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn test_label_then_show_round_trip() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}label-show");
    create_default_socket_session(&sess);

    let out = run_binary_with_socket(&[
        "label", &sess,
        "--label", "Refactoring auth",
        "--purpose", "PR #234",
    ]);
    assert!(out.status.success(), "label failed: {}", String::from_utf8_lossy(&out.stderr));

    let show = run_binary_with_socket(&["show", &sess]);
    assert!(show.status.success(), "show failed: {}", String::from_utf8_lossy(&show.stderr));
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(toml.contains(&format!("session = \"{sess}\"")));
    assert!(toml.contains("label = \"Refactoring auth\""));
    assert!(toml.contains("purpose = \"PR #234\""));
    assert!(toml.contains("label_at = "));

    cleanup_it_sessions();
}

#[test]
fn test_label_clear_removes_metadata() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}clear");
    create_default_socket_session(&sess);

    run_binary_with_socket(&["label", &sess, "--label", "x"]);
    run_binary_with_socket(&["label", &sess, "--clear"]);

    let show = run_binary_with_socket(&["show", &sess]);
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(toml.contains(&format!("session = \"{sess}\"")));
    assert!(!toml.contains("label ="));
    assert!(!toml.contains("project ="));
    assert!(!toml.contains("purpose ="));

    cleanup_it_sessions();
}

#[test]
fn test_label_rejects_pipe_in_value() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}reject-pipe");
    create_default_socket_session(&sess);

    let out = run_binary_with_socket(&["label", &sess, "--label", "a|b"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("must not contain '|'"));

    cleanup_it_sessions();
}

#[test]
fn test_label_rejects_unknown_session() {
    let out = run_binary_with_socket(&[
        "label", "nonexistent-tmuxpicker-test-session",
        "--label", "x",
    ]);
    assert!(!out.status.success());
}

#[test]
fn test_auto_uses_pane_cwd() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}auto");
    // Create session with cwd set to this crate root (which has .git).
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let _ = Command::new("/usr/bin/tmux")
        .args(["new-session", "-d", "-s", &sess, "-c", crate_root])
        .output();
    thread::sleep(Duration::from_millis(50));

    let out = run_binary_with_socket(&["auto", &sess]);
    assert!(out.status.success(), "auto failed: {}", String::from_utf8_lossy(&out.stderr));

    let show = run_binary_with_socket(&["show", &sess]);
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(toml.contains(&format!("project = \"{crate_root}\"")));
    assert!(toml.contains("label = \"tmux-picker\""));

    cleanup_it_sessions();
}
```

- [ ] **Step 3: Build the binary first, then run integration tests**

Run: `cargo build && cargo test --test integration -- --test-threads=1`
Expected: all integration tests pass. If a test fails because tmux is not running, ensure tmux is installed and start a server first with `/usr/bin/tmux start-server`.

- [ ] **Step 4: Commit**

```bash
git add tests/integration.rs
git commit -m "test(integration): label/show/auto/clear round-trip with real tmux"
```

---

## Task 12: E2E shell-script smoke tests

**Files:**
- Modify: `tests/e2e.sh`

- [ ] **Step 1: Append subcommand smoke tests**

Add these test blocks at the end of `tests/e2e.sh` (before the `Results` echo):

```bash
# --- Test 7: --help exits 0 and lists subcommands ---
echo "Test 7: --help mentions subcommands"
help_out=$("$BINARY" --help 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q "label" <<<"$help_out" \
    && grep -q "show" <<<"$help_out" \
    && grep -q "auto" <<<"$help_out"; then
    pass "--help lists label/show/auto"
else
    fail "--help" "exit=$rc; missing one of label/show/auto in output"
fi

# --- Test 8: --version exits 0 ---
echo "Test 8: --version exits 0"
"$BINARY" --version >/dev/null 2>&1 && pass "--version exits 0" \
    || fail "--version" "non-zero exit"

# --- Test 9: label/show round-trip on isolated socket ---
echo "Test 9: label/show round-trip"
SESS="e2e-label-$$"
$TMUX new-session -d -s "$SESS"
sleep 0.1
# The binary uses /usr/bin/tmux on default socket; create on default socket too
$TMUX kill-session -t "$SESS" 2>/dev/null
/usr/bin/tmux new-session -d -s "$SESS" 2>/dev/null
sleep 0.1
"$BINARY" label "$SESS" --label "e2e test" >/dev/null 2>&1
out=$("$BINARY" show "$SESS" 2>/dev/null)
if grep -q 'label = "e2e test"' <<<"$out"; then
    pass "label/show round-trip"
else
    fail "label/show round-trip" "got: $out"
fi
/usr/bin/tmux kill-session -t "$SESS" 2>/dev/null
```

- [ ] **Step 2: Run**

Run: `cargo build --release && bash tests/e2e.sh`
Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e.sh
git commit -m "test(e2e): smoke tests for --help, --version, label/show round-trip"
```

---

## Task 13: README updates

**Files:**
- Create or Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Check for existing README**

Run: `ls README.md 2>/dev/null && echo present || echo missing`

- [ ] **Step 2: Write or extend README.md**

If README.md does not exist, create it with this content. If it exists, replace its contents:

```markdown
# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints
the chosen action to stdout. A bash stub (`shell/tmux-autoattach.sh`) reads
stdout and execs tmux (or drops to a shell if the user picked `s`).

## Features

- Color-coded session list with attached indicator, command, and idle time.
- 10-second auto-attach to the most recent detached session.
- Numbered + arrow + j/k navigation.
- New-session input with name validation.
- Per-session metadata (label / project / purpose) stored as tmux user-options.
- Label-aware display: shows `label (session-name)` when a label is set, plus
  a project-path / purpose detail line under the selected session.

## Build / Install

```bash
cargo build --release
cp target/release/tmux-picker ~/.local/bin/
```

Or use `scripts/install.sh` (see "Install scripts" below) for a complete setup
that includes the shell hook.

## CLI

```
tmux-picker                       Run the picker TUI (default)
tmux-picker label <session> ...   Set or clear session metadata
tmux-picker show  <session>       Print session metadata as TOML
tmux-picker auto  <session>       Auto-detect from session's pane cwd
tmux-picker --help / --version
```

### `label`

```
tmux-picker label <session> [flags]
  --label   <text>    Human label, e.g. "Refactoring auth middleware"
  --project <path>    Project root, e.g. "/home/u/git/app"
  --purpose <text>    Short purpose, e.g. "PR #234"
  --clear             Remove all tmux-picker metadata for this session
```

`--clear` is mutually exclusive with the other flags. Pipe characters (`|`) are
rejected in any value because the underlying `list-sessions` parser is
pipe-delimited.

### `show`

Prints metadata as TOML. Empty metadata prints just the `session = "..."`
line. Errors if the session does not exist.

```toml
session = "claude-app"
label = "Refactoring auth"
project = "/home/u/git/app"
purpose = "PR #234"
label_at = 1746201600
```

### `auto`

Reads the session's active-pane working directory, walks up to the nearest
`.git`, and sets:

- `project` → git root (or pane cwd if no `.git`)
- `label` → `basename(project)` *only if no label is set*
- `purpose` → `branch:<current-branch>` *only if no purpose is set and the
  project is a git repo*

Manually-set values are never overwritten.

## Claude Code integration

Inside a tmux session running Claude Code, the assistant can update its own
session label as it works:

```bash
tmux-picker label "$(tmux display -p '#S')" \
    --label "Investigating runner queue depth"
```

Or auto-derive from cwd:

```bash
tmux-picker auto "$(tmux display -p '#S')"
```

## Storage model

Metadata lives in tmux user-options on each session:

| Key | Meaning |
|---|---|
| `@tmux_picker_label` | Human label string |
| `@tmux_picker_project` | Absolute project path |
| `@tmux_picker_purpose` | Short purpose string |
| `@tmux_picker_label_at` | Epoch seconds when last updated |

`tmux kill-session` removes the metadata along with the session. There is no
external state file.

## Test

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
bash tests/e2e.sh
```
```

- [ ] **Step 3: Update `CLAUDE.md`** (the project one) to point at the new CLI

Replace the contents of `CLAUDE.md`:

```markdown
# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints
action to stdout. Plus CLI subcommands for per-session metadata.

## Build
cargo build --release

## Test
cargo test
bash tests/e2e.sh

## Lint
cargo fmt --check && cargo clippy -- -D warnings

## Install
cp target/release/tmux-picker ~/.local/bin/tmux-picker
# or: bash scripts/install.sh

## CLI
tmux-picker                     # TUI picker
tmux-picker label <s> [flags]   # set/clear metadata
tmux-picker show  <s>           # dump metadata as TOML
tmux-picker auto  <s>           # auto-detect from pane cwd
```

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: README + CLAUDE.md cover label/show/auto subcommands"
```

---

## Task 14: Final lint + test sweep

- [ ] **Step 1: Format check**

Run: `cargo fmt --check`
Expected: clean.

- [ ] **Step 2: Clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: All tests**

Run: `cargo test -- --test-threads=1`
Expected: all pass.

- [ ] **Step 4: E2E**

Run: `cargo build --release && bash tests/e2e.sh`
Expected: all pass.

- [ ] **Step 5: Manual smoke (since user is hands-off, agent runs these)**

```bash
# Seed an isolated test session
/usr/bin/tmux new-session -d -s smoke-test -c "$(pwd)"
./target/release/tmux-picker label smoke-test --label "Smoke verification" --purpose "QA"
./target/release/tmux-picker show smoke-test
./target/release/tmux-picker auto smoke-test
./target/release/tmux-picker show smoke-test
/usr/bin/tmux kill-session -t smoke-test
```

Capture the output of `show` after each step in the commit message of the
final verification commit.

---

## Self-Review Checklist

(Plan author runs this after writing.)

- [x] Spec coverage: every section of the design has a task — metadata struct
      (Task 3), tmux helpers (Task 4), parser extension (Task 5), I/O +
      auto-detect (Task 6), CLI defs (Task 7), routing (Task 8), label-aware
      name (Task 9), detail row (Task 10), tests (Tasks 11-12), docs (13).
- [x] Placeholder scan: no TBD / "implement later" / "fill in details".
      Every code step shows full code.
- [x] Type consistency: `Metadata` struct shape consistent across Tasks 3, 5,
      6, 9, 10, 11. CLI variant names (`Label/Show/Auto`) consistent across
      Tasks 7 and 8.
- [x] All file paths absolute or repo-relative as `src/...` / `tests/...`.
- [x] Each task ends with a commit step.
