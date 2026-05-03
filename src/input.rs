use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Mode};

/// Top-level dispatcher: route to the correct mode handler.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        Mode::Pick => handle_pick_key(app, key),
        Mode::NewInput => handle_input_key(app, key),
        Mode::Filter => handle_filter_key(app, key),
        Mode::ConfirmKill => handle_confirm_kill_key(app, key),
        Mode::Help => handle_help_key(app, key),
        Mode::Rename => handle_rename_key(app, key),
    }
}

/// Key handler for Pick mode.
pub fn handle_pick_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Ctrl+C → shell
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.shell(),

        // Navigation
        KeyCode::Up => app.move_up(),
        KeyCode::Char('k') => app.move_up(),
        KeyCode::Down => app.move_down(),
        KeyCode::Char('j') => app.move_down(),

        // Confirm
        KeyCode::Enter => app.confirm_selection(),

        // Actions
        KeyCode::Char('n') => app.enter_new_mode(),
        KeyCode::Char('s') => app.shell(),
        KeyCode::Char('q') => app.shell(),
        KeyCode::Esc => app.shell(),

        // Power ops
        KeyCode::Char('/') => app.enter_filter_mode(),
        KeyCode::Char('K') => app.enter_kill_confirm(),
        KeyCode::Char('r') => app.enter_rename(),
        KeyCode::Char('o') => app.cycle_sort(),
        KeyCode::Char('y') => app.yank_selected(),

        // Help overlay
        KeyCode::Char('?') => app.enter_help(),

        // 1-indexed digit selection
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let digit = c as usize - '0' as usize;
            app.select_by_number(digit);
        }

        // Anything else is ignored.
        _ => {}
    }
}

/// Key handler for Help mode. `?`, `Esc`, or `q` closes the overlay; every
/// other key is a no-op so the user can read in peace.
pub fn handle_help_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.cancel_help(),
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => app.cancel_help(),
        _ => {}
    }
}

/// Key handler for Rename mode. Mirrors `handle_input_key` but routes
/// confirm to `confirm_rename`.
pub fn handle_rename_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.cancel_rename(),
        KeyCode::Enter => app.confirm_rename(),
        KeyCode::Esc => app.cancel_rename(),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Char(c) => app.input_char(c),
        _ => {}
    }
}

/// Key handler for Filter mode.
pub fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.cancel_filter(),
        KeyCode::Esc => app.cancel_filter(),
        // Enter exits filter mode keeping the current filter and selection,
        // then attaches to that selected session.
        KeyCode::Enter => {
            app.confirm_filter();
            app.confirm_selection();
        }
        KeyCode::Up => app.move_up(),
        KeyCode::Down => app.move_down(),
        KeyCode::Backspace => app.filter_backspace(),
        KeyCode::Char(c) => app.filter_char(c),
        _ => {}
    }
}

/// Key handler for ConfirmKill mode. `y`/`Y` confirms; anything else cancels.
pub fn handle_confirm_kill_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_kill(),
        _ => app.cancel_kill(),
    }
}

/// Key handler for NewInput mode.
pub fn handle_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Ctrl+C → cancel
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.cancel_input(),

        KeyCode::Enter => app.confirm_input(),
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Char(c) => app.input_char(c),

        // Anything else is ignored.
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::action::Action;
    use crate::app::Mode;
    use crate::config::Config;
    use crate::session::Session;

    fn make_app() -> App {
        App::new(
            vec![
                Session {
                    name: "main".into(),
                    window_count: 1,
                    attached: false,
                    current_command: "bash".into(),
                    last_activity: Duration::from_secs(0),
                    metadata: None,
                },
                Session {
                    name: "work".into(),
                    window_count: 2,
                    attached: false,
                    current_command: "vim".into(),
                    last_activity: Duration::from_secs(100),
                    metadata: None,
                },
            ],
            &Config::default(),
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // -----------------------------------------------------------------------
    // Pick mode — navigation
    // -----------------------------------------------------------------------

    #[test]
    fn test_arrow_down() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_j_moves_down() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_arrow_up() {
        let mut app = make_app();
        app.selected = 1;
        handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_k_moves_up() {
        let mut app = make_app();
        app.selected = 1;
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected, 0);
    }

    // -----------------------------------------------------------------------
    // Pick mode — selection
    // -----------------------------------------------------------------------

    #[test]
    fn test_enter_selects() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Enter));
        assert!(matches!(&app.action, Some(Action::Attach(n)) if n == "main"));
    }

    #[test]
    fn test_number_selects() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('2')));
        assert!(matches!(&app.action, Some(Action::Attach(n)) if n == "work"));
    }

    // -----------------------------------------------------------------------
    // Pick mode — shell exits
    // -----------------------------------------------------------------------

    #[test]
    fn test_s_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('s')));
        assert!(matches!(app.action, Some(Action::Shell)));
    }

    #[test]
    fn test_q_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('q')));
        assert!(matches!(app.action, Some(Action::Shell)));
    }

    #[test]
    fn test_esc_shells() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(matches!(app.action, Some(Action::Shell)));
    }

    #[test]
    fn test_ctrl_c_shells() {
        let mut app = make_app();
        handle_key(&mut app, ctrl('c'));
        assert!(matches!(app.action, Some(Action::Shell)));
    }

    // -----------------------------------------------------------------------
    // Pick mode → NewInput transition
    // -----------------------------------------------------------------------

    #[test]
    fn test_n_enters_new_mode() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, Mode::NewInput);
    }

    // -----------------------------------------------------------------------
    // NewInput mode — typing
    // -----------------------------------------------------------------------

    #[test]
    fn test_input_mode_typing() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('a')));
        handle_key(&mut app, key(KeyCode::Char('b')));
        assert_eq!(app.input, "ab");
    }

    #[test]
    fn test_input_mode_backspace() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('a')));
        handle_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.input, "");
    }

    #[test]
    fn test_input_mode_esc_cancels() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, key(KeyCode::Char('x')));
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn test_input_mode_ctrl_c_cancels() {
        let mut app = make_app();
        app.enter_new_mode();
        handle_key(&mut app, ctrl('c'));
        assert_eq!(app.mode, Mode::Pick);
    }

    // -----------------------------------------------------------------------
    // Unknown key is ignored
    // -----------------------------------------------------------------------

    #[test]
    fn test_unknown_key_ignored() {
        let mut app = make_app();
        let before_selected = app.selected;
        handle_key(&mut app, key(KeyCode::F(1)));
        assert_eq!(app.selected, before_selected);
        assert!(app.action.is_none());
        assert_eq!(app.mode, Mode::Pick);
    }

    // -----------------------------------------------------------------------
    // Filter + Kill mode key dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn slash_enters_filter_mode() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('/')));
        assert_eq!(app.mode, Mode::Filter);
    }

    #[test]
    fn capital_k_enters_kill_confirm() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('K')));
        assert_eq!(app.mode, Mode::ConfirmKill);
        assert!(app.kill_target.is_some());
    }

    #[test]
    fn lowercase_k_still_navigates_up() {
        let mut app = make_app();
        app.selected = 1;
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn filter_mode_typing_appends_to_filter() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('/')));
        handle_key(&mut app, key(KeyCode::Char('w')));
        handle_key(&mut app, key(KeyCode::Char('o')));
        assert_eq!(app.filter, "wo");
    }

    #[test]
    fn filter_mode_esc_cancels_back_to_pick() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('/')));
        handle_key(&mut app, key(KeyCode::Char('w')));
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Pick);
        assert_eq!(app.filter, "");
    }

    #[test]
    fn filter_mode_enter_confirms_and_attaches() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('/')));
        // Filter is empty so all visible; enter attaches to selected.
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.action.is_some());
    }

    #[test]
    fn confirm_kill_y_pushes_pending() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('K')));
        handle_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.pending_kill.is_some());
    }

    #[test]
    fn confirm_kill_n_cancels() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('K')));
        handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.pending_kill.is_none());
        assert!(app.kill_target.is_none());
    }

    // -----------------------------------------------------------------------
    // Help mode
    // -----------------------------------------------------------------------

    #[test]
    fn question_mark_enters_help_mode() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('?')));
        assert_eq!(app.mode, Mode::Help);
    }

    #[test]
    fn esc_closes_help() {
        let mut app = make_app();
        app.enter_help();
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn question_mark_closes_help() {
        let mut app = make_app();
        app.enter_help();
        handle_key(&mut app, key(KeyCode::Char('?')));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn q_closes_help() {
        let mut app = make_app();
        app.enter_help();
        handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn ctrl_c_closes_help() {
        let mut app = make_app();
        app.enter_help();
        handle_key(&mut app, ctrl('c'));
        assert_eq!(app.mode, Mode::Pick);
    }

    #[test]
    fn other_keys_in_help_are_noop() {
        let mut app = make_app();
        app.enter_help();
        handle_key(&mut app, key(KeyCode::Char('j')));
        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.mode, Mode::Help);
    }

    // -----------------------------------------------------------------------
    // Quick ops dispatch (phase 6)
    // -----------------------------------------------------------------------

    #[test]
    fn r_enters_rename_mode() {
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('r')));
        assert_eq!(app.mode, Mode::Rename);
        assert_eq!(app.input, "main");
    }

    #[test]
    fn o_advances_sort_mode_and_flashes() {
        use crate::app::SortMode;
        let mut app = make_app();
        handle_key(&mut app, key(KeyCode::Char('o')));
        assert_eq!(app.sort_mode, SortMode::LastActivity);
        assert!(app.flash.is_some());
    }

    #[test]
    fn y_triggers_yank_and_sets_flash() {
        let mut app = make_app();
        // No clipboard tool likely available in CI — accept either an
        // "ok" flash or a "no clipboard" flash, but it must be set.
        handle_key(&mut app, key(KeyCode::Char('y')));
        assert!(app.flash.is_some());
    }

    #[test]
    fn rename_typing_appends() {
        let mut app = make_app();
        app.enter_rename();
        // input starts as "main"; backspace then type
        for _ in 0..4 {
            handle_key(&mut app, key(KeyCode::Backspace));
        }
        for c in "renamed".chars() {
            handle_key(&mut app, key(KeyCode::Char(c)));
        }
        assert_eq!(app.input, "renamed");
    }

    #[test]
    fn rename_enter_confirms() {
        let mut app = make_app();
        app.enter_rename();
        for _ in 0..4 {
            handle_key(&mut app, key(KeyCode::Backspace));
        }
        for c in "renamed".chars() {
            handle_key(&mut app, key(KeyCode::Char(c)));
        }
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.pending_rename.is_some());
    }

    #[test]
    fn rename_esc_cancels() {
        let mut app = make_app();
        app.enter_rename();
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Pick);
        assert!(app.pending_rename.is_none());
    }
}
