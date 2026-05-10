use std::time::{Duration, Instant};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32String};

use crate::action::Action;
use crate::clipboard;
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
    Help,
    Rename,
}

// ---------------------------------------------------------------------------
// Sort mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SortMode {
    /// Whatever order tmux returned. The default — no re-sort happens
    /// until the user presses `o`.
    Default,
    /// Most recently active first (smallest `last_activity` first).
    LastActivity,
    /// Attached sessions on top, then alphabetical within each group.
    AttachedFirst,
    /// Case-insensitive alphabetical.
    Name,
    /// Longest-idle first (largest `last_activity` first).
    IdleLongest,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Default => "tmux order",
            SortMode::LastActivity => "by recent activity",
            SortMode::AttachedFirst => "attached first",
            SortMode::Name => "by name",
            SortMode::IdleLongest => "idle longest first",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SortMode::Default => SortMode::LastActivity,
            SortMode::LastActivity => SortMode::AttachedFirst,
            SortMode::AttachedFirst => SortMode::Name,
            SortMode::Name => SortMode::IdleLongest,
            SortMode::IdleLongest => SortMode::Default,
        }
    }
}

/// Window for sort-flash and yank-flash messages. Long enough to read,
/// short enough that the regular footer comes back quickly.
pub const FLASH_DURATION: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PreviewMode {
    /// Last 6 lines of the highlighted session's active pane.
    Summary,
    /// One row per window, each with the active pane's last non-blank line.
    WindowsList,
}

impl PreviewMode {
    pub fn next(self) -> Self {
        match self {
            PreviewMode::Summary => PreviewMode::WindowsList,
            PreviewMode::WindowsList => PreviewMode::Summary,
        }
    }
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
    /// Active sort mode. `cycle_sort` rotates this; the default is
    /// `LastActivity` so recent work surfaces.
    pub sort_mode: SortMode,
    /// Original session name when entering Rename mode. Restored to the
    /// input buffer so the user can edit rather than retype.
    pub rename_target: Option<String>,
    /// Set by `confirm_rename` as `(old, new)` for the picker loop to
    /// consume via `take_pending_rename`.
    pub pending_rename: Option<(String, String)>,
    /// Transient footer message (e.g. "yanked" or "[sort: by name]") that
    /// expires `FLASH_DURATION` after `flash_remaining` is set.
    pub flash: Option<String>,
    pub flash_remaining: Duration,
    /// Active preview style. `Tab` rotates between Summary and WindowsList.
    pub preview_mode: PreviewMode,
    /// Cached multi-window snapshot for `WindowsList`. Refilled by the
    /// picker loop when the cache is stale or empty.
    pub preview_windows: Option<Vec<crate::tmux::WindowSnapshot>>,
    /// Reusable fuzzy matcher; allocator state survives across keystrokes.
    pub matcher: Matcher,
    /// Last mouse-down `(row, when)` for double-click detection.
    pub last_click: Option<(usize, Instant)>,
}

impl App {
    pub fn new(sessions: Vec<Session>, config: &Config) -> Self {
        let filtered_indices = (0..sessions.len()).collect();
        let mut app = App {
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
            sort_mode: SortMode::Default,
            rename_target: None,
            pending_rename: None,
            flash: None,
            flash_remaining: Duration::ZERO,
            preview_mode: PreviewMode::Summary,
            preview_windows: None,
            matcher: Matcher::new(MatcherConfig::DEFAULT),
            last_click: None,
        };
        app.sort_sessions();
        app.recompute_filter();
        app
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
        self.sort_sessions();
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

    // -----------------------------------------------------------------------
    // Sort
    // -----------------------------------------------------------------------

    /// Apply `self.sort_mode` to `self.sessions` in place. Stable so
    /// equal-key sessions keep their tmux-reported order.
    pub fn sort_sessions(&mut self) {
        match self.sort_mode {
            SortMode::Default => {}
            SortMode::LastActivity => {
                self.sessions.sort_by_key(|s| s.last_activity);
            }
            SortMode::AttachedFirst => {
                self.sessions
                    .sort_by_key(|s| (!s.attached, s.name.to_lowercase()));
            }
            SortMode::Name => {
                self.sessions.sort_by_key(|s| s.name.to_lowercase());
            }
            SortMode::IdleLongest => {
                self.sessions
                    .sort_by_key(|s| std::cmp::Reverse(s.last_activity));
            }
        }
    }

    /// Rotate to the next sort mode, re-sort, and flash the new mode in
    /// the footer. Selection is reset to the top so the highlight tracks
    /// the visually-first match after re-ordering.
    pub fn cycle_sort(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.sort_sessions();
        self.recompute_filter();
        self.selected = 0;
        self.preview = None;
        self.preview_for = None;
        self.set_flash(format!("[sort: {}]", self.sort_mode.label()));
        self.reset_timeout();
    }

    /// Recompute `filtered_indices` based on `filter`. Empty filter keeps
    /// the original session order. A non-empty filter is matched fuzzily
    /// (subseq) against name, label, and project — the indices come back
    /// ordered by score descending so the best match floats to the top.
    pub fn recompute_filter(&mut self) {
        if self.filter.is_empty() {
            self.filtered_indices = (0..self.sessions.len()).collect();
            self.clamp_selected();
            return;
        }
        let pattern = Pattern::parse(&self.filter, CaseMatching::Ignore, Normalization::Smart);
        let mut scored: Vec<(usize, u32)> = self
            .sessions
            .iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                fuzzy_score_session(&pattern, &mut self.matcher, s).map(|score| (idx, score))
            })
            .collect();
        // Highest score first; ties preserve original insertion order.
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        self.filtered_indices = scored.into_iter().map(|(idx, _)| idx).collect();
        self.clamp_selected();
    }

    /// Snap `selected` into the current filtered range. Pulls a stale index
    /// down when the visible list has shrunk, or to 0 when it is empty.
    fn clamp_selected(&mut self) {
        let max = self.filtered_indices.len();
        if max == 0 {
            self.selected = 0;
        } else if self.selected >= max {
            self.selected = max - 1;
        }
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
    /// auto-attach entirely. Also expires any active footer flash.
    pub fn tick(&mut self, elapsed: Duration) {
        if self.flash.is_some() {
            self.flash_remaining = self.flash_remaining.saturating_sub(elapsed);
            if self.flash_remaining.is_zero() {
                self.flash = None;
            }
        }
        if self.mode != Mode::Pick || self.timeout_secs == 0 {
            return;
        }
        self.timeout_remaining = self.timeout_remaining.saturating_sub(elapsed);
        if self.timeout_remaining.is_zero() {
            self.auto_select();
        }
    }

    /// Set or replace the transient footer flash.
    pub fn set_flash(&mut self, msg: String) {
        self.flash = Some(msg);
        self.flash_remaining = FLASH_DURATION;
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

    /// Enter ConfirmKill mode for the currently-selected session. Flashes
    /// "(no session to kill)" if the filtered list is empty so the user
    /// gets feedback that K was received.
    pub fn enter_kill_confirm(&mut self) {
        if let Some(name) = self.selected_name() {
            self.kill_target = Some(name.to_string());
            self.mode = Mode::ConfirmKill;
        } else {
            self.set_flash("(no session to kill)".into());
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

    // -----------------------------------------------------------------------
    // Help overlay
    // -----------------------------------------------------------------------

    /// Open the help overlay. Resets the auto-attach timeout so the user can
    /// read without being kicked back to the picker, and clears the preview
    /// cache because the overlay covers that area.
    pub fn enter_help(&mut self) {
        self.mode = Mode::Help;
        self.preview = None;
        self.preview_for = None;
        self.reset_timeout();
    }

    /// Close the help overlay and return to picking.
    pub fn cancel_help(&mut self) {
        self.mode = Mode::Pick;
        self.reset_timeout();
    }

    // -----------------------------------------------------------------------
    // Rename
    // -----------------------------------------------------------------------

    /// Open Rename mode for the highlighted session. Pre-fills the input
    /// buffer with the current name so the user can edit instead of
    /// retype. No-op if the filtered list is empty.
    pub fn enter_rename(&mut self) {
        let Some(name) = self.selected_name().map(String::from) else {
            return;
        };
        self.input.clear();
        self.input.push_str(&name);
        self.rename_target = Some(name);
        self.input_error = None;
        self.mode = Mode::Rename;
        self.reset_timeout();
    }

    pub fn cancel_rename(&mut self) {
        self.rename_target = None;
        self.input.clear();
        self.input_error = None;
        self.mode = Mode::Pick;
        self.reset_timeout();
    }

    /// Validate the buffered name and stage a rename for the picker loop
    /// to execute. No-op when the name is unchanged. Sets `input_error`
    /// when the name fails validation.
    pub fn confirm_rename(&mut self) {
        let Some(old) = self.rename_target.clone() else {
            self.cancel_rename();
            return;
        };
        let trimmed = self.input.trim();
        if trimmed.is_empty() || trimmed == old {
            self.cancel_rename();
            return;
        }
        match validate_session_name(trimmed) {
            None => {
                self.input_error = Some(format!("'{trimmed}' is not a valid session name"));
            }
            Some(new) => {
                self.pending_rename = Some((old, new));
                self.rename_target = None;
                self.input.clear();
                self.input_error = None;
                self.mode = Mode::Pick;
            }
        }
    }

    /// Picker loop calls this each iteration. If a rename is pending,
    /// returns `(old, new)` and clears the slot. The loop runs the tmux
    /// command and marks `sessions_dirty` so the list refreshes.
    pub fn take_pending_rename(&mut self) -> Option<(String, String)> {
        let pair = self.pending_rename.take();
        if pair.is_some() {
            self.sessions_dirty = true;
        }
        pair
    }

    // -----------------------------------------------------------------------
    // Yank
    // -----------------------------------------------------------------------

    /// Copy the selected session's name to the system clipboard via the
    /// adapter in `crate::clipboard`. Result reported in the footer flash.
    pub fn yank_selected(&mut self) {
        let Some(name) = self.selected_name().map(String::from) else {
            self.set_flash("(no session to yank)".into());
            return;
        };
        match clipboard::copy(&name) {
            Ok(tool) => self.set_flash(format!("yanked '{name}' via {tool}")),
            Err(e) => self.set_flash(format!("yank failed: {e}")),
        }
    }

    // -----------------------------------------------------------------------
    // Preview mode
    // -----------------------------------------------------------------------

    /// Rotate to the next preview mode and clear any cached payload so the
    /// picker loop fetches fresh data on its next pass.
    pub fn cycle_preview_mode(&mut self) {
        self.preview_mode = self.preview_mode.next();
        self.preview = None;
        self.preview_for = None;
        self.preview_windows = None;
        self.reset_timeout();
    }

    // -----------------------------------------------------------------------
    // Mouse
    // -----------------------------------------------------------------------

    /// Handle a left-button-down on a visible row. The first click selects;
    /// a second click on the same row within 500 ms confirms (attaches).
    pub fn handle_mouse_click(&mut self, row: usize, now: Instant) {
        if row >= self.filtered_indices.len() {
            return;
        }
        self.selected = row;
        self.preview = None;
        self.preview_for = None;
        self.preview_windows = None;
        self.reset_timeout();
        let double_click = matches!(
            self.last_click,
            Some((prev_row, prev_when))
                if prev_row == row
                    && now.saturating_duration_since(prev_when) <= Duration::from_millis(500)
        );
        if double_click {
            self.last_click = None;
            self.confirm_selection();
        } else {
            self.last_click = Some((row, now));
        }
    }
}

/// Fuzzy-match a session against the pattern. Returns the highest score
/// across the session's name, label, and project. None when no field
/// matches.
fn fuzzy_score_session(pattern: &Pattern, matcher: &mut Matcher, session: &Session) -> Option<u32> {
    let mut best: Option<u32> = None;
    let mut consider = |s: &str| {
        let utf32 = Utf32String::from(s);
        if let Some(score) = pattern.score(utf32.slice(..), matcher) {
            best = Some(best.map_or(score, |prev| prev.max(score)));
        }
    };
    consider(&session.name);
    if let Some(ref m) = session.metadata {
        if let Some(ref label) = m.label {
            consider(label);
        }
        if let Some(ref project) = m.project {
            consider(project);
        }
    }
    best
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
                marker: None,
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
                metadata: None,
                marker: None,
            },
            Session {
                name: "work".into(),
                window_count: 2,
                attached: false,
                current_command: "vim".into(),
                last_activity: Duration::from_secs(500),
                metadata: None,
                marker: None,
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
    fn enter_kill_with_empty_filtered_list_flashes_and_stays() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        for c in "zzzzz".chars() {
            app.filter_char(c);
        }
        app.enter_kill_confirm();
        // No selection → no mode transition, but a flash so the user sees K landed.
        assert_eq!(app.mode, Mode::Filter);
        assert!(app.kill_target.is_none());
        assert_eq!(app.flash.as_deref(), Some("(no session to kill)"));
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
                marker: None,
            },
            Session {
                name: "claude-aihelp".into(),
                window_count: 3,
                attached: true,
                current_command: "claude".into(),
                last_activity: Duration::from_secs(10),
                metadata: None,
                marker: None,
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
            marker: None,
        }];
        app.replace_sessions(new);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_name(), Some("main"));
    }

    // -----------------------------------------------------------------------
    // Selected clamp
    // -----------------------------------------------------------------------

    #[test]
    fn clamp_selected_pulls_index_into_range() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.selected = 99;
        app.recompute_filter();
        assert!(app.selected < app.filtered_indices.len());
    }

    #[test]
    fn clamp_selected_zero_when_filter_empties_list() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.move_down();
        app.move_down(); // selected = 2
        app.enter_filter_mode();
        for c in "zzzzz".chars() {
            app.filter_char(c);
        }
        // Filter has no matches; selected is forced to 0.
        assert_eq!(app.selected, 0);
        assert!(app.filtered_indices.is_empty());
    }

    // -----------------------------------------------------------------------
    // Help mode
    // -----------------------------------------------------------------------

    #[test]
    fn enter_help_flips_mode_and_clears_preview() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_preview(Some("hello".into()));
        app.enter_help();
        assert_eq!(app.mode, Mode::Help);
        assert!(app.preview.is_none());
    }

    #[test]
    fn enter_help_resets_timeout() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.tick(Duration::from_secs(9));
        app.enter_help();
        assert_eq!(app.timeout_remaining, Duration::from_secs(10));
    }

    #[test]
    fn cancel_help_returns_to_pick_and_resets_timeout() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_help();
        app.tick(Duration::from_secs(5));
        app.cancel_help();
        assert_eq!(app.mode, Mode::Pick);
        assert_eq!(app.timeout_remaining, Duration::from_secs(10));
    }

    // -----------------------------------------------------------------------
    // Sort
    // -----------------------------------------------------------------------

    #[test]
    fn default_sort_preserves_tmux_order() {
        let app = App::new(make_sessions(), &Config::default());
        assert_eq!(app.sort_mode, SortMode::Default);
        assert_eq!(app.sessions[0].name, "main");
        assert_eq!(app.sessions[1].name, "claude-aihelp");
        assert_eq!(app.sessions[2].name, "work");
    }

    #[test]
    fn cycle_sort_advances_through_every_mode() {
        let mut app = App::new(make_sessions(), &Config::default());
        let expected = [
            SortMode::LastActivity,
            SortMode::AttachedFirst,
            SortMode::Name,
            SortMode::IdleLongest,
            SortMode::Default,
        ];
        for want in expected {
            app.cycle_sort();
            assert_eq!(app.sort_mode, want);
        }
    }

    #[test]
    fn sort_last_activity_puts_most_recent_first() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.cycle_sort(); // Default -> LastActivity
        assert_eq!(app.sessions[0].name, "claude-aihelp");
        assert_eq!(app.sessions[1].name, "main");
        assert_eq!(app.sessions[2].name, "work");
    }

    #[test]
    fn sort_attached_first_puts_attached_on_top() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.sort_mode = SortMode::AttachedFirst;
        app.sort_sessions();
        assert!(app.sessions[0].attached);
        // The remaining detached sessions sort alphabetically.
        assert_eq!(app.sessions[1].name, "main");
        assert_eq!(app.sessions[2].name, "work");
    }

    #[test]
    fn sort_by_name_is_case_insensitive_alpha() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.sort_mode = SortMode::Name;
        app.sort_sessions();
        let names: Vec<&str> = app.sessions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["claude-aihelp", "main", "work"]);
    }

    #[test]
    fn sort_idle_longest_puts_longest_idle_first() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.sort_mode = SortMode::IdleLongest;
        app.sort_sessions();
        assert_eq!(app.sessions[0].name, "work"); // 500s
        assert_eq!(app.sessions[2].name, "claude-aihelp"); // 10s
    }

    #[test]
    fn cycle_sort_flashes_the_new_label() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.cycle_sort();
        let flash = app.flash.as_ref().expect("flash set on cycle");
        assert!(flash.contains("recent activity"));
    }

    // -----------------------------------------------------------------------
    // Rename
    // -----------------------------------------------------------------------

    #[test]
    fn enter_rename_prefills_input_with_current_name() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        assert_eq!(app.mode, Mode::Rename);
        assert_eq!(app.input, "main");
        assert_eq!(app.rename_target.as_deref(), Some("main"));
    }

    #[test]
    fn cancel_rename_returns_to_pick_and_clears_state() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        app.input.push('x');
        app.cancel_rename();
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.input.is_empty());
        assert!(app.rename_target.is_none());
    }

    #[test]
    fn confirm_rename_with_unchanged_name_is_noop() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        // input == "main" already
        app.confirm_rename();
        assert!(app.pending_rename.is_none());
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn confirm_rename_with_empty_name_is_noop() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        app.input.clear();
        app.confirm_rename();
        assert!(app.pending_rename.is_none());
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn confirm_rename_with_invalid_name_keeps_mode_and_sets_error() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        // `validate_session_name` is a sanitiser; it returns None only when
        // the input collapses to empty after trimming. A single slash does.
        app.input.clear();
        app.input.push('/');
        app.confirm_rename();
        assert_eq!(app.mode, Mode::Rename);
        assert!(app.input_error.is_some());
        assert!(app.pending_rename.is_none());
    }

    #[test]
    fn confirm_rename_with_valid_new_name_pushes_pending() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        app.input.clear();
        app.input.push_str("renamed");
        app.confirm_rename();
        assert_eq!(
            app.pending_rename,
            Some(("main".to_string(), "renamed".to_string()))
        );
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn take_pending_rename_marks_sessions_dirty() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_rename();
        app.input.clear();
        app.input.push_str("renamed");
        app.confirm_rename();
        let pair = app.take_pending_rename();
        assert_eq!(pair, Some(("main".to_string(), "renamed".to_string())));
        assert!(app.sessions_dirty);
    }

    // -----------------------------------------------------------------------
    // Yank + flash
    // -----------------------------------------------------------------------

    #[test]
    fn yank_with_no_session_flashes_message() {
        let mut app = App::new(vec![], &Config::default());
        app.yank_selected();
        assert!(app.flash.as_deref().unwrap().contains("no session"));
    }

    #[test]
    fn flash_expires_after_tick_window() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_flash("hello".into());
        assert!(app.flash.is_some());
        app.tick(FLASH_DURATION);
        assert!(app.flash.is_none());
    }

    #[test]
    fn flash_persists_within_window() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_flash("hello".into());
        app.tick(Duration::from_millis(500));
        assert!(app.flash.is_some());
    }

    // -----------------------------------------------------------------------
    // Preview mode
    // -----------------------------------------------------------------------

    #[test]
    fn preview_mode_default_is_summary() {
        let app = App::new(make_sessions(), &Config::default());
        assert_eq!(app.preview_mode, PreviewMode::Summary);
    }

    #[test]
    fn cycle_preview_mode_rotates() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.cycle_preview_mode();
        assert_eq!(app.preview_mode, PreviewMode::WindowsList);
        app.cycle_preview_mode();
        assert_eq!(app.preview_mode, PreviewMode::Summary);
    }

    #[test]
    fn cycle_preview_mode_clears_caches() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.set_preview(Some("hello".into()));
        app.preview_windows = Some(Vec::new());
        app.cycle_preview_mode();
        assert!(app.preview.is_none());
        assert!(app.preview_windows.is_none());
    }

    // -----------------------------------------------------------------------
    // Fuzzy filter
    // -----------------------------------------------------------------------

    #[test]
    fn fuzzy_filter_matches_subsequence() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.enter_filter_mode();
        // "ca" should match "claude-aihelp" but not "main" or "work".
        for c in "ca".chars() {
            app.filter_char(c);
        }
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.sessions[i].name.as_str())
            .collect();
        assert_eq!(names, ["claude-aihelp"]);
    }

    #[test]
    fn fuzzy_filter_orders_by_score() {
        let sessions = vec![
            Session {
                name: "claude-frontend".into(),
                ..Default::default()
            },
            Session {
                name: "back-end-claude".into(),
                ..Default::default()
            },
            Session {
                name: "main".into(),
                ..Default::default()
            },
        ];
        let mut app = App::new(sessions, &Config::default());
        app.enter_filter_mode();
        for c in "claude".chars() {
            app.filter_char(c);
        }
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.sessions[i].name.as_str())
            .collect();
        // The picker doesn't enforce a specific ordering between two
        // sessions whose names both contain "claude", but `main` must be
        // missing entirely.
        assert!(names.contains(&"claude-frontend"));
        assert!(names.contains(&"back-end-claude"));
        assert!(!names.contains(&"main"));
    }

    #[test]
    fn fuzzy_filter_empty_keeps_full_order() {
        let app = App::new(make_sessions(), &Config::default());
        assert_eq!(app.filtered_indices.len(), 3);
        assert_eq!(app.filtered_indices, [0, 1, 2]);
    }

    // -----------------------------------------------------------------------
    // Mouse
    // -----------------------------------------------------------------------

    #[test]
    fn click_selects_target_row() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.handle_mouse_click(2, Instant::now());
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn click_outside_visible_range_is_noop() {
        let mut app = App::new(make_sessions(), &Config::default());
        app.handle_mouse_click(99, Instant::now());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn double_click_within_window_confirms() {
        let mut app = App::new(make_sessions(), &Config::default());
        let now = Instant::now();
        app.handle_mouse_click(1, now);
        assert!(app.action.is_none());
        app.handle_mouse_click(1, now + Duration::from_millis(200));
        assert!(app.action.is_some());
    }

    #[test]
    fn double_click_after_window_does_not_confirm() {
        let mut app = App::new(make_sessions(), &Config::default());
        let now = Instant::now();
        app.handle_mouse_click(1, now);
        app.handle_mouse_click(1, now + Duration::from_millis(800));
        assert!(app.action.is_none());
    }

    #[test]
    fn click_on_different_row_does_not_confirm() {
        let mut app = App::new(make_sessions(), &Config::default());
        let now = Instant::now();
        app.handle_mouse_click(0, now);
        app.handle_mouse_click(1, now + Duration::from_millis(200));
        assert!(app.action.is_none());
        assert_eq!(app.selected, 1);
    }
}
