# tmux-picker: Session Metadata + Smart Naming

**Date:** 2026-05-02
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #1 of 4 (roadmap: metadata → context display → power ops → config)

## Problem

`tmux-picker` shows session **names** but not **purpose**. After a few hours
the picker fills with `claude-aihelp`, `main`, `ssh-48201`, `work` and the user
forgets what each one was for ("which one was the auth refactor?"). Names are
short and lossy by design — git branch, current PR, "what I'm doing" cannot fit.

The user wants:

1. A richer label per session ("Refactoring auth middleware", "PR #234 review",
   "Debug runner queue") visible in the picker alongside the bare name.
2. Auto-derived label/project on session creation when the cwd is inside a git
   repo (so a fresh `claude-aihelp` session shows project path automatically).
3. A way for **Claude Code** running inside a session to set its own label as
   it works ("Now refactoring `src/auth.rs`"), so the picker reflects the
   current task without manual annotation.

## Solution

Add **session-scoped metadata** stored in tmux user-options (key/value pairs
attached to each session by tmux itself). Expose CLI subcommands to read/write
metadata. Render label + project in the picker UI.

Tmux user-options are the right storage because:

- They live with the session: `kill-session` → metadata gone (correct).
- No external state file to manage, lock, or garbage-collect.
- `tmux show-options -t <session> -v @key` is a one-liner from any language.
- Survives detach/reattach and tmux server restart-with-resurrect.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  tmux-picker (Rust binary)                                  │
│                                                             │
│  Subcommands:                                               │
│    (no args)              → TUI picker (existing)           │
│    label <sess> [opts]    → set label/project/purpose       │
│    show <sess>            → dump metadata as TOML           │
│    auto <sess>            → auto-detect from session cwd    │
│    --help / --version                                       │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│  tmux user-options on each session                          │
│    @tmux_picker_label    "Refactoring auth middleware"      │
│    @tmux_picker_project  "/home/jalsarraf/git/aihelp"       │
│    @tmux_picker_purpose  "PR #234"                          │
│    @tmux_picker_label_at "1746201600"  (epoch when set)     │
└─────────────────────────────────────────────────────────────┘
                 ▲
                 │ optional
                 │
┌────────────────┴────────────────────────────────────────────┐
│  Claude Code (or any script) running inside a session       │
│  $ tmux-picker label "$(tmux display -p '#S')" \            │
│       --label "Investigating runner queue depth"            │
└─────────────────────────────────────────────────────────────┘
```

## CLI Surface

All subcommands return exit 0 on success, non-zero on error, and print
diagnostic messages to stderr.

### `tmux-picker label <session> [flags]`

Set or update metadata for a session. Flags are independent — pass any subset.

```
--label <text>     Human label, e.g., "Refactoring auth middleware"
--project <path>   Absolute project path, e.g., "/home/jalsarraf/git/aihelp"
--purpose <text>   Short purpose, e.g., "PR #234"
--clear            Remove all tmux-picker metadata for this session
```

Behavior:

- Verify session exists via `tmux has-session -t <session>`. If absent → exit
  non-zero with "session 'X' does not exist".
- For each flag passed, run `tmux set-option -t <session> @tmux_picker_<key>
  <value>`. Updates `@tmux_picker_label_at` to the current epoch when any of
  label/project/purpose change.
- `--clear` runs `tmux set-option -tu` (unset) for each `@tmux_picker_*` key.
- Reject empty flag values (e.g., `--label ""`) with a clear error.
- Reject `--clear` combined with other flags (mutually exclusive).

### `tmux-picker show <session>`

Print metadata as TOML to stdout:

```toml
session = "claude-aihelp"
label = "Refactoring auth middleware"
project = "/home/jalsarraf/git/aihelp"
purpose = "PR #234"
label_at = 1746201600
```

Behavior:

- If session does not exist → exit non-zero with "session 'X' does not exist"
  (consistent with `label`).
- If session exists but has no metadata → print just `session = "<name>"`.
- Missing individual keys are omitted from output.
- Reads metadata via `tmux list-sessions -t <session> -F '...'` (single tmux
  invocation), not per-key `show-options` calls.

Use TOML rather than JSON because no other tool in this stack needs JSON,
and the user prefers TOML configs (per CLAUDE.md).

### `tmux-picker auto <session>`

Auto-detect project + label from the session's working directory.

Algorithm:

1. Get the session's **active** pane working dir:
   `tmux display-message -t <session> -p '#{pane_current_path}'`
   (no `:window.pane` suffix — tmux defaults to the active window's active
   pane, which reflects what the user is actually doing.)
2. Walk up from that path looking for `.git`. If found, project = git root.
   Otherwise project = pane working dir (still useful).
3. Set `@tmux_picker_project` to the absolute path.
4. Set `@tmux_picker_label` to `basename(project)` if no label is currently
   set. Never overwrite a manually-set label.
5. If git repo and `git -C <project> branch --show-current` succeeds and is
   non-empty, set `@tmux_picker_purpose` to `branch:<name>` if no purpose is
   currently set. Same overwrite rule.

This is the subcommand `claude-tmux-namer.sh` will call after renaming.

### Argument parsing

Use `clap` with the `derive` feature. Adds ~150 KB to the binary, which is an
acceptable cost for a clean argument parser with auto-generated `--help`. The
existing TUI behavior (no args) is preserved by making subcommands optional and
defaulting to the picker.

## TUI Changes

### Per-row display

The selected-session highlight, attached indicator, command, and activity
columns stay as they are. Two changes:

1. **Name column** shows the label (if set) followed by the bare name in dim
   parentheses; falls back to bare name if no label.

   ```
     1  main                            1 win   bash        idle 3h
   ▸ 2  Refactoring auth (claude-aihelp) 3 win  claude  ●   active 2m
     3  PR #234 review (work)            2 win  vim         idle 45m
   ```

2. The window/command/activity columns are unchanged — they're still small and
   informative. The label column expands and the rest stay fixed.

### Detail row under selected session

Beneath the selected session, render a single dim line with project + purpose
if either is set:

```
▸ 2  Refactoring auth (claude-aihelp) 3 win  claude  ●   active 2m
   ↳ ~/git/aihelp  ·  PR #234
```

The arrow uses U+21B3. Project path collapses `$HOME` to `~` for readability.
Hidden when both project and purpose are unset.

### Layout impact

Total height grows by 1 line for the selected-session detail row. Existing
3-section layout (table / actions / help) becomes (table / detail / actions /
help). The detail row is a 1-line strip between table and actions, only
rendered when the selected session has metadata.

## Module Layout

New file: `src/metadata.rs`

```rust
pub struct Metadata {
    pub label: Option<String>,
    pub project: Option<String>,
    pub purpose: Option<String>,
    pub label_at: Option<u64>,  // epoch seconds
}

pub fn read(session: &str) -> Result<Metadata, String>;
pub fn write(session: &str, m: &Metadata) -> Result<(), String>;
pub fn clear(session: &str) -> Result<(), String>;
pub fn auto_detect(session: &str) -> Result<Metadata, String>;
```

`src/tmux.rs` gains:

- `pane_current_path(session: &str) -> Result<String, String>`
- `set_user_option(session, key, value)` and `unset_user_option(session, key)`
- `get_user_option(session, key) -> Option<String>` (None if unset)

`src/session.rs` gains a `metadata: Option<Metadata>` field. `list_sessions()`
populates it via a single batch call:
`tmux show-options -g -A -t <session>` is too coarse — instead use
`tmux list-sessions -F '#{session_name}|#{@tmux_picker_label}|...'` so we
collect everything in one tmux invocation, no per-session round-trips.

The format string becomes:

```
#{session_name}|#{session_windows}|#{session_attached}|#{session_activity}|#{@tmux_picker_label}|#{@tmux_picker_project}|#{@tmux_picker_purpose}|#{@tmux_picker_label_at}
```

Unset user-options expand to empty string in tmux's `-F` formatter, so empty
fields → `Option::None`. Field count grows from 4 to 8; parser updates to
splitn(8) and treats trailing empties as `None`.

`src/main.rs` gains subcommand routing:

```rust
match Cli::parse().command {
    None | Some(Picker) => run_tui(),
    Some(Label { ... }) => run_label(...),
    Some(Show { ... })  => run_show(...),
    Some(Auto { ... })  => run_auto(...),
}
```

`src/ui.rs` gains a detail-row renderer.

## Integration with `claude-tmux-namer.sh`

That hook lives in `~/.bashrc.d/` outside this repo. After it renames a tmux
session to `claude-<project>`, it calls:

```bash
tmux-picker auto "claude-<project>" 2>/dev/null || true
```

Failure is silent — the rename has already happened. Documented in README; the
hook itself is owned by user dotfiles.

## Tests

Unit tests in each module:

- `metadata::tests` — parse/serialize Metadata, format TOML output, auto-detect
  with mocked `pane_current_path`, label-not-overwritten-by-auto rule,
  empty-fields handling.
- `tmux::tests` — extend `parse_session_line` for 8-field format, including
  empty trailing fields. Add tests for `set_user_option` value escaping (values
  with spaces, single quotes).
- `session::tests` — sort order with metadata present (metadata does not
  affect sort).
- `ui::tests` — none currently; not adding (rendering is exercised by
  integration tests).

Integration tests in `tests/integration.rs`:

- Set label via `tmux set-option -t <s> @tmux_picker_label "..."`, then call
  the binary's `show` subcommand and assert output.
- Set project and call `auto` on a session whose pane cwd is a git repo, assert
  label is set to repo basename.
- `auto` does not overwrite a manually-set label.
- `--clear` removes all `@tmux_picker_*` keys.
- `label` on a non-existent session exits non-zero.

E2E in `tests/e2e.sh`:

- `tmux-picker label`, `show`, `auto` round-trip on a real session.
- `tmux-picker --help` exits 0 and mentions all subcommands.

## Edge Cases

| Case | Behavior |
|---|---|
| Session has no metadata | Picker shows bare name, no detail row. |
| Label contains pipe `\|` | tmux `-F` outputs the literal pipe in field, our parser uses `splitn(8, '\|')` — labels with pipes will corrupt parse. **Mitigation:** the `label` subcommand rejects values containing `\|` with a clear error. Documented limitation. |
| Label is very long (>80 chars) | TUI truncates at column width with `…`. Stored value is unchanged. |
| Session name itself contains `\|` | Already not allowed by `validate_session_name`. No new exposure. |
| `auto` on a session in `$HOME` (no git repo) | project = `$HOME`, label = `home` (basename). Acceptable. |
| `auto` and `pane_current_path` returns deleted dir | project still set to that path; auto-detect does not stat. Documented. |
| Multiple panes / windows in session | `auto` uses session's first window's first pane (`<sess>:0.0`). Documented. Future enhancement: heuristic to pick "active" pane. |
| `--clear` on a session with no metadata | No-op, exit 0. |
| User runs `tmux-picker label <sess>` with no flags | Error: "no fields to set; pass --label, --project, --purpose, or --clear". |

## Backward Compatibility

- TUI behavior with no subcommand is byte-for-byte identical to v1.0.0 except
  for the new label-aware name display and the optional detail row. Both are
  invisible until metadata exists, so an upgrade-in-place user sees no change
  until they start setting labels.
- The action protocol on stdout (`attach:<name>`, `new:<name>`, `shell`) is
  unchanged. Shell stub does not need changes.
- Existing tests continue to pass — the 4-field `parse_session_line` becomes
  8-field but maintains backward compat by treating missing fields as `None`
  (verified via test).

## What This Sub-Project Does NOT Cover

(Explicit scope guard — these belong to later sub-projects.)

- Showing git branch / dirty status from real-time git inspection (#2).
- Preview pane of session contents (#2).
- Fuzzy filter / search (#3).
- Kill / rename / detach-others from picker (#3).
- Config file, themes, keybindings (#4).
- Action hooks on attach/new (#4).

## Risks

- **Tmux version skew.** `@user_option` interpolation in `-F` exists since
  tmux 3.0 (2019). Fedora 43 ships 3.5, fine. Documented minimum.
- **Field-count change in parser.** The existing parser uses `splitn(4, '|')`.
  Bumping to `splitn(8, '|')` is a one-line change but every test of
  `parse_session_line` needs to either pass 8-field input or rely on the
  trailing-fields-optional guarantee. Plan: keep accepting 4-field input
  (treat missing as None) so the change is purely additive.
- **clap dependency.** Adds ~150 KB binary growth and a tree of crates. The
  alternative (hand-rolled parser for 4 subcommands with ~10 flags total) is
  ~80 LOC of fiddly code with worse error messages. Going with clap.

## Validation Checklist (pre-merge)

- [ ] `cargo fmt --check && cargo clippy -- -D warnings && cargo test` clean.
- [ ] `bash tests/e2e.sh` clean.
- [ ] Manual: launch picker with no metadata anywhere — same as v1.0.0.
- [ ] Manual: set label/project/purpose, relaunch picker, verify display.
- [ ] Manual: `tmux-picker auto` from inside a git repo session, verify
      project + label set correctly.
- [ ] Manual: full uninstall + reinstall via `scripts/uninstall.sh` and
      `scripts/install.sh` (delivered as part of the rollout, separate from
      this sub-project's source changes).
