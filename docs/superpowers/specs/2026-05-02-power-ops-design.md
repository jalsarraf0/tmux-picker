# tmux-picker: Power Ops

**Date:** 2026-05-02
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #3 of 4

## Problem

With more than ~5 sessions the picker requires too many keystrokes to find
one ("scroll, scroll, scroll"). And there's no way to clean up dead sessions
without dropping to a shell and running `tmux kill-session` manually.

## Solution

Two power-user operations, both reachable with one keystroke from Pick mode:

1. **Filter mode** (`/`) — substring-match the session list as you type.
   Matches against name + label + project (case-insensitive). Esc/Enter exits
   filter mode; Enter also confirms the currently-selected match.
2. **Kill** (`K`, capital) — kill the currently-selected session after a
   confirm prompt that requires pressing `y` to proceed (`n` or any other
   key cancels). Refreshes the session list after success.

Out of scope (defer):
- Rename — already achievable as "create new + kill old" with the existing
  `n` flow plus the new `K` op.
- Detach-others — rare need.
- Sort options — current sort order (attached, then most-recent activity)
  has been working well; no concrete pain to fix.

## Architecture

`Mode` enum gains two variants:

```rust
pub enum Mode {
    Pick,
    NewInput,
    Filter,         // `/` typed; collecting filter substring
    ConfirmKill,    // `K` pressed; awaiting y/n
}
```

`App` gains:

```rust
pub filter: String,       // current filter substring (lowercased on read)
pub filtered_indices: Vec<usize>,  // indices into self.sessions that match
```

`filtered_indices` is recomputed whenever `filter` changes or sessions
change. `App::selected` is interpreted as an index into `filtered_indices`
when in Filter mode, and into `sessions` when in Pick mode.

To keep this simple: when entering Filter mode, snapshot the current
`selected` (selected name) so we can restore it on cancel. When matching,
update `selected` to point at the first index in `filtered_indices` that
contains a session whose name equals the snapshotted name; if none, reset
to 0.

For kill: when ConfirmKill mode is entered, store a `kill_target: Option<String>`
on App with the selected session's name. On `y`, run a new `tmux::kill_session`
helper, refresh the session list via the existing list_sessions code path
on the next tick, and return to Pick mode. On `n`/Esc, just return to Pick.

## CLI / behavior changes

No new subcommands. TUI gains:

- `/` enters Filter mode.
- In Filter mode: `Esc` cancels (restores selection), `Enter` confirms current
  match, any printable char appends, Backspace removes a char.
- `K` (uppercase, shift+k) enters ConfirmKill mode for the selected session.
- In ConfirmKill mode: `y`/`Y` kills, anything else cancels.

The help bar in Pick mode is updated to mention `/` filter and `K` kill.

## Edge cases

| Case | Behavior |
|---|---|
| Filter matches nothing | Selected = 0 (no-op selector); Enter is a no-op. UI shows "no matches" empty-state. |
| Kill the only session | After kill, list is empty → picker auto-creates `main`. |
| Kill the currently-attached session | Same as above. tmux handles the disconnect. |
| Filter while a session is killed in another terminal | Next tick refreshes the list via the live `list_sessions` poll already in the TUI loop — no special path needed (we already have a 250 ms tick). Actually no, we *don't* poll list_sessions every tick — we only call it once at startup. **Decision:** introduce a "session list dirty" flag. Set it after kill; on the next tick the loop re-fetches list_sessions. This keeps the no-kill case unchanged. |
| `/` typed while in NewInput mode | Treated as a regular char (NewInput already accepts arbitrary chars). |

## Tests

- App unit tests for filter substring matching (case insensitive, multi-token
  not supported, empty filter = all).
- App unit tests for ConfirmKill mode transitions (y, n, esc).
- Integration test: create N sessions on default socket, run picker
  programmatically? No — the TUI is interactive. Instead, test `tmux::kill_session`
  directly (create, kill, assert list excludes it).

## Validation

- [ ] Manual: `/cl` filters to only `claude-*` sessions.
- [ ] Manual: `K`, `y` removes selected session and refreshes the list.
- [ ] Manual: `/`, `Esc` restores prior selection.
