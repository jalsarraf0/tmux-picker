# tmux-picker: Discovery markers + multi-pane preview

**Date:** 2026-05-03
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #7 of 9

## Problem

Phase 6 made everyday ops one keystroke away, but the picker still treats
every session as a single anonymous row. With many sessions in flight a
user has to read the `current_command` column to spot which one is
running an editor, a long-running build, an LLM client, or just a shell.
Three small additions raise information density without making the
display noisy:

1. **No glance-based recognition.** A session running `claude` looks
   identical to one running `bash` — only `current_command` distinguishes
   them, and it sits in column 4 with no special weight.
2. **Recent activity is invisible.** `activity_display()` shows the
   relative time since the session was last touched (e.g. `2m ago`), but
   "active right now" deserves a visual signal a user can scan in <100ms.
3. **Preview shows only the highlighted pane.** Multi-window sessions hide
   information unless the user attaches.

## Solution

Three additions, all opt-in or backwards-compatible at the column level:

1. **Process markers.** A glyph next to the session name when any of its
   panes is running a recognised long-running command. Defaults cover
   `claude` (🤖), `vim` / `nvim` (✏️), `htop` / `top` / `btop` (📊),
   `node` / `npm` / `pnpm` (📦), `cargo` / `rustc` (🦀), `python` (🐍),
   `git` (🌿). Users override or extend via the new `[markers]` config
   table. Empty config disables nothing — defaults always apply unless a
   user sets `[markers] disable_defaults = true`.
2. **Activity dot.** A small `•` (Unicode `U+2022`) in the row just left
   of the activity column, coloured green when `last_activity < 60s`,
   yellow when `< 5min`, dark gray otherwise. Drops off entirely on
   stale sessions so the column stays clean.
3. **Multi-pane preview, toggled with `Tab`.** A new `PreviewMode` enum
   (`Summary` is the existing 6-line capture; `WindowsList` lists every
   window with its active pane's last non-blank line). `Tab` rotates.
   The mode persists across selection changes within one picker run.

Out of scope (defer):

- Per-window panes-tree (each pane gets its own row). `WindowsList` is
  the right level of detail for now — full tree explodes for sessions
  with many windows.
- Animated activity heatmaps. The single dot is enough.

## Architecture

### Process markers

`Session` gains:

```rust
pub struct Session {
    // ...existing fields...
    /// First marker glyph that matched any of the session's pane commands.
    pub marker: Option<String>,
}
```

`tmux::list_sessions` already calls `tmux list-panes -a` to populate
`current_command`. The same pass is extended to collect every pane's
command per session, then the picker matches each against the configured
marker map and stores the first hit on the `Session`.

Config schema additions:

```toml
[markers]
# Set to true to drop the built-in defaults entirely.
disable_defaults = false

# Pattern -> glyph (substring match, case-insensitive).
patterns = { claude = "🤖", vim = "✏️", nvim = "✏️", cargo = "🦀" }
```

`Config::Markers` carries:

```rust
pub struct Markers {
    pub disable_defaults: bool,
    pub patterns: Vec<(String, String)>, // ordered, lower-cased keys
}
```

Defaults are baked into a const `DEFAULT_MARKERS`. The merged map at
runtime is `if disable_defaults { user_only } else { defaults
chained_with user_overrides }`. User-defined patterns win on key match.

`session::marker_for` is the lookup function: takes the per-session pane
commands (a `&[String]`) and returns the first matching glyph from the
merged map, or None.

UI: column 2 (session name) becomes `<marker> <name>` with the marker
rendered in the theme accent color and ⎵-padded to a fixed 2-cell width
when present (so names line up between marked and unmarked rows).

### Activity dot

`session::activity_dot()` returns a `(char, Color)` pair:

```rust
pub fn activity_dot(&self) -> (char, ratatui::style::Color) {
    let secs = self.last_activity.as_secs();
    if self.is_stale() { (' ', Color::DarkGray) }
    else if secs < 60 { ('\u{2022}', Color::Green) }
    else if secs < 300 { ('\u{2022}', Color::Yellow) }
    else { ('\u{2022}', Color::DarkGray) }
}
```

UI: a new column to the left of the activity-text column (3 cells: pad,
dot, pad) renders the dot with its colour.

### Multi-pane preview

```rust
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PreviewMode {
    Summary,     // existing 6-line pane_capture
    WindowsList, // one row per window
}
```

`App` gains `preview_mode: PreviewMode` (default `Summary`) and a
`take_preview_window_list: bool` flag the picker loop checks each tick.
When the user presses `Tab` the mode rotates and the cache invalidates.

`tmux::list_windows(session)` returns
`Vec<(window_name, last_line: String)>`. Implementation:
`tmux list-windows -t <sess> -F '#{window_name}|#{window_active_pane_id}'`
followed by `tmux capture-pane -p -S -1 -t <pane_id>` per active pane.
Cap at 8 windows for sanity; truncate the rest with a `…(N more)` line.

UI: when `preview_mode == WindowsList`, the preview body is a
`Paragraph` with one `Line` per window: `<window_name>  ›  <last_line>`
truncated to area width. Block title flips to ` windows (Tab) `.

Input wiring: in Pick mode, `KeyCode::Tab` → `app.cycle_preview_mode()`.
In other modes Tab is a no-op (filter-mode users would otherwise lose
their focus).

## Edge cases

| Case | Behavior |
|---|---|
| No matching marker | Column renders the session name without a glyph; alignment preserved. |
| Multiple panes match different markers | First match wins (config order, then defaults if not disabled). |
| `[markers] patterns` is malformed | Warn + fall back to defaults. |
| Activity = 0 (just opened) | Green dot. |
| Stale (`is_stale() == true`) | No dot, dark gray placeholder. |
| Session with 1 window in `WindowsList` | Renders one line plus the title, no fallback to Summary. |
| `pane_capture` fails for a window | That row shows `(unavailable)`. |
| Terminal too narrow for a window line | Truncated with `…`. |

## Tests

Unit (`src/session.rs`):
- `activity_dot` returns green for activity < 60s.
- `activity_dot` returns yellow for activity 60..300s.
- `activity_dot` returns gray for activity >= 300s and not stale.
- `activity_dot` returns blank for stale.

Unit (`src/config.rs`):
- `[markers]` parses `patterns` into the right struct.
- `[markers] disable_defaults = true` drops the built-ins.
- Unknown sub-key warns but keeps defaults.

Unit (`src/session.rs` or `src/markers.rs`):
- `marker_for(["claude"], merged)` returns 🤖.
- `marker_for(["bash"], merged)` returns None.
- `marker_for(["bash", "vim"], merged)` returns ✏️ (first non-shell hit).
- User override of `claude → "C"` beats default 🤖.

Unit (`src/app.rs`):
- `cycle_preview_mode` advances Summary → WindowsList → Summary.

Integration:
- `tmux::list_windows` returns one entry per window for a session with
  three windows.
- `Session.marker` populates from list_sessions when a pane runs a
  recognised process.

E2E:
- Skipped — markers/preview/dot are visual; their correctness is covered
  by unit tests rather than scripted shell.

## Validation

- [ ] Manual: open the picker; confirm a session running `claude` shows
      🤖 next to the name.
- [ ] Manual: configure `[markers] patterns = { foo = "★" }` and a
      session with `foo` running shows ★.
- [ ] Manual: press Tab; preview switches to a windows list with the last
      line of each active pane.
- [ ] Manual: an idle-for-hours session shows no activity dot; a freshly
      active one shows green.
