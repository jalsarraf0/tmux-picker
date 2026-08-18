use std::collections::HashMap;
use std::io::Read;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use crate::session::Session;

const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolve the tmux binary once: prefer /usr/bin/tmux, fall back to PATH lookup.
fn tmux_bin() -> &'static str {
    static BIN: OnceLock<String> = OnceLock::new();
    BIN.get_or_init(|| {
        if std::path::Path::new("/usr/bin/tmux").exists() {
            return "/usr/bin/tmux".to_string();
        }
        // Fall back to PATH lookup via `which`
        if let Ok(output) = Command::new("which").arg("tmux").output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
        // Last resort — hope it's in PATH
        "tmux".to_string()
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn run_tmux(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(tmux_bin())
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;

    // Poll with timeout — tmux commands typically finish in < 100ms
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > TMUX_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("tmux command timed out".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(format!("failed to wait on tmux: {e}")),
        }
    };

    // Drain pipes directly — avoids relying on wait_with_output after try_wait
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }

    if status.success() {
        Ok(stdout)
    } else {
        let mut stderr_str = String::new();
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut stderr_str);
        }
        Err(stderr_str)
    }
}

/// Parse `list-panes -a` output into a map of session_name → current command
/// of that session's active window's active pane.
///
/// Format per line: `session_name|window_active|pane_active|pane_current_command`
pub fn parse_pane_commands(output: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let mut fields = line.splitn(4, '|');
        let (Some(session_name), Some(window_active), Some(pane_active), Some(command)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };

        if window_active == "1" && pane_active == "1" {
            map.insert(session_name.to_string(), command.to_string());
        }
    }
    map
}

/// Parse `list-panes -a` output into a map of session_name → every pane's
/// current command (active and inactive). Used for marker matching so a
/// session keeps its 🤖 even when the active pane is the bash next to it.
pub fn parse_all_pane_commands(output: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines() {
        let mut fields = line.splitn(4, '|');
        let (Some(session_name), Some(_window_active), Some(_pane_active), Some(command)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        map.entry(session_name.to_owned())
            .or_default()
            .push(command.to_owned());
    }
    map
}

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
    let mut fields = line.splitn(8, '|');
    let (Some(name), Some(window_count), Some(attached), Some(activity)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    let name = name.to_owned();
    if name.is_empty() {
        return None;
    }

    let window_count: u32 = window_count.parse().unwrap_or(0);
    let attached: bool = attached == "1";
    let last_activity = match activity.parse::<u64>() {
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

    let metadata = if let (Some(label), Some(project), Some(purpose), Some(label_at)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    {
        let label = nonempty(label);
        let project = nonempty(project);
        let purpose = nonempty(purpose);
        let label_at = label_at.parse::<u64>().ok();
        let m = crate::metadata::Metadata {
            label,
            project,
            purpose,
            label_at,
        };
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
        marker: None,
    })
}

fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Query the tmux server for all sessions and return them sorted.
/// Returns Err if the tmux server is not running or unreachable.
pub fn list_sessions() -> Result<Vec<Session>, String> {
    list_sessions_impl(None)
}

/// Query all sessions and apply marker detection using the same pane query.
/// This avoids a second `list-panes` subprocess during picker refreshes.
pub fn list_sessions_with_markers(
    markers: &crate::config::Markers,
) -> Result<Vec<Session>, String> {
    list_sessions_impl(Some(markers))
}

fn list_sessions_impl(markers: Option<&crate::config::Markers>) -> Result<Vec<Session>, String> {
    // Collect pane commands first (best-effort; ignore errors).
    let pane_output = run_tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ])
    .unwrap_or_default();
    let commands = parse_pane_commands(&pane_output);
    let all_commands = markers.map(|_| parse_all_pane_commands(&pane_output));

    // Query sessions (8-field format includes user-options for metadata).
    let session_output = run_tmux(&[
        "list-sessions",
        "-F",
        "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}|#{@tmux_picker_label}|#{@tmux_picker_project}|#{@tmux_picker_purpose}|#{@tmux_picker_label_at}",
    ])?;

    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut sessions: Vec<Session> = session_output
        .lines()
        .filter_map(|line| parse_session_line(line, now_epoch, &commands))
        .collect();

    if let (Some(markers), Some(all_commands)) = (markers, all_commands.as_ref()) {
        for session in &mut sessions {
            if let Some(commands) = all_commands.get(&session.name) {
                session.marker = markers.lookup(commands);
            }
        }
    }

    sessions.sort();
    Ok(sessions)
}

/// Apply the configured marker map to every session by scanning all of its
/// panes. Run after `list_sessions` from the picker loop so the integration
/// tests (which only need plain session metadata) stay marker-agnostic.
pub fn populate_markers(sessions: &mut [Session], markers: &crate::config::Markers) {
    let pane_output = run_tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ])
    .unwrap_or_default();
    let all = parse_all_pane_commands(&pane_output);
    for s in sessions {
        if let Some(cmds) = all.get(&s.name) {
            s.marker = markers.lookup(cmds);
        }
    }
}

/// Per-window snapshot for the multi-window preview mode.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowSnapshot {
    /// Window name.
    pub name: String,
    /// Last non-blank line captured from its active pane.
    pub last_line: String,
}

/// List a session's windows along with the last non-blank line of each
/// window's active pane. Caps at `max` entries; returns an extra entry
/// `("…", "(N more)")` if windows exceed the cap.
pub fn list_windows(session: &str, max: usize) -> Result<Vec<WindowSnapshot>, String> {
    let raw = run_tmux(&[
        "list-windows",
        "-t",
        session,
        "-F",
        "#{window_name}|#{window_active}|#{pane_id}",
    ])?;
    // Format: name|active|paneid (paneid of active pane in that window — tmux
    // does not have a per-window "active pane id" format token, but every
    // window has one active pane and `list-panes -t SESS:WIN -F …` would
    // require another loop. Instead we use `display-message`-style format on
    // the window itself which exposes #{pane_id} of the active pane.)
    let entries: Vec<(String, String)> = raw
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '|');
            let (Some(name), Some(_active), Some(pane_id)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return None;
            };
            Some((name.to_owned(), pane_id.to_owned()))
        })
        .collect();

    let total = entries.len();
    let mut out: Vec<WindowSnapshot> = Vec::new();
    for (name, pane_id) in entries.into_iter().take(max) {
        let captured =
            run_tmux(&["capture-pane", "-p", "-t", &pane_id, "-S", "-3", "-J"]).unwrap_or_default();
        let last_line = captured
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("(empty)")
            .trim()
            .to_string();
        out.push(WindowSnapshot { name, last_line });
    }
    if total > max {
        out.push(WindowSnapshot {
            name: "…".into(),
            last_line: format!("({} more)", total - max),
        });
    }
    Ok(out)
}

/// Returns true if a tmux session with the given name exists.
pub fn session_exists(name: &str) -> bool {
    run_tmux(&["has-session", "-t", name]).is_ok()
}

/// Kill a tmux session by name.
pub fn kill_session(name: &str) -> Result<(), String> {
    run_tmux(&["kill-session", "-t", name]).map(|_| ())
}

/// Rename a tmux session. tmux refuses if `new` is already in use, so the
/// returned error is propagated up to the picker for the user to see.
pub fn rename_session(old: &str, new: &str) -> Result<(), String> {
    run_tmux(&["rename-session", "-t", old, new]).map(|_| ())
}

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
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
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

/// Capture the last `lines` lines of the session's active pane buffer.
/// Returns up to `lines` lines (may be fewer if the pane buffer is shorter).
pub fn pane_capture(session: &str, lines: u16) -> Result<String, String> {
    let start = format!("-{lines}");
    let out = run_tmux(&["capture-pane", "-t", session, "-p", "-J", "-S", &start])?;
    // Strip trailing whitespace/newlines for cleaner rendering.
    Ok(out.trim_end().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_session_line
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_session_line_valid() {
        let commands = HashMap::new();
        let s = parse_session_line("main|2|0|1000", 1300, &commands).unwrap();
        assert_eq!(s.name, "main");
        assert_eq!(s.window_count, 2);
        assert!(!s.attached);
        assert_eq!(s.last_activity, Duration::from_secs(300));
        assert_eq!(s.current_command, "?");
    }

    #[test]
    fn test_parse_session_line_attached() {
        let commands = HashMap::new();
        let s = parse_session_line("work|1|1|1000", 1300, &commands).unwrap();
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
        // window_count and activity_epoch fall back to 0 gracefully.
        let s = parse_session_line("test|notanum|0|notanum", 1000, &commands).unwrap();
        assert_eq!(s.window_count, 0);
        assert_eq!(s.last_activity, Duration::from_secs(0));
    }

    #[test]
    fn test_parse_session_line_with_full_metadata() {
        let commands = HashMap::new();
        let s = parse_session_line(
            "main|2|0|1000|My Label|/home/u/git/app|PR #1|1500",
            1300,
            &commands,
        )
        .unwrap();
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

    // -----------------------------------------------------------------------
    // parse_pane_commands
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_pane_commands() {
        let input = "main|1|1|bash\nmain|0|1|vim\nwork|1|1|claude\n";
        let map = parse_pane_commands(input);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("main").map(String::as_str), Some("bash"));
        assert_eq!(map.get("work").map(String::as_str), Some("claude"));
    }

    #[test]
    fn test_parse_pane_commands_empty() {
        let map = parse_pane_commands("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_pane_commands_inactive_window() {
        // window_active=0, so this entry must NOT appear in the map.
        let input = "main|0|1|vim\n";
        let map = parse_pane_commands(input);
        assert!(map.is_empty());
    }
}
