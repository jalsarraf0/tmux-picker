use std::time::Duration;

use crate::action::Action;
use crate::session::{Session, validate_session_name};
use crate::tmux;

const TIMEOUT_SECS: u64 = 10;

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mode {
    Pick,
    NewInput,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub mode: Mode,
    pub input: String,
    pub action: Option<Action>,
    pub timeout_remaining: Duration,
    pub input_error: Option<String>,
}

impl App {
    pub fn new(sessions: Vec<Session>) -> Self {
        App {
            sessions,
            selected: 0,
            mode: Mode::Pick,
            input: String::new(),
            action: None,
            timeout_remaining: Duration::from_secs(TIMEOUT_SECS),
            input_error: None,
        }
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.reset_timeout();
    }

    pub fn move_down(&mut self) {
        if !self.sessions.is_empty() && self.selected < self.sessions.len() - 1 {
            self.selected += 1;
        }
        self.reset_timeout();
    }

    // -----------------------------------------------------------------------
    // Selection
    // -----------------------------------------------------------------------

    /// 1-indexed. If n is in range, set selected and confirm. Otherwise no-op.
    pub fn select_by_number(&mut self, n: usize) {
        if n == 0 || n > self.sessions.len() {
            return;
        }
        self.selected = n - 1;
        self.confirm_selection();
    }

    pub fn confirm_selection(&mut self) {
        if let Some(session) = self.sessions.get(self.selected) {
            self.action = Some(Action::Attach(session.name.clone()));
        }
    }

    // -----------------------------------------------------------------------
    // New-session input
    // -----------------------------------------------------------------------

    pub fn enter_new_mode(&mut self) {
        self.mode = Mode::NewInput;
        self.input.clear();
        self.input_error = None;
    }

    pub fn cancel_input(&mut self) {
        self.mode = Mode::Pick;
        self.input.clear();
        self.input_error = None;
    }

    pub fn input_char(&mut self, c: char) {
        self.input.push(c);
        self.input_error = None;
    }

    pub fn input_backspace(&mut self) {
        self.input.pop();
        self.input_error = None;
    }

    /// Confirm the current input string.
    ///
    /// - Empty input → cancel back to Pick.
    /// - Invalid name (validate_session_name returns None) → set input_error.
    /// - Name exists in tmux → Attach.
    /// - Name does not exist → New.
    pub fn confirm_input(&mut self) {
        if self.input.is_empty() {
            self.cancel_input();
            return;
        }

        match validate_session_name(&self.input) {
            None => {
                self.input_error = Some(format!("'{}' is not a valid session name", self.input));
            }
            Some(name) => {
                if tmux::session_exists(&name) {
                    self.action = Some(Action::Attach(name));
                } else {
                    self.action = Some(Action::New(name));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Shell fallback
    // -----------------------------------------------------------------------

    pub fn shell(&mut self) {
        self.action = Some(Action::Shell);
    }

    // -----------------------------------------------------------------------
    // Tick / timeout
    // -----------------------------------------------------------------------

    /// Subtract elapsed from the remaining timeout. If timeout reaches zero
    /// and we are in Pick mode, call auto_select.
    pub fn tick(&mut self, elapsed: Duration) {
        if self.mode != Mode::Pick {
            return;
        }
        self.timeout_remaining = self.timeout_remaining.saturating_sub(elapsed);
        if self.timeout_remaining.is_zero() {
            self.auto_select();
        }
    }

    // -----------------------------------------------------------------------
    // Quit predicate
    // -----------------------------------------------------------------------

    pub fn should_quit(&self) -> bool {
        self.action.is_some()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Choose the first detached session, or the first session, or Shell if
    /// the session list is empty.
    fn auto_select(&mut self) {
        if self.sessions.is_empty() {
            self.action = Some(Action::Shell);
            return;
        }

        // Prefer the first detached session.
        if let Some(idx) = self.sessions.iter().position(|s| !s.attached) {
            self.selected = idx;
        } else {
            self.selected = 0;
        }

        self.confirm_selection();
    }

    fn reset_timeout(&mut self) {
        self.timeout_remaining = Duration::from_secs(TIMEOUT_SECS);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sessions() -> Vec<Session> {
        vec![
            Session {
                name: "main".into(),
                window_count: 1,
                attached: false,
                current_command: "bash".into(),
                last_activity: Duration::from_secs(100),
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
            },
            Session {
                name: "work".into(),
                window_count: 2,
                attached: false,
                current_command: "vim".into(),
                last_activity: Duration::from_secs(500),
            },
        ]
    }

    #[test]
    fn test_new_starts_at_zero() {
        let app = App::new(make_sessions());
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_move_down() {
        let mut app = App::new(make_sessions());
        app.move_down();
        assert_eq!(app.selected, 1);
        app.move_down();
        assert_eq!(app.selected, 2);
        // Already at last; should stay.
        app.move_down();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_move_up() {
        let mut app = App::new(make_sessions());
        // At 0 already; should stay.
        app.move_up();
        assert_eq!(app.selected, 0);
        app.move_down();
        assert_eq!(app.selected, 1);
        app.move_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_by_number() {
        let mut app = App::new(make_sessions());
        app.select_by_number(2);
        assert_eq!(app.selected, 1);
        assert!(app.action.is_some());
        if let Some(Action::Attach(name)) = &app.action {
            assert_eq!(name, "claude-aihelp");
        } else {
            panic!("expected Attach action");
        }
    }

    #[test]
    fn test_select_by_number_out_of_range() {
        let mut app = App::new(make_sessions());
        app.select_by_number(5);
        assert!(app.action.is_none());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_by_number_zero() {
        let mut app = App::new(make_sessions());
        app.select_by_number(0);
        assert!(app.action.is_none());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_confirm_selection() {
        let mut app = App::new(make_sessions());
        app.move_down();
        app.confirm_selection();
        if let Some(Action::Attach(name)) = &app.action {
            assert_eq!(name, "claude-aihelp");
        } else {
            panic!("expected Attach action");
        }
        assert!(app.should_quit());
    }

    #[test]
    fn test_shell() {
        let mut app = App::new(make_sessions());
        app.shell();
        assert!(matches!(app.action, Some(Action::Shell)));
        assert!(app.should_quit());
    }

    #[test]
    fn test_enter_new_mode() {
        let mut app = App::new(make_sessions());
        app.input.push_str("leftover");
        app.enter_new_mode();
        assert_eq!(app.mode, Mode::NewInput);
        assert!(app.input.is_empty());
        assert!(app.input_error.is_none());
    }

    #[test]
    fn test_cancel_input() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.input_char('x');
        app.cancel_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.input.is_empty());
        assert!(app.input_error.is_none());
    }

    #[test]
    fn test_input_char_and_backspace() {
        let mut app = App::new(make_sessions());
        app.input_char('a');
        app.input_char('b');
        assert_eq!(app.input, "ab");
        app.input_backspace();
        assert_eq!(app.input, "a");
        app.input_backspace();
        assert_eq!(app.input, "");
        // Extra backspace on empty — must not panic.
        app.input_backspace();
        assert_eq!(app.input, "");
    }

    #[test]
    fn test_confirm_empty_input_cancels() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.confirm_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_confirm_invalid_input() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        // "..." → validate_session_name returns None
        for c in "...".chars() {
            app.input_char(c);
        }
        app.confirm_input();
        assert!(app.input_error.is_some());
        assert!(app.action.is_none());
    }

    #[test]
    fn test_timeout_auto_selects() {
        let mut app = App::new(make_sessions());
        app.tick(Duration::from_secs(10));
        assert!(app.action.is_some());
    }

    #[test]
    fn test_timeout_picks_first_detached() {
        // make_sessions: index 0 = "main" (detached), index 1 = "claude-aihelp" (attached)
        let mut app = App::new(make_sessions());
        app.tick(Duration::from_secs(10));
        if let Some(Action::Attach(name)) = &app.action {
            // First detached in the list is index 0, "main".
            assert_eq!(name, "main");
        } else {
            panic!("expected Attach action");
        }
    }

    #[test]
    fn test_interaction_resets_timeout() {
        let mut app = App::new(make_sessions());
        // Advance to 1 second remaining.
        app.tick(Duration::from_secs(9));
        assert!(app.action.is_none());
        // Any navigation resets the timeout.
        app.move_down();
        // Another 9 seconds — should still be 1s remaining, no action yet.
        app.tick(Duration::from_secs(9));
        assert!(app.action.is_none());
    }

    #[test]
    fn test_timeout_does_not_fire_in_input_mode() {
        let mut app = App::new(make_sessions());
        app.enter_new_mode();
        app.tick(Duration::from_secs(15));
        assert!(app.action.is_none());
    }

    #[test]
    fn test_empty_sessions_auto_select_shell() {
        let mut app = App::new(vec![]);
        app.tick(Duration::from_secs(10));
        assert!(matches!(app.action, Some(Action::Shell)));
    }
}
