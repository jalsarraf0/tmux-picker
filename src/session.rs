use std::cmp::Ordering;
use std::time::Duration;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Session {
    pub name: String,
    pub window_count: u32,
    pub attached: bool,
    pub current_command: String,
    pub last_activity: Duration,
}

impl Session {
    /// Returns true if the session name starts with "claude-".
    pub fn is_claude(&self) -> bool {
        self.name.starts_with("claude-")
    }

    /// Returns true if the session has been idle for more than 3600 seconds.
    pub fn is_stale(&self) -> bool {
        self.last_activity.as_secs() > 3600
    }

    /// Returns a human-readable activity string.
    pub fn activity_display(&self) -> String {
        let secs = self.last_activity.as_secs();
        if secs < 60 {
            format!("active {}s", secs)
        } else if secs < 3600 {
            format!("idle {}m", secs / 60)
        } else if secs < 86400 {
            format!("idle {}h", secs / 3600)
        } else {
            format!("idle {}d", secs / 86400)
        }
    }

    /// Returns a human-readable window count string.
    pub fn windows_display(&self) -> String {
        format!("{} win", self.window_count)
    }
}

impl Ord for Session {
    fn cmp(&self, other: &Self) -> Ordering {
        // Attached sessions sort before detached.
        match (self.attached, other.attached) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        // Most recent activity first (smaller Duration = more recent = less).
        match self.last_activity.cmp(&other.last_activity) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // Alphabetical by name as tiebreaker.
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Sanitize a session name: replace invalid characters with hyphens, collapse
/// consecutive hyphens, trim leading/trailing hyphens.  Returns `None` if the
/// result would be empty.
///
/// Valid characters are ASCII alphanumeric plus hyphen and underscore.
pub fn validate_session_name(input: &str) -> Option<String> {
    // Replace every invalid character with a hyphen.
    let replaced: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens into a single hyphen.
    let mut collapsed = String::with_capacity(replaced.len());
    let mut last_was_hyphen = false;
    for c in replaced.chars() {
        if c == '-' {
            if !last_was_hyphen {
                collapsed.push(c);
            }
            last_was_hyphen = true;
        } else {
            collapsed.push(c);
            last_was_hyphen = false;
        }
    }

    // Trim leading and trailing hyphens.
    let trimmed = collapsed.trim_matches('-').to_string();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make(name: &str, attached: bool, secs: u64) -> Session {
        Session {
            name: name.to_string(),
            window_count: 1,
            attached,
            current_command: String::new(),
            last_activity: Duration::from_secs(secs),
        }
    }

    // -----------------------------------------------------------------------
    // activity_display boundaries
    // -----------------------------------------------------------------------

    #[test]
    fn activity_display_0s() {
        assert_eq!(make("s", false, 0).activity_display(), "active 0s");
    }

    #[test]
    fn activity_display_30s() {
        assert_eq!(make("s", false, 30).activity_display(), "active 30s");
    }

    #[test]
    fn activity_display_59s() {
        assert_eq!(make("s", false, 59).activity_display(), "active 59s");
    }

    #[test]
    fn activity_display_60s() {
        assert_eq!(make("s", false, 60).activity_display(), "idle 1m");
    }

    #[test]
    fn activity_display_300s() {
        assert_eq!(make("s", false, 300).activity_display(), "idle 5m");
    }

    #[test]
    fn activity_display_3599s() {
        assert_eq!(make("s", false, 3599).activity_display(), "idle 59m");
    }

    #[test]
    fn activity_display_3600s() {
        assert_eq!(make("s", false, 3600).activity_display(), "idle 1h");
    }

    #[test]
    fn activity_display_7200s() {
        assert_eq!(make("s", false, 7200).activity_display(), "idle 2h");
    }

    #[test]
    fn activity_display_86400s() {
        assert_eq!(make("s", false, 86400).activity_display(), "idle 1d");
    }

    #[test]
    fn activity_display_172800s() {
        assert_eq!(make("s", false, 172800).activity_display(), "idle 2d");
    }

    // -----------------------------------------------------------------------
    // is_claude
    // -----------------------------------------------------------------------

    #[test]
    fn is_claude_true() {
        assert!(make("claude-aihelp", false, 0).is_claude());
    }

    #[test]
    fn is_claude_prefix_exact() {
        assert!(make("claude-", false, 0).is_claude());
    }

    #[test]
    fn is_not_claude_no_prefix() {
        assert!(!make("main", false, 0).is_claude());
    }

    #[test]
    fn is_not_claude_substring() {
        // "claude" without trailing hyphen is not a match
        assert!(!make("claude", false, 0).is_claude());
    }

    #[test]
    fn is_not_claude_suffix() {
        assert!(!make("my-claude-session", false, 0).is_claude());
    }

    // -----------------------------------------------------------------------
    // is_stale
    // -----------------------------------------------------------------------

    #[test]
    fn is_stale_at_3601() {
        assert!(make("s", false, 3601).is_stale());
    }

    #[test]
    fn is_not_stale_at_3600() {
        // Boundary: exactly 3600s is NOT stale (> not >=)
        assert!(!make("s", false, 3600).is_stale());
    }

    #[test]
    fn is_not_stale_at_3599() {
        assert!(!make("s", false, 3599).is_stale());
    }

    // -----------------------------------------------------------------------
    // Sort order
    // -----------------------------------------------------------------------

    #[test]
    fn sort_attached_before_detached() {
        let detached = make("alpha", false, 0);
        let attached = make("beta", true, 0);
        assert!(attached < detached);
    }

    #[test]
    fn sort_more_recent_activity_first() {
        let recent = make("a", false, 10);
        let older = make("b", false, 500);
        assert!(recent < older);
    }

    #[test]
    fn sort_alphabetical_tiebreaker() {
        let a = make("alpha", false, 100);
        let b = make("beta", false, 100);
        assert!(a < b);
    }

    #[test]
    fn sort_full_order() {
        let mut sessions = vec![
            make("zebra", false, 50),
            make("alpha", false, 200),
            make("work", true, 300),
            make("beta", false, 50),
        ];
        sessions.sort();
        // Attached first
        assert_eq!(sessions[0].name, "work");
        // Then detached sorted by activity ascending (50s before 200s)
        assert_eq!(sessions[1].name, "beta");
        assert_eq!(sessions[2].name, "zebra");
        // Alphabetical within same activity
        assert_eq!(sessions[3].name, "alpha");
    }

    #[test]
    fn sort_two_attached_by_activity() {
        let a = make("a", true, 10);
        let b = make("b", true, 200);
        let mut v = vec![b.clone(), a.clone()];
        v.sort();
        assert_eq!(v[0].name, "a");
        assert_eq!(v[1].name, "b");
    }

    // -----------------------------------------------------------------------
    // validate_session_name
    // -----------------------------------------------------------------------

    #[test]
    fn validate_valid_name() {
        assert_eq!(
            validate_session_name("my-session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_dots_replaced() {
        assert_eq!(
            validate_session_name("my.session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_colons_replaced() {
        assert_eq!(
            validate_session_name("my:session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_spaces_replaced() {
        assert_eq!(
            validate_session_name("my session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_collapse_hyphens() {
        assert_eq!(
            validate_session_name("my--session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_collapse_multiple_invalid_chars() {
        // Two adjacent invalid chars should collapse to one hyphen
        assert_eq!(
            validate_session_name("my..session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn validate_trim_leading_hyphens() {
        assert_eq!(
            validate_session_name("-session"),
            Some("session".to_string())
        );
    }

    #[test]
    fn validate_trim_trailing_hyphens() {
        assert_eq!(
            validate_session_name("session-"),
            Some("session".to_string())
        );
    }

    #[test]
    fn validate_trim_both_hyphens() {
        assert_eq!(
            validate_session_name("-session-"),
            Some("session".to_string())
        );
    }

    #[test]
    fn validate_empty_input() {
        assert_eq!(validate_session_name(""), None);
    }

    #[test]
    fn validate_only_invalid_chars() {
        assert_eq!(validate_session_name("...:::"), None);
    }

    #[test]
    fn validate_underscores_preserved() {
        assert_eq!(
            validate_session_name("my_session"),
            Some("my_session".to_string())
        );
    }

    #[test]
    fn validate_mixed_valid_invalid() {
        assert_eq!(
            validate_session_name("my.session_2:work"),
            Some("my-session_2-work".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // windows_display
    // -----------------------------------------------------------------------

    #[test]
    fn windows_display_one() {
        let s = Session {
            name: "s".to_string(),
            window_count: 1,
            attached: false,
            current_command: String::new(),
            last_activity: Duration::from_secs(0),
        };
        assert_eq!(s.windows_display(), "1 win");
    }

    #[test]
    fn windows_display_many() {
        let s = Session {
            name: "s".to_string(),
            window_count: 5,
            attached: false,
            current_command: String::new(),
            last_activity: Duration::from_secs(0),
        };
        assert_eq!(s.windows_display(), "5 win");
    }
}
