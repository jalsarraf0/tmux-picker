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

use std::path::PathBuf;
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

/// Return the path of the tmux socket file for our isolated socket.
/// tmux stores sockets at `/tmp/tmux-<uid>/<socket-name>`.
fn socket_path() -> PathBuf {
    let uid = libc_getuid();
    PathBuf::from(format!("/tmp/tmux-{}/{}", uid, SOCKET))
}

/// Minimal wrapper around libc getuid so we don't need a libc dep.
fn libc_getuid() -> u32 {
    // SAFETY: getuid() is always safe to call.
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        // SAFETY: getuid() has no preconditions; it is always safe.
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

fn setup() {
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(200));
    // Remove any stale socket file that tmux leaves behind after kill-server.
    let _ = std::fs::remove_file(socket_path());
    thread::sleep(Duration::from_millis(50));
}

fn teardown() {
    let _ = tmux_cmd(&["kill-server"]);
    thread::sleep(Duration::from_millis(100));
    let _ = std::fs::remove_file(socket_path());
}

fn create_session(name: &str) {
    tmux_cmd(&["new-session", "-d", "-s", name]);
    thread::sleep(Duration::from_millis(100));
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
        "expected 3 session lines, got {}; output:\n{}",
        lines.len(),
        stdout
    );

    for line in &lines {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        assert_eq!(
            parts.len(),
            4,
            "expected 4 pipe-separated fields in line {:?}",
            line
        );

        // Field [1] is the window count — must parse as a non-negative integer.
        let window_count: u32 = parts[1]
            .parse()
            .unwrap_or_else(|_| panic!("window count is not numeric in line {:?}", line));
        // A freshly created session has at least 1 window.
        assert!(
            window_count >= 1,
            "expected window_count >= 1, got {} in line {:?}",
            window_count,
            line
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
        "expected at least one pane line; output:\n{}",
        stdout
    );

    for line in &lines {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        assert_eq!(
            parts.len(),
            4,
            "expected 4 pipe-separated fields in pane line {:?}",
            line
        );
        assert!(
            !parts[0].is_empty(),
            "session_name field must not be empty in line {:?}",
            line
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
        create_session(&format!("sess-{:02}", i));
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
            "expected session name {:?} to appear in list-sessions output:\n{}",
            name,
            stdout
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
