# tmux-picker: Fixes + UX foundation

**Date:** 2026-05-03
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #5 of 9

## Problem

After phases 1–4 the picker has the right feature set but a few rough edges
remain that hurt reliability and discoverability:

1. **Config error reporting is asymmetric.** `Config::from_str` logs malformed
   TOML to stderr and falls back to defaults, but `Config::load` swallows
   filesystem errors silently — a config that exists but cannot be read (perm
   denied, broken symlink) leaves the user wondering why their overrides do
   not take effect.
2. **Theme colors are limited to 16 named values.** Power users on
   truecolor-capable terminals cannot use brand colors or 256-color indexes.
3. **Selected-index can drift past the visible list.** After `K` (kill)
   removes the highlighted session, the next refresh shrinks
   `filtered_indices` but `selected` is not clamped — an immediate `Enter`
   either confirms the wrong session or no-ops on `selected_session()`'s
   bounds check.
4. **Discovery is poor for new users.** No help overlay; the only way to learn
   the kill / filter / new-session bindings is to read the README. The footer
   hints are decent but the bindings list is long enough to deserve a `?`
   overlay.
5. **Config debugging is opaque.** When a user's overrides fail to apply, the
   only feedback is the one-line warning printed during the picker's render
   pass — easy to miss, never re-displayable. No standalone "what does
   tmux-picker think my config is?" command.

## Solution

Five changes, all backward-compatible, no migration:

1. `Config::load` reports `read_to_string` errors via the same one-line
   stderr format used for parse errors, then returns defaults (unchanged
   behavior beyond the new log line).
2. `apply_color` accepts three additional value shapes:
   - Hex `"#rrggbb"` and `"#rgb"` (3-digit shorthand) → `Color::Rgb`.
   - Decimal integer `"0"`–`"255"` (string) → `Color::Indexed`.
   - TOML integer `0`–`255` → `Color::Indexed` (so users can write
     `accent = 196` without quoting).
   Unknown strings still warn and keep defaults.
3. New `App::clamp_selected` helper. Called from `recompute_filter`,
   `confirm_kill`, and the picker loop's tmux-refresh path. Pulls `selected`
   into `0..filtered_indices.len()` (or `0` when empty).
4. New `Mode::Help`. `?` from `Mode::Pick` opens an overlay listing every
   keybinding grouped by mode (Pick / NewInput / Filter / ConfirmKill).
   `Esc`, `?`, or `q` closes it. The overlay does not block timer ticks but
   the timer is reset on entry so a user reading help does not get
   auto-attached out from under them.
5. New CLI flag `tmux-picker --check-config`. Prints two sections to stdout:
   - The parse warnings (if any) — same lines `Config::load`/`from_str` emit
     to stderr during normal startup, but consolidated.
   - The effective config rendered as TOML — what the picker would actually
     use after defaults + overrides + sanitisation.
   Exits `0` always (warnings are still warnings, not errors).

Out of scope (defer to later phases):

- Keybinding remapping. Still YAGNI.
- Hot-reload. Phase 8 owns that.
- Markers / multi-window preview. Phase 7.

## Architecture

### Color parsing

`apply_color` currently dispatches only on string lookup. Refactor to:

```rust
fn apply_color(table: &toml::Table, key: &str, dest: &mut Color) {
    let Some(val) = table.get(key) else { return };
    match parse_color_value(val) {
        Ok(c) => *dest = c,
        Err(e) => eprintln!("tmux-picker config: theme.{key}: {e}; using default"),
    }
}

fn parse_color_value(val: &toml::Value) -> Result<Color, String>;
```

`parse_color_value` accepts `Value::String` and `Value::Integer`:

- Integer in `0..=255` → `Color::Indexed(n as u8)`.
- Integer out of range → `Err("indexed color must be 0..255, got {n}")`.
- String `"#rrggbb"` (case-insensitive) → `Color::Rgb`.
- String `"#rgb"` shorthand → expand each nibble (`#abc` → `#aabbcc`).
- String matching the existing named table → that named color.
- String `"0"`–`"255"` (decimal digits only) → `Color::Indexed`.
- Anything else → `Err("unknown color '{s}'")`.

The existing `parse_color(&str)` helper keeps its current shape for the
named-color path; `parse_color_value` wraps and extends it.

### Mode::Help

Add a fourth `Mode` variant:

```rust
pub enum Mode { Pick, NewInput, Filter, ConfirmKill, Help }
```

`App` gains `pub fn enter_help(&mut self)` / `pub fn cancel_help(&mut self)`.
Entry resets the auto-attach timeout and clears the preview cache (the help
overlay covers the preview area so the cache would be stale on exit anyway).
Exit returns to `Mode::Pick`.

Input wiring: `?` in `handle_pick_key` → `app.enter_help()`. `Esc`, `?`,
or `q` in a new `handle_help_key` → `app.cancel_help()`.

UI wiring: `ui::draw` checks `app.mode == Mode::Help` and renders a centered
overlay using `Block::bordered().title(" help ")`. Body is a `Paragraph`
with a static keybinding table — no dynamic content beyond the colors from
`UiContext`.

### Selected clamp

```rust
impl App {
    fn clamp_selected(&mut self) {
        let max = self.filtered_indices.len();
        if max == 0 {
            self.selected = 0;
        } else if self.selected >= max {
            self.selected = max - 1;
        }
    }
}
```

Call sites:
- End of `recompute_filter` (every filter mutation already calls it; the
  current `selected = 0` in callers becomes redundant but harmless and we
  keep it for explicit reset semantics on filter input).
- `confirm_kill` after the session list is rebuilt by the picker loop —
  actually, `confirm_kill` itself only sets `pending_kill`; the loop does
  the rebuild. The clamp goes in the path that replaces `self.sessions` and
  re-runs `recompute_filter`. Concretely: a new
  `App::replace_sessions(&mut self, Vec<Session>)` method that the picker
  loop already calls (via direct field assignment today) — switch to the
  method and have it run `recompute_filter` + `clamp_selected`.

### --check-config

`src/cli.rs` already has a subcommand parser. Add a new top-level flag (not
a subcommand) handled before the subcommand dispatch:

```
tmux-picker --check-config
```

Behavior: load `Config` via the same path `main()` does, but capture the
warning lines into a `Vec<String>` instead of writing to stderr. Print:

```
# tmux-picker config check

# warnings
<one line per warning, or "(none)">

# effective config
timeout_secs = <n>

[theme]
accent = "..."
warning = "..."
selection_bg = "..."
```

Implementation: `Config::load_with_warnings() -> (Config, Vec<String>)`.
The existing `Config::load()` becomes a thin wrapper that prints warnings
to stderr. Color values that came from RGB or indexed inputs render back
as their canonical TOML form (`"#ff8800"`, `196`).

## Edge cases

| Case | Behavior |
|---|---|
| Config file unreadable (EACCES) | New stderr line `read error: <io error>; using defaults`, picker continues. |
| `accent = "#fff"` | Expanded to `Color::Rgb(0xff, 0xff, 0xff)`. |
| `accent = "#ggg"` | Warns, keeps default. |
| `accent = 256` (out of range) | Warns, keeps default. |
| `accent = -1` | Already rejected by existing `n >= 0` guard for ints; warns. |
| Kill the only filtered session | `clamp_selected` snaps to 0; if list is now empty the picker shows the "no sessions" placeholder. |
| `?` pressed in `NewInput` / `Filter` / `ConfirmKill` | No-op (each mode handler ignores `?` already). |
| `--check-config` with no config file | Prints `(none)` warnings + the all-defaults effective config. |

## Tests

Unit (`src/config.rs`):
- `apply_color` accepts `"#ff8800"` → `Color::Rgb(0xff, 0x88, 0x00)`.
- `apply_color` accepts `"#abc"` → `Color::Rgb(0xaa, 0xbb, 0xcc)`.
- `apply_color` accepts integer `196` → `Color::Indexed(196)`.
- `apply_color` accepts string `"196"` → `Color::Indexed(196)`.
- `apply_color` rejects integer `256` (warns, default kept).
- `apply_color` rejects malformed hex `"#gg00ff"`.
- `Config::load_with_warnings` collects warnings into the returned vec
  rather than printing them.

Unit (`src/app.rs`):
- `clamp_selected` no-ops when in range.
- `clamp_selected` snaps `selected` down when filter shrank.
- `clamp_selected` snaps `selected` to 0 when list empty.
- `enter_help` / `cancel_help` flip the mode and reset the preview cache.

Unit (`src/input.rs`):
- `?` from `Mode::Pick` enters `Mode::Help`.
- `?`, `Esc`, `q` from `Mode::Help` returns to `Mode::Pick`.
- Other keys in `Mode::Help` are no-ops.

Integration (`tests/`):
- Render the help overlay with `ratatui::backend::TestBackend`; assert the
  buffer contains the binding strings and the overlay border.

E2E (`tests/e2e.sh`):
- `tmux-picker --check-config` exits 0 with no config file.
- `tmux-picker --check-config` exits 0 and prints a warning line for an
  unknown color.

## Validation

- [ ] Manual: `chmod 000 ~/.config/tmux-picker/config.toml` and confirm a
      stderr warning, picker continues with defaults.
- [ ] Manual: `accent = "#ff8800"` and confirm orange action keys.
- [ ] Manual: kill the highlighted session, immediately press `Enter`,
      confirm the picker attaches to a real session (not a phantom row).
- [ ] Manual: press `?` to view help, `Esc` to dismiss, confirm timer
      restarts at the configured value.
- [ ] Manual: `tmux-picker --check-config` shows the effective config and
      any warnings.
