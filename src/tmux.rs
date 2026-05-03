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
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 {
            continue;
        }
        let session_name = parts[0];
        let window_active = parts[1];
        let pane_active = parts[2];
        let command = parts[3];

        if window_active == "1" && pane_active == "1" {
            map.insert(session_name.to_string(), command.to_string());
        }
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
    // Collect pane commands first (best-effort; ignore errors).
    let pane_output = run_tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}",
    ])
    .unwrap_or_default();
    let commands = parse_pane_commands(&pane_output);

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

    sessions.sort();
    Ok(sessions)
}

/// Returns true if a tmux session with the given name exists.
pub fn session_exists(name: &str) -> bool {
    run_tmux(&["has-session", "-t", name]).is_ok()
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
