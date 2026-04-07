# tmux-picker: SSH Login Session Picker

**Date:** 2026-04-07
**Status:** Draft
**Author:** jalsarraf + Claude

## Problem

SSH logins to amarillo disconnect immediately (~1 second) due to two competing
tmux-on-login blocks in the shell startup:

1. `~/.bashrc.d/tmux-autoattach.sh` — interactive picker using `exec tmux`
2. Bottom of `~/.bashrc` — fallback `exec tmux new-session -s "ssh-$$"`

Both use `exec` with no pre-flight check or error recovery. If tmux fails to
attach (terminal negotiation failure, Docker container without PTY, tmux server
issue), the SSH session dies instantly. There is no fallback to a plain shell.

Additionally, the current picker is a plain bash menu with no colors, no session
metadata, and no keyboard navigation.

## Solution

Replace the bash picker with a Rust TUI application (`tmux-picker`) that
provides an info-rich, color-coded session picker with arrow-key and number-key
navigation. The TUI communicates with a thin bash stub via stdout, and the bash
stub handles all `exec tmux` calls with a guaranteed shell fallback on any
failure.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  ~/.bashrc.d/tmux-autoattach.sh  (bash stub)             │
│  - guards: SSH + interactive + no $TMUX + no $NO_TMUX    │
│  - calls ~/.local/bin/tmux-picker                        │
│  - reads stdout for action string                        │
│  - execs tmux or drops to shell                          │
│  - fallback: binary missing/crash/non-zero → shell       │
└────────────────────────┬─────────────────────────────────┘
                         │ stdout (action protocol)
┌────────────────────────▼─────────────────────────────────┐
│  tmux-picker  (Rust binary)                              │
│  - queries tmux server for session data                  │
│  - renders TUI to stderr via alternate screen buffer     │
│  - handles input (arrows, j/k, numbers, n, s, q, Esc)   │
│  - prints action to stdout, exits 0                      │
│  - on error: exits non-zero (bash catches it)            │
└──────────────────────────────────────────────────────────┘
```

**Removed:** The `exec tmux new-session -s "ssh-$$"` block at the bottom of
`~/.bashrc` is deleted entirely. One entry point, one picker, one safety net.

## TUI Layout

The picker renders in the alternate screen buffer (no scroll pollution).

```
┌─ tmux sessions ─────────────────────────────────────────┐
│                                                         │
│  ▸ 1  main            1 win   bash        idle 3h       │
│    2  claude-aihelp   3 win   claude  ●   active 2m     │
│    3  ssh-48201       1 win   vim         idle 45m      │
│                                                         │
├─────────────────────────────────────────────────────────┤
│    n  new session     s  shell (no tmux)                │
├─────────────────────────────────────────────────────────┤
│  ↑↓ navigate  ·  enter/# select  ·  s shell  ·  q quit │
└─────────────────────────────────────────────────────────┘
```

### Color Scheme (ANSI 256, respects terminal theme)

| Element | Style |
|---|---|
| Session name | Bold white |
| `claude-*` session name | Cyan (visually distinct) |
| Attached indicator `●` | Green |
| Detached sessions | Dim/grey |
| Selected row `▸` | Reverse video highlight |
| Active sessions (< 5m) | Normal brightness |
| Stale sessions (> 1h) | Dimmed |
| Box drawing | Standard `─│┌┐└┘├┤` |

### Interaction

| Key | Action |
|---|---|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `1`-`9` | Jump to session by number |
| `Enter` | Select highlighted session |
| `n` | Prompt for new session name (inline) |
| `s` | Exit with `shell` action |
| `q` / `Esc` | Same as `s` (safe default — never trap the user) |
| 10s timeout | Auto-attach to first detached session |

### New Session Flow (pressing `n`)

When the user presses `n`, an inline text input appears at the bottom of the
TUI. Behavior:

| Condition | Action |
|---|---|
| Empty name (Enter with no input) | Cancel, return to picker |
| Name already exists as a session | Output `attach:<name>` instead |
| Invalid characters (`.`, `:`, whitespace) | Sanitize to hyphens, collapse multiples, trim |
| Valid name | Output `new:<name>` |
| `Esc` during input | Cancel, return to picker |

### Session Row Data

Each row displays:

| Field | Source | Example |
|---|---|---|
| Session name | `#{session_name}` | `claude-aihelp` |
| Window count | `#{session_windows}` | `3 win` |
| Current command | `#{pane_current_command}` via active window | `claude` |
| Attached indicator | `#{session_attached}` | `●` or blank |
| Last activity | `now - #{session_activity}` | `active 2m`, `idle 3h` |

## Tmux Data Collection

Queries tmux via subprocess (not a library):

```
tmux list-sessions -F '#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}|#{session_created}'
tmux list-windows -t <session> -F '#{window_active}|#{pane_current_command}'
```

### Data Model

```rust
struct Session {
    name: String,
    window_count: u32,
    attached: bool,
    current_command: String,
    last_activity: Duration,
    created: Duration,
}
```

### Edge Cases

| Condition | Behavior |
|---|---|
| tmux server not running | Print `shell`, exit 0 |
| Zero sessions | Print `new:main`, exit 0 |
| Exactly one detached session | Show picker (user might want shell) |
| tmux command timeout (5s) | Treat as server not running |
| Malformed tmux output | Skip malformed lines, warn to stderr |

## Stdout Protocol

All TUI rendering goes to stderr (alternate screen buffer). Stdout carries
exactly one line: the action.

| Output | Bash stub action |
|---|---|
| `attach:<name>` | `exec tmux attach-session -dt <name>` |
| `new:<name>` | `exec tmux new-session -s <name>` |
| `shell` | Do nothing, drop to prompt |

### Rules

- Exactly one line via `println!`
- Session names validated: alphanumeric, hyphens, underscores only
- Non-zero exit or empty stdout → bash treats as `shell`

## Bash Stub

File: `~/.bashrc.d/tmux-autoattach.sh` (replaces current 123-line picker)

```bash
#!/usr/bin/env bash
# tmux session picker — interactive SSH logins only.
# Calls tmux-picker binary for TUI, falls back to shell on any failure.

[[ -z "$SSH_CONNECTION" ]] && return
[[ -n "$TMUX" ]] && return
[[ "$-" != *i* ]] && return
[[ -n "${NO_TMUX:-}" ]] && return

_PICKER="${HOME}/.local/bin/tmux-picker"

if [[ -x "$_PICKER" ]]; then
    action="$("$_PICKER" 2>/dev/tty)"
    rc=$?
else
    echo "tmux-picker not found at $_PICKER — dropping to shell" >&2
    return
fi

if [[ $rc -ne 0 || -z "$action" ]]; then
    echo "tmux-picker exited $rc — dropping to shell" >&2
    return
fi

case "$action" in
    attach:*)
        sess="${action#attach:}"
        exec /usr/bin/tmux attach-session -dt "$sess"
        echo "Failed to attach to '$sess'" >&2
        ;;
    new:*)
        sess="${action#new:}"
        exec /usr/bin/tmux new-session -s "$sess"
        echo "Failed to create session '$sess'" >&2
        ;;
    shell) ;;
    *)
        echo "Unknown action from tmux-picker: $action" >&2
        ;;
esac
```

## Fastfetch Integration

**Problem:** Currently `fastfetch` runs in the `case *i*)` block before the
tmux picker. It prints, the picker launches an alternate screen (hiding it),
then tmux starts a new shell which runs `fastfetch` again.

**Fix:** Add a guard in `.bashrc` to skip `fastfetch` when in a pre-tmux SSH
state:

```bash
# Inside case *i*) block, replace bare `fastfetch` with:
if [[ -z "$SSH_CONNECTION" || -n "$TMUX" ]]; then
    fastfetch
fi
```

This means fastfetch runs:
- On local logins (no `$SSH_CONNECTION`) — always
- Inside tmux on SSH (`$SSH_CONNECTION` set, `$TMUX` set) — yes
- Pre-tmux SSH (`$SSH_CONNECTION` set, `$TMUX` empty) — skipped

Result: fastfetch prints exactly once, after the picker, inside the tmux session.

## Shell Changes Summary

| File | Change |
|---|---|
| `~/.bashrc.d/tmux-autoattach.sh` | Replace entirely with bash stub |
| `~/.bashrc` (bottom block) | Delete `exec tmux new-session -s "ssh-$$"` block |
| `~/.bashrc` (fastfetch) | Add guard to skip on pre-tmux SSH |

## Testing Strategy

### Layer 1 — Unit Tests (`cargo test`)

- tmux output parsing: valid, malformed, empty, unicode session names
- Session name validation and sanitization
- Action serialization (`attach:foo`, `new:bar`, `shell`)
- Activity duration formatting (boundary cases: 0s, 59s, 60s, 59m, 1h, 23h, 1d)
- Sort order: attached first, then by activity, then alphabetical
- Color/style selection logic (claude-* prefix, stale thresholds)

### Layer 2 — Integration Tests (real tmux, isolated socket)

- Spawn tmux server on `tmux -L tmux-picker-test`
- Create/destroy sessions programmatically
- Verify binary parses real tmux output correctly
- Test with 0, 1, 5, 20 sessions
- Test sessions with edge-case names (hyphens, underscores, long names)
- Test tmux server not running
- Test tmux command timeout

### Layer 3 — E2E Tests (`tests/e2e.sh`)

- Full flow: bash stub → binary → action output → verify tmux state
- Binary missing → verify shell fallback
- Binary crashes (SIGKILL) → verify shell fallback
- Binary outputs garbage → verify shell fallback
- `/dev/tty` unavailable → verify shell fallback
- `NO_TMUX=1` escape hatch works
- Non-interactive shell does not trigger picker

### Layer 4 — Regression Tests

- Simulate SSH disconnect scenario: spawn PTY, run bash stub, verify session survives
- Verify fastfetch runs exactly once
- Verify `claude-tmux-namer.sh` still works
- Verify `$SSH_CONNECTION` and `$TMUX` propagation through the full flow

### Lint

- `cargo fmt --check` — no formatting violations
- `cargo clippy -- -D warnings` — no warnings tolerated
- Enforced at every step and in CI

## Installation

| Step | Detail |
|---|---|
| Build | `cargo build --release` in `~/git/tmux-picker` |
| Install binary | Copy to `~/.local/bin/tmux-picker` |
| Install stub | Replace `~/.bashrc.d/tmux-autoattach.sh` |
| Remove fallback | Delete bottom-of-`.bashrc` tmux block |
| Guard fastfetch | Add SSH/TMUX guard |
| Backup | Keep old `tmux-autoattach.sh.bak` until verified |

## Rollback

- `NO_TMUX=1 ssh amarillo` bypasses everything — guaranteed shell access
- Restore `tmux-autoattach.sh.bak` to revert to old picker
- Re-add bottom-of-`.bashrc` block if needed

## Verification After Install

1. `ssh localhost` — picker appears, select session, fastfetch once, prompt
2. `NO_TMUX=1 ssh localhost` — straight to shell, no picker
3. `ssh amarillo` from dominus — picker appears, no disconnect
4. `rm ~/.local/bin/tmux-picker && ssh localhost` — warning, drops to shell
