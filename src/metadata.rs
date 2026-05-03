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
