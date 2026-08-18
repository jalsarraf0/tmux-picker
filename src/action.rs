use std::fmt;

/// Action requested by the picker loop for the shell hook to execute.
pub enum Action {
    /// Attach to an existing session.
    Attach(String),
    /// Create and attach to a new session.
    New(String),
    /// Return control to the invoking shell.
    Shell,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Attach(name) => write!(f, "attach:{name}"),
            Action::New(name) => write!(f, "new:{name}"),
            Action::Shell => write!(f, "shell"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attach_display() {
        assert_eq!(
            Action::Attach("main".to_string()).to_string(),
            "attach:main"
        );
    }

    #[test]
    fn test_new_display() {
        assert_eq!(
            Action::New("my-session".to_string()).to_string(),
            "new:my-session"
        );
    }

    #[test]
    fn test_shell_display() {
        assert_eq!(Action::Shell.to_string(), "shell");
    }

    #[test]
    fn test_attach_with_hyphens() {
        assert_eq!(
            Action::Attach("claude-aihelp".to_string()).to_string(),
            "attach:claude-aihelp"
        );
    }

    #[test]
    fn test_attach_with_underscores() {
        assert_eq!(
            Action::Attach("ssh_48201".to_string()).to_string(),
            "attach:ssh_48201"
        );
    }
}
