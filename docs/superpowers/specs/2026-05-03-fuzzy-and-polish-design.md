# tmux-picker: Fuzzy filter + polish

**Date:** 2026-05-03
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #8 of 9

## Problem

The picker is functional and discoverable; what remains before
distribution is the small set of polish items that make day-to-day use
feel like a real tool:

1. **Substring filter is too rigid.** Typing `acme` to find
   `claude-acme-fronted` is fine; typing `cf` to find the same session
   is not. Fuzzy match on subsequences would match more naturally.
2. **No mouse support.** Users on a fresh laptop or a graphical terminal
   typically expect to click a row. Right now the only inputs are arrow
   keys and digits.
3. **No live config reload.** Changing `~/.config/tmux-picker/config.toml`
   today requires killing the current picker session. SIGHUP is the
   conventional Linux signal for "reread your config."

## Solution

Three changes, each one self-contained:

1. **Fuzzy filter via `nucleo-matcher` (~150KB).** Replace the
   `to_lowercase().contains()` matcher in `session_matches` with
   `nucleo_matcher::Matcher` plus a `Pattern` parsed from
   `app.filter`. Empty filter still bypasses entirely. Filtered
   sessions are ordered by score descending (was: insertion order),
   so the best match floats to the top regardless of its position in
   the unfiltered list.
2. **Mouse: click-to-select, double-click to attach.** Enable mouse
   capture in `main.rs` (the `EnableMouseCapture` event from
   `crossterm`). The picker loop converts a single mouse-down on a
   visible row into `app.select_row(idx)`; a second click on the same
   row within 500 ms calls `app.confirm_selection()`.
3. **SIGHUP reload.** Install a signal handler that toggles an atomic
   `bool RELOAD_REQUESTED`. The picker loop polls it on each tick,
   re-reads the config, applies new theme/markers/timeout, and flashes
   `[config reloaded]`. Sessions are not re-fetched (kill / rename
   already covers that path).

Out of scope (defer):

- Drag-to-resize panes. That's not what tmux-picker does.
- File-watch reload via `inotify`. SIGHUP is enough for now and one
  fewer dep.

## Architecture

### Fuzzy match

Add to `Cargo.toml`:

```toml
nucleo-matcher = "0.3"
```

(`nucleo-matcher` is the standalone matcher, no async, ~150 KB.)

`session_matches` becomes a wrapper around `Matcher::fuzzy_match`:

```rust
fn session_matches(
    matcher: &mut Matcher,
    pattern: &Pattern,
    session: &Session,
) -> Option<u32> {
    let haystacks = vec![
        session.name.as_str(),
        session.metadata.as_ref().and_then(|m| m.label.as_deref()).unwrap_or(""),
        session.metadata.as_ref().and_then(|m| m.project.as_deref()).unwrap_or(""),
    ];
    haystacks.iter().filter_map(|h| {
        let utf32 = nucleo_matcher::Utf32String::from(*h);
        pattern.score(utf32.slice(..), matcher)
    }).max()
}
```

`recompute_filter` builds the `Pattern` from `app.filter` (with
`CaseMatching::Ignore`, `Normalization::Smart`) and ranks
`filtered_indices` by score descending. Empty filter still produces
the full list in insertion order — the early return preserves the
original cheap path.

The `Matcher` lives on `App` so its allocator survives across
keystrokes. `Pattern` is rebuilt every call (cheap).

### Mouse

Enable / disable capture in the `picker_loop` `TerminalGuard`. crossterm
exports `EnableMouseCapture` / `DisableMouseCapture`. `event::poll`
already returns `Event::Mouse(_)`.

A new `App::handle_mouse_click(&mut self, row: usize, now: Instant)`
absorbs the click logic. The loop converts the screen-row in
`MouseEvent.row` to a session row by subtracting the table border + the
window title row. The math lives in `ui.rs::row_for_y` so the layout
constants stay close to the layout itself.

Double-click detection: store `last_click: Option<(usize, Instant)>` on
`App`. Two clicks on the same `idx` within `Duration::from_millis(500)`
trigger `confirm_selection`.

Mouse events are routed only in `Mode::Pick` and `Mode::Filter`; the
NewInput / Rename / Help / ConfirmKill modes ignore them.

### SIGHUP

Use `signal-hook` (stable, no async runtime).

```toml
signal-hook = "0.3"
```

```rust
use std::sync::atomic::{AtomicBool, Ordering};
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

fn install_sighup_handler() -> Result<(), std::io::Error> {
    signal_hook::flag::register(
        signal_hook::consts::SIGHUP,
        std::sync::Arc::new(AtomicBool::new(false))
            .clone(), // shared flag
    )?;
    Ok(())
}
```

Concretely the picker loop holds an `Arc<AtomicBool>` that
`signal_hook::flag::register` mutates. Each tick checks the flag,
swaps it back to false, and reloads:

```rust
if reload_flag.swap(false, Ordering::SeqCst) {
    let new_cfg = Config::load();
    app.timeout_secs = new_cfg.timeout_secs;
    *config_mut = new_cfg;
    app.set_flash("[config reloaded]".into());
}
```

`UiContext` borrows the (now mutable) config, so the loop holds the
config in a `Box<Config>` and re-creates the borrow each render. Or we
just store the theme on `UiContext` by value and reconstruct each tick.
Pragmatic choice: copy `Theme` into `UiContext` per render so the
borrow is local.

## Edge cases

| Case | Behavior |
|---|---|
| Filter typed extremely fast | `Pattern::parse` is cheap; matcher state machine handles per-char calls. No throttling needed. |
| Click outside the rows area | No-op. |
| Double-click on a different row | First click selects, second click attaches that other row. |
| SIGHUP arrives during render | Flag is checked on the next tick — at most one frame delay. |
| Reloaded config has a malformed entry | Existing warning path applies; effective config falls back to defaults; flash reports `[config reloaded with N warnings]`. |
| `nucleo_matcher` produces no matches | Treat as empty filtered list (existing behaviour). |

## Tests

Unit (`src/app.rs`):
- `recompute_filter` with fuzzy `cf` matches `claude-frontend` (subseq).
- `recompute_filter` orders by score descending (best first).
- Empty filter still produces 0..N indices in original order.
- `handle_mouse_click` selects the matching row.
- Two clicks on the same row within 500ms confirm.
- Two clicks on different rows do not confirm.

Unit (`src/main.rs` flag plumbing only):
- Setting the reload flag and calling the reload helper updates the
  config in-place and produces a flash.

E2E:
- Skipped — mouse and SIGHUP are runtime concerns; tests would need a
  pty or signal harness. Unit coverage is enough.

## Validation

- [ ] Manual: with sessions `claude-frontend` and `bash-helper`, type
      `cf` in filter mode; confirm `claude-frontend` is the top match.
- [ ] Manual: click a row in the picker; confirm it gets highlighted.
      Click the same row again within 500 ms; confirm the picker
      attaches.
- [ ] Manual: edit `~/.config/tmux-picker/config.toml` to set
      `[theme] accent = "#ff8800"`, then `kill -HUP $(pgrep
      tmux-picker)` (or send SIGHUP from inside the same shell).
      Confirm the action-bar accent updates and the footer flashes
      `[config reloaded]`.
