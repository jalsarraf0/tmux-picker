//! Per-session metadata stored as tmux user-options.

#[derive(Debug, Default, Clone, Eq, PartialEq)]
/// Optional label, project, purpose, and timestamp associated with a session.
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
            out.push_str(&format!("label_at = {v}\n"));
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
            .map_or(0, |d| d.as_secs());
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
///
/// Search order for the project root:
///   1. Walk up from the pane cwd until a `.git` directory exists.
///   2. If that fails (pane sits above any repo, e.g. `$HOME` at SSH login),
///      try `~/git/<session-name>` so the user gets a useful label when
///      they create a new session named after a project.
///   3. Otherwise fall back to the pane cwd itself.
pub fn auto_detect(session: &str) -> Result<(), String> {
    if !tmux::session_exists(session) {
        return Err(format!("session '{session}' does not exist"));
    }
    let cwd = tmux::pane_current_path(session)?;
    let project = walk_up_to_git_root(&cwd)
        .or_else(|| project_for_session_name(session))
        .unwrap_or(cwd);

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
        tmux::set_user_option(session, "tmux_picker_purpose", &format!("branch:{branch}"))?;
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

/// `~/git/<name>` if that directory exists, else None.
///
/// Used as an `auto_detect` fallback when the pane cwd has no nearby
/// `.git`. tmux-picker assumes the user keeps repos under `~/git/` per the
/// project's own filesystem-containment convention.
fn project_for_session_name(name: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    project_for_session_name_under(name, &home)
}

fn project_for_session_name_under(name: &str, home: &str) -> Option<String> {
    if name.is_empty() || name.contains('/') {
        return None;
    }
    let candidate = std::path::PathBuf::from(home).join("git").join(name);
    if candidate.is_dir() {
        candidate.to_str().map(String::from)
    } else {
        None
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
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
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

    #[test]
    fn walk_up_finds_repo_root() {
        let crate_root = env!("CARGO_MANIFEST_DIR");
        let started_in = format!("{crate_root}/src");
        let found = walk_up_to_git_root(&started_in).unwrap();
        assert_eq!(found, crate_root);
    }

    #[test]
    fn walk_up_terminates_below_root_without_git() {
        // Don't assert specific result (some systems may have /.git);
        // assert termination without panic.
        let _ = walk_up_to_git_root("/tmp");
    }

    fn unique_tempdir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        std::env::temp_dir().join(format!(
            "tmux-picker-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn project_for_session_name_finds_existing_dir() {
        let tmp = unique_tempdir("pj");
        let git_dir = tmp.join("git").join("acme");
        std::fs::create_dir_all(&git_dir).unwrap();

        let got = project_for_session_name_under("acme", tmp.to_str().unwrap());

        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(got.as_deref(), git_dir.to_str());
    }

    #[test]
    fn project_for_session_name_returns_none_when_missing() {
        let tmp = unique_tempdir("pj-missing");
        std::fs::create_dir_all(&tmp).unwrap();

        let got = project_for_session_name_under("does-not-exist", tmp.to_str().unwrap());

        let _ = std::fs::remove_dir_all(&tmp);
        assert!(got.is_none());
    }

    #[test]
    fn project_for_session_name_rejects_slash() {
        assert!(project_for_session_name_under("a/b", "/tmp").is_none());
        assert!(project_for_session_name_under("", "/tmp").is_none());
    }
}
