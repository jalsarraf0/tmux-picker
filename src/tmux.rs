use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use crate::session::Session;

const TMUX_BIN: &str = "/usr/bin/tmux";
const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn run_tmux(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(TMUX_BIN)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;

    // Poll with timeout — tmux commands typically finish in < 100ms
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
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
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to read tmux output: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
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
/// Format: `session_name|window_count|session_attached|session_activity_epoch`
///
/// Returns `None` if the line does not have exactly 4 `|`-separated fields.
/// Numeric parse failures fall back to 0 (graceful degradation).
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

    Some(Session {
        name,
        window_count,
        attached,
        current_command,
        last_activity,
    })
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

    // Query sessions.
    let session_output = run_tmux(&[
        "list-sessions",
        "-F",
        "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}",
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
    Command::new(TMUX_BIN)
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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
