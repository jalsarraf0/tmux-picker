use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::session::Session;

const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

/// Line printed between the outputs of chained commands. Printable ASCII
/// only: tmux 3.4 octal-escapes control characters in `-p` output. Contains
/// no `|`, so it can never be mistaken for a pane/session/window line.
const SEP: &str = "::tmux-picker-sep::";

/// `#{session_name}|…` per pane, shared by the session and marker queries.
const PANE_FORMAT: &str = "#{session_name}|#{window_active}|#{pane_active}|#{pane_current_command}";
const SESSION_FORMAT: &str = "#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}|#{@tmux_picker_label}|#{@tmux_picker_project}|#{@tmux_picker_purpose}|#{@tmux_picker_label_at}";

/// Resolve the tmux binary once: prefer /usr/bin/tmux, else let the
/// process spawn search PATH (no `which` subprocess).
fn tmux_bin() -> &'static str {
    static BIN: OnceLock<&'static str> = OnceLock::new();
    BIN.get_or_init(|| {
        if std::path::Path::new("/usr/bin/tmux").exists() {
            "/usr/bin/tmux"
        } else {
            "tmux"
        }
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Exact-match session target. A bare `-t name` makes tmux fall back to
/// prefix matching, so `-t ma` would hit `main`.
fn session_target(name: &str) -> String {
    format!("={name}")
}

/// Exact-match target for the session's active pane, for commands whose
/// `-t` is a pane target (options, display-message, capture-pane).
fn pane_target(name: &str) -> String {
    format!("={name}:")
}

/// tmux treats an argument ending in `;` as a command separator and drops
/// the `;`. Escape it so user-supplied values reach tmux verbatim.
fn escape_arg(value: &str) -> Cow<'_, str> {
    match value.strip_suffix(';') {
        Some(head) => Cow::Owned(format!("{head}\\;")),
        None => Cow::Borrowed(value),
    }
}

/// Raw result of one tmux invocation.
struct TmuxOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Spawn tmux, drain stdout and stderr together with `poll(2)` so a large
/// output can never fill a pipe and deadlock, and enforce `TMUX_TIMEOUT`.
/// No sleep-polling: the call returns as soon as tmux exits.
fn exec_tmux(args: &[&str]) -> Result<TmuxOutput, String> {
    let mut child = Command::new(tmux_bin())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;
    let deadline = Instant::now() + TMUX_TIMEOUT;

    let mut streams: [(Option<File>, Vec<u8>); 2] = [
        (
            child.stdout.take().map(|p| File::from(OwnedFd::from(p))),
            Vec::new(),
        ),
        (
            child.stderr.take().map(|p| File::from(OwnedFd::from(p))),
            Vec::new(),
        ),
    ];
    let mut chunk = [0u8; 16 * 1024];
    let timed_out = loop {
        let mut fds = [libc::pollfd {
            fd: -1,
            events: libc::POLLIN,
            revents: 0,
        }; 2];
        let mut open = 0;
        for (slot, (file, _)) in streams.iter().enumerate() {
            if let Some(file) = file {
                fds[slot].fd = file.as_raw_fd();
                open += 1;
            }
        }
        if open == 0 {
            break false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break true;
        }
        let timeout_ms = i32::try_from(remaining.as_millis() + 1).unwrap_or(i32::MAX);
        // Entries with fd = -1 are ignored by poll(2).
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };
        if rc < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break true;
        }
        for (slot, pfd) in fds.iter().enumerate() {
            if pfd.fd < 0 || pfd.revents == 0 {
                continue;
            }
            let (file, sink) = &mut streams[slot];
            match file.as_mut().map(|f| f.read(&mut chunk)) {
                Some(Ok(n)) if n > 0 => sink.extend_from_slice(&chunk[..n]),
                Some(Err(e)) if e.kind() == std::io::ErrorKind::Interrupted => {}
                _ => *file = None, // EOF or a read error: stop watching
            }
        }
    };

    // Both pipes are closed, so tmux is exiting; reap it within the budget.
    let status = if timed_out {
        None
    } else {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_micros(200));
                }
                Ok(None) => break None,
                Err(e) => return Err(format!("failed to wait on tmux: {e}")),
            }
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("tmux command timed out".into());
    };

    let [(_, stdout), (_, stderr)] = streams;
    Ok(TmuxOutput {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// Run tmux; stdout on success, stderr (trimmed) on failure.
fn run_tmux(args: &[&str]) -> Result<String, String> {
    let out = exec_tmux(args)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(out.stderr.trim_end().to_string())
    }
}

/// Append `display-message -p SEP` after every command so one tmux process
/// can run them all and the output can be split back per command.
fn chain<'a>(commands: &[&[&'a str]]) -> Vec<&'a str> {
    let mut args = Vec::new();
    for (i, command) in commands.iter().enumerate() {
        if i > 0 {
            args.push(";");
        }
        args.extend_from_slice(command);
        args.extend_from_slice(&[";", "display-message", "-p", SEP]);
    }
    args
}

/// Split the stdout of a `chain` into one chunk per completed command. A
/// chain stops at its first failing command, so the result can be shorter
/// than the chain; output after the last separator is discarded.
fn split_chained(stdout: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = stdout;
    while let Some(pos) = rest.find(SEP) {
        chunks.push(&rest[..pos]);
        let after = &rest[pos + SEP.len()..];
        rest = after.strip_prefix('\n').unwrap_or(after);
    }
    chunks
}

/// Make captured pane text safe to paint: tabs become spaces and other
/// control characters are dropped, so they cannot shift or corrupt cells.
pub fn sanitize_line(line: &str) -> Cow<'_, str> {
    if !line.chars().any(char::is_control) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len() + 8);
    for c in line.chars() {
        match c {
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// The last `max` lines of a captured screen, trailing blank lines dropped.
pub fn tail_lines(captured: &str, max: usize) -> String {
    let lines: Vec<&str> = captured.trim_end().lines().collect();
    let start = lines.len().saturating_sub(max);
    let mut out = String::new();
    for (i, line) in lines[start..].iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(sanitize_line(line.trim_end()).as_ref());
    }
    out
}

/// Last non-blank line of a captured pane, or "(empty)".
fn last_nonblank(captured: &str) -> String {
    captured
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map_or_else(
            || "(empty)".to_string(),
            |l| sanitize_line(l.trim()).into_owned(),
        )
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

/// Same as `list_sessions`, also attaching a process marker to each session
/// from `markers`. One tmux process answers both the pane and session
/// queries.
pub fn list_sessions_with_markers(
    markers: &crate::config::Markers,
) -> Result<Vec<Session>, String> {
    list_sessions_impl(Some(markers))
}

fn list_sessions_impl(markers: Option<&crate::config::Markers>) -> Result<Vec<Session>, String> {
    let out = exec_tmux(&chain(&[
        &["list-panes", "-a", "-F", PANE_FORMAT],
        &["list-sessions", "-F", SESSION_FORMAT],
    ]))?;
    let chunks = split_chained(&out.stdout);
    let (Some(pane_output), Some(session_output)) = (chunks.first(), chunks.get(1)) else {
        let err = out.stderr.trim_end();
        return Err(if err.is_empty() {
            "tmux query failed".into()
        } else {
            err.to_string()
        });
    };
    let commands = parse_pane_commands(pane_output);

    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut sessions: Vec<Session> = session_output
        .lines()
        .filter_map(|line| parse_session_line(line, now_epoch, &commands))
        .collect();

    if let Some(markers) = markers {
        let all_commands = parse_all_pane_commands(pane_output);
        for session in &mut sessions {
            if let Some(commands) = all_commands.get(&session.name) {
                session.marker = markers.lookup(commands);
            }
        }
    }

    sessions.sort();
    Ok(sessions)
}

/// Re-run marker discovery on an existing session list (used after a
/// SIGHUP config reload changes the marker table).
pub fn populate_markers(sessions: &mut [Session], markers: &crate::config::Markers) {
    let pane_output = run_tmux(&["list-panes", "-a", "-F", PANE_FORMAT]).unwrap_or_default();
    let all = parse_all_pane_commands(&pane_output);
    for s in sessions {
        s.marker = all.get(&s.name).and_then(|cmds| markers.lookup(cmds));
    }
}

/// One window's name and the active pane's last non-blank line, for the
/// picker's Tab-toggled windows preview.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowSnapshot {
    /// Window index as tmux numbers it (`None` for the "more" summary row).
    pub index: Option<u32>,
    pub name: String,
    pub last_line: String,
    /// True for the session's current window.
    pub active: bool,
}

/// List up to `max` windows of `session` with each active pane's last
/// non-blank line. Two tmux processes in total: one lists the windows, one
/// captures every pane. A trailing "(N more)" row is appended when the
/// session has more than `max` windows.
pub fn list_windows(session: &str, max: usize) -> Result<Vec<WindowSnapshot>, String> {
    let raw = run_tmux(&[
        "list-windows",
        "-t",
        &session_target(session),
        "-F",
        "#{window_index}|#{window_active}|#{pane_id}|#{window_name}",
    ])?;
    let entries: Vec<(Option<u32>, bool, &str, &str)> = raw
        .lines()
        .filter_map(|line| {
            // The name comes last, so a `|` inside it stays in the name.
            let mut fields = line.splitn(4, '|');
            let (Some(index), Some(active), Some(pane_id), Some(name)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                return None;
            };
            Some((index.parse().ok(), active == "1", pane_id, name))
        })
        .collect();

    let shown = &entries[..entries.len().min(max)];
    let captures: Vec<[&str; 7]> = shown
        .iter()
        .map(|&(_, _, pane_id, _)| ["capture-pane", "-p", "-J", "-S", "-3", "-t", pane_id])
        .collect();
    let commands: Vec<&[&str]> = captures.iter().map(<[&str; 7]>::as_slice).collect();
    // Use whatever the chain produced even if a pane vanished midway.
    let captured = if commands.is_empty() {
        None
    } else {
        exec_tmux(&chain(&commands)).ok()
    };
    let chunks = captured
        .as_ref()
        .map(|o| split_chained(&o.stdout))
        .unwrap_or_default();

    let mut out: Vec<WindowSnapshot> = shown
        .iter()
        .enumerate()
        .map(|(i, &(index, active, _, name))| WindowSnapshot {
            index,
            name: name.to_owned(),
            last_line: chunks
                .get(i)
                .map_or_else(|| "(gone)".to_string(), |c| last_nonblank(c)),
            active,
        })
        .collect();
    if entries.len() > max {
        out.push(WindowSnapshot {
            index: None,
            name: "…".into(),
            last_line: format!("({} more)", entries.len() - max),
            active: false,
        });
    }
    Ok(out)
}

/// Check whether a session with exactly this name exists.
pub fn session_exists(name: &str) -> bool {
    run_tmux(&["has-session", "-t", &session_target(name)]).is_ok()
}

/// Kill the session with exactly this name.
pub fn kill_session(name: &str) -> Result<(), String> {
    run_tmux(&["kill-session", "-t", &session_target(name)]).map(|_| ())
}

/// Rename the session named exactly `old` to `new`.
pub fn rename_session(old: &str, new: &str) -> Result<(), String> {
    run_tmux(&[
        "rename-session",
        "-t",
        &session_target(old),
        &escape_arg(new),
    ])
    .map(|_| ())
}

/// Set a session-level user option (`@key`).
pub fn set_user_option(session: &str, key: &str, value: &str) -> Result<(), String> {
    set_user_options(session, &[(key, Some(value))])
}

/// Unset a session-level user option (`@key`).
pub fn unset_user_option(session: &str, key: &str) -> Result<(), String> {
    set_user_options(session, &[(key, None)])
}

/// Apply several user-option changes in a single tmux process: `Some(v)`
/// sets `@key` to `v`, `None` unsets it.
pub fn set_user_options(session: &str, changes: &[(&str, Option<&str>)]) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }
    let target = pane_target(session);
    let opts: Vec<String> = changes.iter().map(|(key, _)| format!("@{key}")).collect();
    let values: Vec<Option<Cow<'_, str>>> =
        changes.iter().map(|(_, v)| v.map(escape_arg)).collect();
    let mut args: Vec<&str> = Vec::new();
    for (i, (opt, value)) in opts.iter().zip(&values).enumerate() {
        if i > 0 {
            args.push(";");
        }
        match value {
            Some(v) => args.extend_from_slice(&["set-option", "-t", &target, opt, v]),
            None => args.extend_from_slice(&["set-option", "-t", &target, "-u", opt]),
        }
    }
    run_tmux(&args).map(|_| ())
}

/// Read a session-level user option (`@key`); None when unset or empty.
pub fn get_user_option(session: &str, key: &str) -> Option<String> {
    let opt = format!("#{{@{key}}}");
    let mut values = display(session, &[&opt]).ok()?;
    values.pop().filter(|v| !v.is_empty())
}

/// Expand several tmux formats against the session's active pane in one
/// tmux process. Errs with "session '…' does not exist" when there is no
/// session with exactly that name.
pub fn display(session: &str, formats: &[&str]) -> Result<Vec<String>, String> {
    let target = pane_target(session);
    let commands: Vec<[&str; 5]> = std::iter::once("#{session_name}")
        .chain(formats.iter().copied())
        .map(|f| ["display-message", "-p", "-t", target.as_str(), f])
        .collect();
    let refs: Vec<&[&str]> = commands.iter().map(<[&str; 5]>::as_slice).collect();
    let out = exec_tmux(&chain(&refs))?;
    let mut values = split_chained(&out.stdout)
        .into_iter()
        .map(|chunk| chunk.strip_suffix('\n').unwrap_or(chunk).to_owned());
    // display-message does not fail on a missing target; it expands to
    // empty strings instead, so check the name came back.
    if values.next().as_deref() != Some(session) {
        let err = out.stderr.trim_end();
        return Err(if err.is_empty() || err.contains("can't find") {
            format!("session '{session}' does not exist")
        } else {
            err.to_string()
        });
    }
    let values: Vec<String> = values.collect();
    if values.len() == formats.len() {
        Ok(values)
    } else {
        Err("unexpected display-message output".into())
    }
}

/// The current working directory of the session's active pane.
pub fn pane_current_path(session: &str) -> Result<String, String> {
    display(session, &["#{pane_current_path}"]).map(|mut v| v.pop().unwrap_or_default())
}

/// The last `lines` non-blank-trailing lines of what the session's active
/// pane shows right now, sanitized for painting. (Capturing from the visible
/// screen, not `-S -N`: that starts N lines up in the scrollback and would
/// return history above the screen rather than the newest output.)
pub fn pane_capture(session: &str, lines: u16) -> Result<String, String> {
    let out = run_tmux(&["capture-pane", "-p", "-J", "-t", &pane_target(session)])?;
    Ok(tail_lines(&out, usize::from(lines)))
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

    // -----------------------------------------------------------------------
    // Process plumbing helpers
    // -----------------------------------------------------------------------

    #[test]
    fn escape_arg_protects_trailing_semicolon() {
        assert_eq!(escape_arg("plain"), "plain");
        assert_eq!(escape_arg("fix;"), "fix\\;");
        assert_eq!(escape_arg(";"), "\\;");
        assert_eq!(escape_arg("a;b"), "a;b");
    }

    #[test]
    fn targets_force_exact_match() {
        assert_eq!(session_target("main"), "=main");
        assert_eq!(pane_target("main"), "=main:");
    }

    #[test]
    fn chain_terminates_every_command_with_separator() {
        let args = chain(&[&["list-panes", "-a"], &["list-sessions"]]);
        assert_eq!(
            args,
            [
                "list-panes",
                "-a",
                ";",
                "display-message",
                "-p",
                SEP,
                ";",
                "list-sessions",
                ";",
                "display-message",
                "-p",
                SEP,
            ]
        );
    }

    #[test]
    fn split_chained_keeps_only_completed_chunks() {
        let out = format!("a|1\nb|2\n{SEP}\nmain|1\n{SEP}\npartial");
        assert_eq!(split_chained(&out), ["a|1\nb|2\n", "main|1\n"]);
        assert_eq!(split_chained(&format!("{SEP}\n")), [""]);
        assert!(split_chained("no separator at all").is_empty());
    }

    #[test]
    fn tail_lines_takes_the_newest_lines() {
        let screen = "one\ntwo\nthree\nfour\n\n\n   \n";
        assert_eq!(tail_lines(screen, 2), "three\nfour");
        assert_eq!(tail_lines(screen, 10), "one\ntwo\nthree\nfour");
        assert_eq!(tail_lines("", 3), "");
    }

    #[test]
    fn sanitize_line_expands_tabs_and_drops_controls() {
        assert_eq!(sanitize_line("a\tb"), "a    b");
        assert_eq!(sanitize_line("x\u{7}y\u{1b}z"), "xyz");
        assert!(matches!(sanitize_line("clean"), Cow::Borrowed(_)));
    }

    #[test]
    fn last_nonblank_skips_trailing_blank_lines() {
        assert_eq!(last_nonblank("$ make\nok\n\n  \n"), "ok");
        assert_eq!(last_nonblank("\n\n"), "(empty)");
    }
}
