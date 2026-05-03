use std::time::Duration;

use crate::action::Action;
use crate::config::Config;
use crate::session::{Session, validate_session_name};
use crate::tmux;

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mode {
    Pick,
    NewInput,
    Filter,
    ConfirmKill,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub sessions: Vec<Session>,
    /// Index into `filtered_indices` (which itself indexes into `sessions`).
    pub selected: usize,
    pub mode: Mode,
    pub input: String,
    pub action: Option<Action>,
    pub timeout_remaining: Duration,
    pub input_error: Option<String>,
    /// Cached preview text for the currently-selected session.
    pub preview: Option<String>,
    /// Session name the preview was last fetched for. None forces a refresh.
    pub preview_for: Option<String>,
    /// Filter substring (case-insensitive). Empty = match all.
    pub filter: String,
    /// Indices into `sessions` that match the current filter.
    pub filtered_indices: Vec<usize>,
    /// When true, the picker loop should re-fetch sessions before next render.
    pub sessions_dirty: bool,
    /// Selected session name to kill, set when Mode == ConfirmKill.
    pub kill_target: Option<String>,
    /// Set by `confirm_kill` for the picker loop to consume via
    /// `take_pending_kill`. Decouples input handling from tmux I/O.
    pub pending_kill: Option<String>,
    /// Auto-attach timeout in seconds. 0 disables auto-attach.
    pub timeout_secs: u64,
}

impl App {
    pub fn new(sessions: Vec<Session>, config: &Config) -> Self {
        let filtered_indices = (0..sessions.len()).collect();
        App {
            sessions,
            selected: 0,
            mode: Mode::Pick,
            input: String::new(),
            action: None,
            timeout_remaining: Duration::from_secs(config.timeout_secs),
            input_error: None,
            preview: None,
            preview_for: None,
            filter: String::new(),
            filtered_indices,
            sessions_dirty: false,
            kill_target: None,
            pending_kill: None,
            timeout_secs: config.timeout_secs,
        }
    }

    /// Returns the index into `sessions` for the currently-selected row in
    /// the visible (filtered) list, or None if the filtered list is empty.
    pub fn selected_session_index(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected).copied()
    }

    /// Returns the currently-selected session, if any.
    pub fn selected_session(&self) -> Option<&Session> {
        self.selected_session_index()
            .and_then(|i| self.sessions.get(i))
    }

    /// Returns the currently-selected session name, if any.
    pub fn selected_name(&self) -> Option<&str> {
        self.selected_session().map(|s| s.name.as_str())
    }

    /// Replace the session list (e.g., after kill). Resets filter index
    /// and tries to keep selection on the same session name if still present.
    pub fn replace_sessions(&mut self, sessions: Vec<Session>) {
        let prev_name = self.selected_name().map(String::from);
        self.sessions = sessions;
        self.recompute_filter();
        if let Some(name) = prev_name {
            if let Some(pos) = self
                .filtered_indices
                .iter()
                .position(|&i| self.sessions.get(i).is_some_and(|s| s.name == name))
            {
                self.selected = pos;
            } else {
                self.selected = 0;
            }
        } else {
            self.selected = 0;
        }
        self.sessions_dirty = false;
        self.preview = None;
        self.preview_for = None;
    }

    /// Recompute `filtered_indices` based on `filter`. Empty filter = all.
    pub fn recompute_filter(&mut self) {
        let f = self.filter.to_lowercase();
        if f.is_empty() {
            self.filtered_indices = (0..self.sessions.len()).collect();
            return;
        }
        self.filtered_indices = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| session_matches(s, &f))
            .map(|(i, _)| i)
            .collect();
    }

    /// True if the cached preview is for the currently-selected session.
    pub fn preview_is_current(&self) -> bool {
        match (self.selected_name(), self.preview_for.as_deref()) {
            (Some(sel), Some(cached)) => sel == cached,
            (None, None) => true,
            _ => false,
        }
    }

    /// Update the preview cache. Call from the picker loop after a successful
    /// `tmux::pane_capture`. Pass None for the text on capture failure to
    /// render an "(unavailable)" placeholder.
    pub fn set_preview(&mut self, text: Option<String>) {
        self.preview = text;
        self.preview_for = self.selected_name().map(String::from);
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.invalidate_preview_if_changed();
        self.reset_timeout();
    }

    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected < self.filtered_indices.len() - 1 {
            self.selected += 1;
        }
        self.invalidate_preview_if_changed();
        self.reset_timeout();
    }

    fn invalidate_preview_if_changed(&mut self) {
        if !self.preview_is_current() {
            self.preview = None;
        }
    }

    // -----------------------------------------------------------------------
    // Selection
    // -----------------------------------------------------------------------

    /// 1-indexed. If n is in range of the visible (filtered) list, set
    /// selected and confirm. Otherwise no-op.
    pub fn select_by_number(&mut self, n: usize) {
        if n == 0 || n > self.filtered_indices.len() {
            return;
        }
        self.selected = n - 1;
        self.confirm_selection();
    }

    pub fn confirm_selection(&mut self) {
        if let Some(session) = self.selected_session() {
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
    /// and we are in Pick mode, call auto_select. `timeout_secs == 0` disables
    /// auto-attach entirely.
    pub fn tick(&mut self, elapsed: Duration) {
        if self.mode != Mode::Pick || self.timeout_secs == 0 {
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
    /// the session list is empty. Operates against the full session list
    /// (auto-select bypasses any filter — auto-attach is for the no-input
    /// case where the user wouldn't have started typing a filter anyway).
    fn auto_select(&mut self) {
        if self.sessions.is_empty() {
            self.action = Some(Action::Shell);
            return;
        }

        // Prefer the first detached session.
        let pos = self.sessions.iter().position(|s| !s.attached).unwrap_or(0);

        if let Some(name) = self.sessions.get(pos).map(|s| s.name.clone()) {
            self.action = Some(Action::Attach(name));
        }
    }

    fn reset_timeout(&mut self) {
        self.timeout_remaining = Duration::from_secs(self.timeout_secs);
    }

    // -----------------------------------------------------------------------
    // Filter mode
    // -----------------------------------------------------------------------

    pub fn enter_filter_mode(&mut self) {
        self.mode = Mode::Filter;
        self.filter.clear();
        self.recompute_filter();
        self.selected = 0;
        self.preview = None;
        self.preview_for = None;
    }

    pub fn cancel_filter(&mut self) {
        self.mode = Mode::Pick;
        self.filter.clear();
        self.recompute_filter();
        self.selected = 0;
        self.preview = None;
        self.preview_for = None;
    }

    pub fn confirm_filter(&mut self) {
        // Keep the filter in effect but exit Filter mode so navigation works.
        self.mode = Mode::Pick;
        self.reset_timeout();
    }

    pub fn filter_char(&mut self, c: char) {
        self.filter.push(c);
        self.recompute_filter();
        // Reset selection to the first match when the filter changes.
        self.selected = 0;
        self.preview = None;
        self.preview_for = None;
    }

    pub fn filter_backspace(&mut self) {
        self.filter.pop();
        self.recompute_filter();
        self.selected = 0;
        self.preview = None;
        self.preview_for = None;
    }

    // -----------------------------------------------------------------------
    // Kill (with confirm)
    // -----------------------------------------------------------------------

    /// Enter ConfirmKill mode for the currently-selected session. No-op if
    /// the filtered list is empty.
    pub fn enter_kill_confirm(&mut self) {
        if let Some(name) = self.selected_name() {
            self.kill_target = Some(name.to_string());
            self.mode = Mode::ConfirmKill;
        }
    }

    pub fn cancel_kill(&mut self) {
        self.kill_target = None;
        self.mode = Mode::Pick;
    }

    /// User confirmed the kill. Move the target to `pending_kill` for the
    /// picker loop to pick up, return to Pick mode.
    pub fn confirm_kill(&mut self) {
        self.pending_kill = self.kill_target.take();
        self.mode = Mode::Pick;
    }

    /// Picker loop calls this each iteration. If a kill is pending, returns
    /// the session name and clears the slot.
    pub fn take_pending_kill(&mut self) -> Option<String> {
        let target = self.pending_kill.take();
        if target.is_some() {
            self.sessions_dirty = true;
        }
        target
    }
}

/// Case-insensitive substring match of `needle` against the session's
/// name, label (if any), and project basename (if any).
fn session_matches(session: &Session, needle_lower: &str) -> bool {
    if session.name.to_lowercase().contains(needle_lower) {
        return true;
    }
    if let Some(ref m) = session.metadata {
        if let Some(ref label) = m.label
            && label.to_lowercase().contains(needle_lower)
        {
            return true;
        }
        if let Some(ref project) = m.project
            && project.to_lowercase().contains(needle_lower)
        {
            return true;
        }
    }
    false
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
                metadata: None,
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
                metadata: None,
            },
            Session {
                name: "work".into(),
                window_count: 2,
                attached: false,
                current_command: "vim".into(),
                last_activity: Duration::from_secs(500),
                metadata: None,
            },
        ]
    }

    #[test]
    fn test_new_starts_at_zero() {
        let app = App::new(make_sessions(), &Config::default());
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_move_down() {
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
        app.select_by_number(5);
        assert!(app.action.is_none());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_by_number_zero() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.select_by_number(0);
        assert!(app.action.is_none());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_confirm_selection() {
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
        app.shell();
        assert!(matches!(app.action, Some(Action::Shell)));
        assert!(app.should_quit());
    }

    #[test]
    fn test_enter_new_mode() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.input.push_str("leftover");
        app.enter_new_mode();
        assert_eq!(app.mode, Mode::NewInput);
        assert!(app.input.is_empty());
        assert!(app.input_error.is_none());
    }

    #[test]
    fn test_cancel_input() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_new_mode();
        app.input_char('x');
        app.cancel_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.input.is_empty());
        assert!(app.input_error.is_none());
    }

    #[test]
    fn test_input_char_and_backspace() {
        let mut app = App::new(make_sessions(), &Config::default());
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
    fn test_input_clears_error() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_new_mode();
        app.input_error = Some("test error".into());
        // Typing a char should clear the error
        app.input_char('a');
        assert!(app.input_error.is_none());
        // Set error again, backspace should also clear it
        app.input_error = Some("test error".into());
        app.input_backspace();
        assert!(app.input_error.is_none());
    }

    #[test]
    fn test_confirm_empty_input_cancels() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_new_mode();
        app.confirm_input();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_none());
    }

    #[test]
    fn test_confirm_invalid_input() {
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
        app.tick(Duration::from_secs(10));
        assert!(app.action.is_some());
    }

    #[test]
    fn test_timeout_zero_disables_auto_attach() {
        // timeout_secs = 0 means never auto-attach. Even after a long elapsed,
        // app.action stays None.
        let cfg = Config {
            timeout_secs: 0,
            ..Config::default()
        };
        let mut app = App::new(make_sessions(), &cfg);
        app.tick(Duration::from_secs(3600));
        assert!(app.action.is_none());
    }

    #[test]
    fn test_custom_timeout_secs_applied() {
        let cfg = Config {
            timeout_secs: 3,
            ..Config::default()
        };
        let mut app = App::new(make_sessions(), &cfg);
        // 2s elapsed: 1s remaining, no action.
        app.tick(Duration::from_secs(2));
        assert!(app.action.is_none());
        // Total 4s elapsed: should fire auto-select.
        app.tick(Duration::from_secs(2));
        assert!(app.action.is_some());
    }

    #[test]
    fn test_timeout_picks_first_detached() {
        // make_sessions: index 0 = "main" (detached), index 1 = "claude-aihelp" (attached)
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
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
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_new_mode();
        app.tick(Duration::from_secs(15));
        assert!(app.action.is_none());
    }

    #[test]
    fn test_empty_sessions_auto_select_shell() {
        let mut app = App::new(vec![], &Config::default());
        app.tick(Duration::from_secs(10));
        assert!(matches!(app.action, Some(Action::Shell)));
    }

    // -----------------------------------------------------------------------
    // Preview cache
    // -----------------------------------------------------------------------

    #[test]
    fn preview_is_current_after_set() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_preview(Some("hello".into()));
        assert!(app.preview_is_current());
        assert_eq!(app.preview.as_deref(), Some("hello"));
    }

    #[test]
    fn preview_invalidated_on_navigation() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_preview(Some("first".into()));
        app.move_down();
        // After navigation, preview is invalidated (set to None) so the loop
        // refetches it.
        assert!(app.preview.is_none());
        assert!(!app.preview_is_current());
    }

    #[test]
    fn preview_persists_when_navigation_no_op() {
        // make_sessions returns 3; selected starts at 0. move_up at 0 is no-op.
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_preview(Some("kept".into()));
        app.move_up();
        assert_eq!(app.preview.as_deref(), Some("kept"));
    }

    // -----------------------------------------------------------------------
    // Filter mode
    // -----------------------------------------------------------------------

    #[test]
    fn enter_filter_clears_filter_and_recomputes() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        assert_eq!(app.mode, Mode::Filter);
        assert_eq!(app.filter, "");
        assert_eq!(app.filtered_indices, vec![0, 1, 2]);
    }

    #[test]
    fn filter_char_narrows_visible_list() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "claude".chars() {
            app.filter_char(c);
        }
        // make_sessions: index 1 = "claude-aihelp" — only one match
        assert_eq!(app.filtered_indices, vec![1]);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_name(), Some("claude-aihelp"));
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "CLAUDE".chars() {
            app.filter_char(c);
        }
        assert_eq!(app.filtered_indices.len(), 1);
    }

    #[test]
    fn filter_no_match_yields_empty_list() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "zzzzz".chars() {
            app.filter_char(c);
        }
        assert!(app.filtered_indices.is_empty());
        assert_eq!(app.selected_name(), None);
    }

    #[test]
    fn filter_backspace_widens() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "claude".chars() {
            app.filter_char(c);
        }
        assert_eq!(app.filtered_indices.len(), 1);
        app.filter_backspace();
        // "claud" still matches "claude-aihelp"
        assert_eq!(app.filtered_indices.len(), 1);
        for _ in 0..5 {
            app.filter_backspace();
        }
        // empty filter — all visible
        assert_eq!(app.filtered_indices.len(), 3);
    }

    #[test]
    fn cancel_filter_returns_to_pick_with_clean_state() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        app.filter_char('z');
        app.cancel_filter();
        assert_eq!(app.mode, Mode::Pick);
        assert_eq!(app.filter, "");
        assert_eq!(app.filtered_indices, vec![0, 1, 2]);
    }

    #[test]
    fn filter_navigation_uses_filtered_indices() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        // No filter: 3 visible. move_down twice goes to last.
        app.move_down();
        app.move_down();
        assert_eq!(app.selected, 2);
        // move_down again — clamped at last.
        app.move_down();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn filter_select_by_number_uses_filtered_list() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "claude".chars() {
            app.filter_char(c);
        }
        // Only one match. select_by_number(1) confirms it.
        app.select_by_number(1);
        if let Some(Action::Attach(name)) = &app.action {
            assert_eq!(name, "claude-aihelp");
        } else {
            panic!("expected Attach action");
        }
    }

    // -----------------------------------------------------------------------
    // Kill confirm
    // -----------------------------------------------------------------------

    #[test]
    fn enter_kill_confirm_sets_target() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_kill_confirm();
        assert_eq!(app.mode, Mode::ConfirmKill);
        assert_eq!(app.kill_target.as_deref(), Some("main"));
    }

    #[test]
    fn confirm_kill_moves_target_to_pending() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_kill_confirm();
        app.confirm_kill();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.kill_target.is_none());
        let target = app.take_pending_kill();
        assert_eq!(target.as_deref(), Some("main"));
        assert!(app.sessions_dirty);
    }

    #[test]
    fn cancel_kill_clears_target() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_kill_confirm();
        app.cancel_kill();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.kill_target.is_none());
        assert!(app.pending_kill.is_none());
    }

    #[test]
    fn enter_kill_with_empty_filtered_list_is_noop() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "zzzzz".chars() {
            app.filter_char(c);
        }
        app.enter_kill_confirm();
        // No selection → no mode transition.
        assert_eq!(app.mode, Mode::Filter);
        assert!(app.kill_target.is_none());
    }

    // -----------------------------------------------------------------------
    // Replace sessions (post-kill)
    // -----------------------------------------------------------------------

    #[test]
    fn replace_sessions_keeps_selection_on_same_name() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.move_down(); // selected = 1 ("claude-aihelp")
        let new = vec![
            Session {
                name: "main".into(),
                window_count: 1,
                attached: false,
                current_command: "bash".into(),
                last_activity: Duration::from_secs(0),
                metadata: None,
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
                metadata: None,
            },
        ];
        app.replace_sessions(new);
        assert_eq!(app.selected_name(), Some("claude-aihelp"));
    }

    #[test]
    fn replace_sessions_resets_selection_when_name_gone() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.move_down(); // selected = 1 ("claude-aihelp")
        let new = vec![Session {
            name: "main".into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: None,
        }];
        app.replace_sessions(new);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_name(), Some("main"));
    }
}
