# tmux-picker: Config + Extensibility

**Date:** 2026-05-02
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #4 of 4

## Problem

Constants are hardcoded: 10-second auto-attach, color choices, navigation keys.
Users can't tune the picker without recompiling.

## Solution

Read a small TOML config from `~/.config/tmux-picker/config.toml`. Missing file
or missing keys → fall back to current built-in defaults. No config required to
keep the existing experience.

```toml
# ~/.config/tmux-picker/config.toml — all keys optional

# How long (seconds) before auto-attaching to the most-recent detached session.
# 0 disables auto-attach.
timeout_secs = 10

# Override accent colors. Values: "black" "red" "green" "yellow" "blue"
# "magenta" "cyan" "white" "darkgray" "lightred" ... (crossterm color names).
[theme]
accent = "cyan"      # used for action keys (n, /, s) and the claude-* badge
warning = "red"      # used for the K (kill) key and ConfirmKill prompt
selection_bg = "darkgray"
```

Out of scope (defer):
- Keybinding remapping. Adds parsing complexity for marginal benefit; the
  current bindings cover everything.
- Action hooks (run a script on attach/new). Real risk of tying tmux-picker
  to user shell quirks. The bash stub already provides a hook surface.

## Architecture

New file `src/config.rs`:

```rust
pub struct Config {
    pub timeout_secs: u64,
    pub theme: Theme,
}

pub struct Theme {
    pub accent: ratatui::style::Color,
    pub warning: ratatui::style::Color,
    pub selection_bg: ratatui::style::Color,
}

impl Config {
    pub fn load() -> Self;            // never fails — falls back to defaults
    pub fn from_str(s: &str) -> Self; // for tests
    pub fn default() -> Self;
}
```

Path resolution:

1. `$XDG_CONFIG_HOME/tmux-picker/config.toml` if `$XDG_CONFIG_HOME` is set.
2. Else `$HOME/.config/tmux-picker/config.toml`.
3. Else built-in defaults.

A malformed config logs a single line to stderr and falls back to defaults
(silent failure would be worse than a quiet warning).

`Config` is loaded once in `main()` and passed to `App::new(sessions, &config)`
so the App can use `config.timeout_secs` instead of the constant. The `ui`
module reads color overrides from a borrowed `&Theme` passed via a new
`UiContext` argument, so we don't have to thread the full config around.

Add `toml = "0.9"` dependency without `serde`. We parse via `toml::from_str()`
which returns a `Table`. Keys are looked up explicitly — no derive macros, no
extra deps.

## Edge cases

| Case | Behavior |
|---|---|
| No config file | Built-in defaults, silent. |
| Malformed TOML | One-line stderr warning, defaults. |
| Unknown color string | One-line stderr warning, that field uses default. |
| `timeout_secs = 0` | Auto-attach disabled (the tick handler treats 0 as "never fire"). |
| `timeout_secs` very large (e.g., u64::MAX) | Effectively never fires; harmless. |

## Tests

- `Config::from_str` with empty input → returns defaults.
- `Config::from_str` with malformed TOML → returns defaults.
- `Config::from_str` with `timeout_secs = 30` → applied.
- `Config::from_str` with theme accent override → parsed to expected Color.
- `Config::from_str` with unknown color name → field falls back to default.

## Validation

- [ ] Manual: write `timeout_secs = 3` in config; verify the auto-attach
      countdown starts at 3.
- [ ] Manual: write `[theme] accent = "magenta"`; verify action keys render
      magenta.
