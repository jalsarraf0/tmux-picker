# tmux-picker: Quick ops

**Date:** 2026-05-03
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #6 of 9

## Problem

After phase 5 the picker discovers and configures cleanly, but everyday
session manipulation still requires dropping out of the picker:

1. **Rename.** Today the only way to rename a session is to leave the picker,
   type `tmux rename-session -t <old> <new>`. A `kill` + `new-session` would
   destroy the metadata.
2. **Sort.** The picker shows whatever order tmux returned (effectively
   creation order). Users with many sessions cannot promote
   recently-active or attached ones to the top.
3. **Yank.** Pasting the highlighted session's name into another tool means
   reading it off the screen and re-typing it.
4. **New session friction.** Pressing `n` and typing a name often means the
   user knows exactly which project they want to work on. Walking up the
   pane cwd to a `.git` is fine when the user happens to start in that
   directory, but at SSH login the new session's cwd is `$HOME`, so the
   automatic label/project detection produces nothing useful.

## Solution

Three new pick-mode keys plus a one-line addition to the bash stub:

1. **`r` — rename.** Enters a new `Mode::Rename` (input buffer pre-filled
   with the current name). `Enter` commits via `tmux rename-session -t old
   new`. `Esc` cancels. Validation reuses `validate_session_name` so the
   name rules match `n`. Metadata follows the session because tmux
   user-options are tied to the session id, not the name.
2. **`o` — sort cycle.** Rotates `SortMode` through
   `LastActivity → AttachedFirst → Name → IdleLongest → LastActivity`.
   The current mode is shown briefly in the footer (e.g. `[sort: by
   name]`). Default is `LastActivity` so recently-touched work surfaces.
   Sort survives re-fetching the session list (post-kill); it does not
   persist across launches.
3. **`y` — yank session name.** Copies `selected_name()` to the system
   clipboard via the first working tool from `wl-copy` → `xclip -selection
   clipboard` → `xsel -b`. A short footer flash confirms the copy or
   reports that no clipboard tool was found.
4. **bash stub auto-label after `n`.** The `new:` branch in
   `shell/tmux-autoattach.sh` now runs `tmux-picker auto "$sess" 2>/dev/null
   || true` after a successful `new-session`. The auto command becomes
   smarter: when pane cwd has no `.git` and no project is set, it tries
   `~/git/<sessname>` as a fallback project root.

## Architecture

### Rename

New mode variant and methods:

```rust
pub enum Mode { Pick, NewInput, Filter, ConfirmKill, Help, Rename }

impl App {
    pub fn enter_rename(&mut self);     // pre-fills input from selected_name
    pub fn cancel_rename(&mut self);    // back to Pick, clear input
    pub fn confirm_rename(&mut self);   // validates + sets pending_rename
    pub fn take_pending_rename(&mut self) -> Option<(String, String)>;
}
```

Picker loop side: just like `take_pending_kill`, the loop pulls
`take_pending_rename()` after each input pass and calls a new
`tmux::rename_session(old, new)`. The session list goes dirty so the
next iteration re-fetches.

`tmux::rename_session` wraps `tmux rename-session -t <old> <new>`.

Input wiring: in Pick mode `KeyCode::Char('r')` → `app.enter_rename()`. In
Rename mode the dispatcher reuses `handle_rename_key`, which is a near
clone of `handle_input_key` modulo the confirm path.

UI wiring: a new `Mode::Rename` arm in both `draw_actions` and `draw_help`
prompts the user with `rename: <new-name>█`.

### Sort

```rust
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SortMode { LastActivity, AttachedFirst, Name, IdleLongest }
```

`App` gains `sort_mode: SortMode`, defaulting to `LastActivity`. A new
`App::sort_sessions(&mut self)` applies the current mode in place using
`Vec::sort_by` over a key derived per mode:

| Mode | Sort key (descending unless noted) |
|---|---|
| `LastActivity` | `Duration::MAX - last_activity` (most-recent first) |
| `AttachedFirst` | `(!attached, name.to_lowercase())` (ascending) |
| `Name` | `name.to_lowercase()` (ascending) |
| `IdleLongest` | `last_activity` (descending — most idle first) |

`sort_sessions` runs in `App::new` after the initial sessions are stored,
in `replace_sessions` after the new vec is in place, and in `cycle_sort`.
Each call also runs `recompute_filter` so `filtered_indices` follows the
new order, and then `clamp_selected`.

Input wiring: in Pick mode `KeyCode::Char('o')` → `app.cycle_sort()`. The
existing 'o' key is currently unbound. The footer displays the current
sort mode for ~2 seconds via a new `app.sort_flash_until: Option<Instant>`
field (cleared in `tick`).

### Yank

A new module `src/clipboard.rs` exposes:

```rust
pub fn copy(text: &str) -> Result<&'static str, String>;
```

Returns the tool name on success (so the UI can flash "yanked via
wl-copy"). Tries each candidate (`wl-copy`, `xclip -selection clipboard`,
`xsel -b`) using `std::process::Command`, piping `text` to stdin. Stops at
the first one that exits 0.

Input wiring: in Pick mode `KeyCode::Char('y')` → `app.yank_selected()`.
That method reads `selected_name()`, calls `clipboard::copy`, and stores a
result message in `app.flash`, an `Option<(String, Instant)>` cleared in
`tick` after a short window.

UI: the existing footer hint becomes mode-aware again — when `flash` is
set, render its message instead of the default Pick-mode hint.

### bash stub auto-label

```bash
new:*)
    sess="${action#new:}"
    if "$_TMUX" new-session -As "$sess" 2>/dev/null; then
        "$_PICKER" auto "$sess" 2>/dev/null || true
    else
        _tmux_fallback "$sess"
    fi
    ;;
```

`metadata::auto_detect` gains a `~/git/<sessname>` fallback: when the pane
cwd resolves to `$HOME` (or otherwise produces no `.git` root) and the
target session has no metadata yet, look for `~/git/<sessname>`. If that
directory exists, set `project = ~/git/<sessname>`, `label = <sessname>`.
Existing manually-set fields are preserved (the auto path already only
fills empty fields).

## Edge cases

| Case | Behavior |
|---|---|
| Rename to invalid name (`/` etc) | Same path as new-session input: footer shows error, mode stays Rename. |
| Rename to a name that already exists | tmux refuses; `rename_session` returns an error which surfaces as a one-line stderr warning post-render. |
| `o` with one session | Cycle is a no-op visually but still advances the mode. |
| Sort by `IdleLongest` with attached sessions | Idle on attached is 0, so attached sessions sink to the bottom. |
| `y` with no clipboard tool installed | Flash reports "no clipboard tool (wl-copy / xclip / xsel)". |
| `y` with no selection (empty list) | Flash reports "(no session)". |
| `auto` after `new:` when `~/git/<name>` doesn't exist | Existing pane-cwd path runs; if that also yields nothing, no metadata is written. |

## Tests

Unit (`src/app.rs`):
- `enter_rename` pre-fills `input` from `selected_name`.
- `confirm_rename` with a valid name pushes `pending_rename = Some((old, new))`.
- `confirm_rename` with the same name is a no-op (pending stays None).
- `cancel_rename` clears input and returns to Pick.
- `cycle_sort` rotates through every variant.
- `sort_sessions(LastActivity)` produces most-recent-first ordering.
- `sort_sessions(AttachedFirst)` puts attached before detached.
- `sort_sessions(Name)` is alphabetical case-insensitive.
- `sort_sessions(IdleLongest)` puts longest idle first.
- `yank_selected` with a stub clipboard sets a success flash.

Unit (`src/clipboard.rs`):
- `copy` returns Err when no tools are on PATH (test by manipulating
  PATH within the test, or by depending on `which` returning none).

Unit (`src/input.rs`):
- `r` from Pick enters Rename mode.
- `o` from Pick cycles sort_mode forward by 1.
- `y` from Pick triggers yank.

Integration:
- `tmux::rename_session` round-trips: create a session, rename, confirm
  the new name appears in `list_sessions`.
- `metadata::auto_detect` fallback: with `pane_current_path = $HOME` and a
  `~/git/<dummy>/.git` directory, sets project / label correctly.

E2E (`tests/e2e.sh`):
- After phase 5's three new tests, add: rename-session round-trip via
  the picker binary's tmux integration test pattern.

## Validation

- [ ] Manual: press `r`, type a new name, confirm rename.
- [ ] Manual: cycle sort modes with `o`; verify ordering changes and the
      footer flashes the active mode.
- [ ] Manual: press `y` and paste somewhere; verify the session name lands.
- [ ] Manual: from SSH login, hit `n`, type the name of a project under
      `~/git/`, confirm `tmux-picker show <sess>` lists project + label.
