// Integration tests for tmux-picker.
//
// These tests spawn a real tmux server on an isolated socket to avoid
// touching the user's live sessions.  They MUST run sequentially because
// they share the same tmux socket.  Prefer:
//
//   cargo test --test integration -- --test-threads=1
//
// The SERIAL_LOCK mutex below also serialises execution when Cargo runs the
// integration binary with its default (multi-threaded) test runner so that a
// plain `cargo test` also passes.
//
// Each test calls setup() to kill any leftover server and teardown() to
// clean up afterwards.
//
// Subcommand tests (label/show/auto) hit the default tmux socket because
// the binary does not yet accept a socket flag. They create unique session
// names (prefixed `tmuxpicker-it-`) and clean up after themselves.

use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

// Global mutex that every test acquires before touching the tmux socket.
static SERIAL_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the serial lock.  Poison is recovered so a panicking test does not
/// block all subsequent tests.
fn serial_lock() -> MutexGuard<'static, ()> {
    match SERIAL_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

const TMUX: &str = "/usr/bin/tmux";
const SOCKET: &str = "tmux-picker-test";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tmux_cmd(args: &[&str]) -> std::process::Output {
    Command::new(TMUX)
        .args(["-L", SOCKET])
        .args(args)
        .output()
        .expect("failed to run tmux")
}

fn setup() {
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(250));
}

fn teardown() {
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(100));
}

fn create_session(name: &str) {
    tmux_cmd(&["new-session", "-d", "-s", name]);
    thread::sleep(Duration::from_millis(100));
}

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

fn run_binary(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("failed to run tmux-picker binary")
}

const IT_PREFIX: &str = "tmuxpicker-it-";

fn cleanup_it_sessions() {
    let out = Command::new(TMUX)
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();
    if let Ok(out) = out {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with(IT_PREFIX) {
                let _ = Command::new(TMUX)
                    .args(["kill-session", "-t", line])
                    .output();
            }
        }
    }
}

fn create_default_socket_session(name: &str) {
    let _ = Command::new(TMUX)
        .args(["new-session", "-d", "-s", name])
        .output();
    thread::sleep(Duration::from_millis(50));
}

fn create_default_socket_session_in(name: &str, cwd: &str) {
    let _ = Command::new(TMUX)
        .args(["new-session", "-d", "-s", name, "-c", cwd])
        .output();
    thread::sleep(Duration::from_millis(50));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that `list-sessions` output with 4 pipe-separated fields is parseable
/// and that the window count field is numeric.
#[test]
fn test_parse_real_tmux_sessions() {
    let _lock = serial_lock();
    setup();

    create_session("test-main");
    create_session("test-work");
    create_session("claude-test");

    let output = tmux_cmd(&[
        "list-sessions",
        "-F",
        "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}",
    ]);

    assert!(
        output.status.success(),
        "list-sessions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        3,
        "expected 3 session lines, got {}; output:\n{stdout}",
        lines.len(),
    );

    for line in &lines {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        assert_eq!(
            parts.len(),
            4,
            "expected 4 pipe-separated fields in line {line:?}",
        );

        // Field [1] is the window count — must parse as a non-negative integer.
        let window_count: u32 = parts[1]
            .parse()
            .unwrap_or_else(|_| panic!("window count is not numeric in line {line:?}"));
        // A freshly created session has at least 1 window.
        assert!(
            window_count >= 1,
            "expected window_count >= 1, got {window_count} in line {line:?}",
        );
    }

    teardown();
}

/// Verify that `list-panes` output has 4 pipe-separated fields per line and
/// that the session name is non-empty.
#[test]
fn test_parse_real_pane_commands() {
    let _lock = serial_lock();
    setup();

    create_session("pane-test-session");

    let output = tmux_cmd(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ]);

    assert!(
        output.status.success(),
        "list-panes failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert!(
        !lines.is_empty(),
        "expected at least one pane line; output:\n{stdout}",
    );

    for line in &lines {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        assert_eq!(
            parts.len(),
            4,
            "expected 4 pipe-separated fields in pane line {line:?}",
        );
        assert!(
            !parts[0].is_empty(),
            "session_name field must not be empty in line {line:?}",
        );
    }

    teardown();
}

/// Verify that 20 sessions can be created and all 20 appear in `list-sessions`.
#[test]
fn test_many_sessions() {
    let _lock = serial_lock();
    setup();

    for i in 0..20 {
        create_session(&format!("sess-{i:02}"));
    }

    let output = tmux_cmd(&["list-sessions", "-F", "#{session_name}"]);

    assert!(
        output.status.success(),
        "list-sessions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines.len(),
        20,
        "expected 20 session lines, got {}; output:\n{}",
        lines.len(),
        stdout
    );

    teardown();
}

/// Verify that sessions with hyphens, underscores, and uppercase letters are
/// preserved exactly as created.
#[test]
fn test_session_with_special_names() {
    let _lock = serial_lock();
    setup();

    let names = [
        "with-hyphens",
        "with_underscores",
        "ALLCAPS",
        "MixedCase-01",
    ];

    for name in &names {
        create_session(name);
    }

    let output = tmux_cmd(&["list-sessions", "-F", "#{session_name}"]);

    assert!(
        output.status.success(),
        "list-sessions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    for name in &names {
        assert!(
            stdout.lines().any(|l| l == *name),
            "expected session name {name:?} to appear in list-sessions output:\n{stdout}",
        );
    }

    teardown();
}

/// Verify that `list-sessions` returns a non-zero exit code when no tmux
/// server is running on the isolated socket.
#[test]
fn test_no_server_running() {
    let _lock = serial_lock();
    // Ensure the test server is definitely not running.
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(200));

    let output = tmux_cmd(&["list-sessions"]);

    assert!(
        !output.status.success(),
        "expected list-sessions to fail when no server is running, but it succeeded"
    );
}

// ---------------------------------------------------------------------------
// Subcommand integration tests (default tmux socket)
// ---------------------------------------------------------------------------

#[test]
fn test_label_then_show_round_trip() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}label-show");
    create_default_socket_session(&sess);

    let out = run_binary(&[
        "label",
        &sess,
        "--label",
        "Refactoring auth",
        "--purpose",
        "PR #234",
    ]);
    assert!(
        out.status.success(),
        "label failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let show = run_binary(&["show", &sess]);
    assert!(
        show.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(
        toml.contains(&format!("session = \"{sess}\"")),
        "got: {toml}"
    );
    assert!(toml.contains("label = \"Refactoring auth\""), "got: {toml}");
    assert!(toml.contains("purpose = \"PR #234\""), "got: {toml}");
    assert!(toml.contains("label_at = "), "got: {toml}");

    cleanup_it_sessions();
}

#[test]
fn test_label_clear_removes_metadata() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}clear");
    create_default_socket_session(&sess);

    let _ = run_binary(&["label", &sess, "--label", "x"]);
    let _ = run_binary(&["label", &sess, "--clear"]);

    let show = run_binary(&["show", &sess]);
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(
        toml.contains(&format!("session = \"{sess}\"")),
        "got: {toml}"
    );
    assert!(!toml.contains("label ="), "got: {toml}");
    assert!(!toml.contains("project ="), "got: {toml}");
    assert!(!toml.contains("purpose ="), "got: {toml}");

    cleanup_it_sessions();
}

#[test]
fn test_label_rejects_pipe_in_value() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}reject-pipe");
    create_default_socket_session(&sess);

    let out = run_binary(&["label", &sess, "--label", "a|b"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("must not contain '|'"), "got: {stderr}");

    cleanup_it_sessions();
}

#[test]
fn test_label_rejects_unknown_session() {
    let out = run_binary(&[
        "label",
        "nonexistent-tmuxpicker-test-session-xyz",
        "--label",
        "x",
    ]);
    assert!(!out.status.success());
}

#[test]
fn test_rename_session_round_trip() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let old = format!("{IT_PREFIX}rename-old");
    let new = format!("{IT_PREFIX}rename-new");
    create_default_socket_session(&old);

    tmux_picker::tmux::rename_session(&old, &new).expect("rename should succeed");

    let has_old = Command::new(TMUX)
        .args(["has-session", "-t", &old])
        .status()
        .expect("has-session");
    let has_new = Command::new(TMUX)
        .args(["has-session", "-t", &new])
        .status()
        .expect("has-session");
    assert!(!has_old.success(), "old name should be gone after rename");
    assert!(has_new.success(), "new name should exist after rename");

    let _ = Command::new(TMUX)
        .args(["kill-session", "-t", &new])
        .status();
    cleanup_it_sessions();
}

#[test]
fn test_rename_to_existing_name_fails() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let a = format!("{IT_PREFIX}rename-a");
    let b = format!("{IT_PREFIX}rename-b");
    create_default_socket_session(&a);
    create_default_socket_session(&b);

    let result = tmux_picker::tmux::rename_session(&a, &b);
    assert!(
        result.is_err(),
        "tmux should refuse to rename onto an existing session"
    );

    cleanup_it_sessions();
}

#[test]
fn test_kill_session_removes_it() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}kill-target");
    create_default_socket_session(&sess);

    // Verify it exists.
    let before = Command::new(TMUX)
        .args(["has-session", "-t", &sess])
        .status()
        .expect("has-session");
    assert!(before.success(), "session should exist before kill");

    // Kill via the binary's tmux::kill_session path: easiest is calling tmux
    // directly because we don't expose kill via subcommand.
    let killed = Command::new(TMUX)
        .args(["kill-session", "-t", &sess])
        .status()
        .expect("kill-session");
    assert!(killed.success());

    let after = Command::new(TMUX)
        .args(["has-session", "-t", &sess])
        .status()
        .expect("has-session");
    assert!(!after.success(), "session should be gone after kill");

    cleanup_it_sessions();
}

#[test]
fn test_pane_capture_returns_buffer_text() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}capture");
    create_default_socket_session(&sess);

    // Send a unique line into the pane and wait for it to render.
    let _ = Command::new(TMUX)
        .args([
            "send-keys",
            "-t",
            &sess,
            "echo CAPTURE-MARKER-12345",
            "Enter",
        ])
        .output();
    thread::sleep(Duration::from_millis(200));

    let out = run_binary(&["show", &sess]);
    // We invoke `show` to ensure the binary still works; capture itself uses
    // the underlying tmux::pane_capture function indirectly via picker — here
    // we test the tmux command directly to verify the format matches.
    assert!(out.status.success());

    let cap = Command::new(TMUX)
        .args(["capture-pane", "-t", &sess, "-p", "-J", "-S", "-6"])
        .output()
        .expect("capture-pane");
    let text = String::from_utf8_lossy(&cap.stdout);
    assert!(
        text.contains("CAPTURE-MARKER-12345"),
        "capture missing marker; got: {text}"
    );

    cleanup_it_sessions();
}

#[test]
fn test_auto_uses_pane_cwd() {
    let _lock = serial_lock();
    cleanup_it_sessions();

    let sess = format!("{IT_PREFIX}auto");
    let crate_root = env!("CARGO_MANIFEST_DIR");
    create_default_socket_session_in(&sess, crate_root);

    let out = run_binary(&["auto", &sess]);
    assert!(
        out.status.success(),
        "auto failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let show = run_binary(&["show", &sess]);
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(
        toml.contains(&format!("project = \"{crate_root}\"")),
        "got: {toml}"
    );
    assert!(toml.contains("label = \"tmux-picker\""), "got: {toml}");

    cleanup_it_sessions();
}
