# tmux-picker: Context Display

**Date:** 2026-05-02
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #2 of 4

## Problem

After sub-project #1, the picker shows label/project/purpose for each session.
But it still doesn't show **what's actually happening** in a session right now.
A user picking between three `claude-*` sessions wants to know: is one waiting
for input? Is one mid-build? Is one printing an error?

## Solution

Add a **preview pane** beneath the detail row showing the last few lines of
the selected session's active pane. This makes "what's running" visible at
a glance without attaching.

Out of scope (defer or skip):
- Live git status (`*` for dirty repo). Costly: a `git status` per session
  per tick. Defer until a clear pain shows up.
- Tail-like live updates of the preview pane. The picker is short-lived (10s
  default), so 1-tick refresh of the selected session's preview is enough.

## Architecture

1. New `pane_capture(session, lines)` in `tmux.rs` invoking
   `tmux capture-pane -t <session> -pJ -S -<lines>` to grab the last `<lines>`
   wrapped lines from the active pane. `-p` writes to stdout, `-J` joins
   wrapped lines, `-S -N` starts N lines back.
2. `App` gains `preview: Option<String>` and `preview_for: Option<String>`
   (the session name the preview is for). On selection change, the cache is
   invalidated; on the next `tick()`, the picker re-fetches the preview for
   the new selected session.
3. `ui::draw` adds a 6-line preview strip between detail and actions when
   a preview exists. The strip is bordered and labeled "preview" in dim.

## CLI / behavior changes

None. Only the TUI changes.

## Edge cases

| Case | Behavior |
|---|---|
| Selected session was killed mid-render | `pane_capture` returns Err; preview rendered as empty box with "(unavailable)". |
| Pane has no scrollback | Capture returns the visible buffer (still useful). |
| User cycles selection rapidly | We only fetch preview on tick boundaries (250ms) — no per-keypress fetch storm. |
| Picker has no sessions at all | No preview rendered (no selected session). |

## Tests

- `tmux::pane_capture` integration test: create session, send a command,
  capture, assert output appears.
- UI: no new unit tests — preview strip rendering exercised by manual smoke.

## Validation

- [ ] Manual: launch picker, watch preview update as selection moves.
- [ ] Manual: kill a session in another terminal, verify picker survives.
