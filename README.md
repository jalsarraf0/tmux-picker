# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints
the chosen action to stdout. A bash stub (`shell/tmux-autoattach.sh`) reads
stdout and execs tmux (or drops to a shell if the user picked `s`).

## Features

- Color-coded session list with attached indicator, command, and idle time.
- 10-second auto-attach to the most recent detached session (configurable).
- Numbered + arrow + j/k navigation.
- New-session input with name validation.
- Per-session metadata (label / project / purpose) stored as tmux user-options.
- Label-aware display: shows `label (session-name)` when a label is set, plus
  a project-path / purpose detail line under the selected session.
- Preview pane: last 6 lines of the highlighted session's active pane.
- Live fuzzy filter (`/`): subsequence match on session name, label, and
  project; matches are ordered by score so the closest hit floats to the top.
  `Enter` keeps the filter active, `Esc` clears it.
- Kill session (`K`) with a `y`/`Y` confirmation prompt; any other key cancels.
- Quick ops: rename (`r`), sort cycle (`o`), yank session name to clipboard (`y`).
- Process markers (🤖 claude, ✏️ vim/nvim, 📊 htop, 🦀 cargo, 📦 node/npm,
  🐍 python, 🌿 git) and a coloured activity dot for at-a-glance status.
- `Tab` toggles between the 6-line preview and a multi-window summary
  showing each window's last non-blank line.
- Mouse: click to select, double-click to attach.
- `SIGHUP` reloads `~/.config/tmux-picker/config.toml` in place — change a
  theme colour or marker map without restarting the picker.
- Optional user config at `~/.config/tmux-picker/config.toml` for the auto-attach
  timeout, theme colors, and process markers.

## Install

### From crates.io

```bash
cargo install tmux-picker
```

### Pre-built binary (cargo-binstall)

```bash
cargo binstall tmux-picker
```

### Arch Linux (AUR)

A `PKGBUILD` ships under `packaging/PKGBUILD` for the `tmux-picker-bin`
package; clone it and build with `makepkg -si`.

### Build from source

```bash
cargo build --release
cp target/release/tmux-picker ~/.local/bin/
```

Or use `scripts/install.sh` for a complete setup that includes the shell hook.

### First-run config

```bash
tmux-picker --init    # writes ~/.config/tmux-picker/config.toml
```

The starter file documents every available knob; pass `--force` to
overwrite a hand-edited config.

## CLI

```
tmux-picker                       Run the picker TUI (default)
tmux-picker label <session> ...   Set or clear session metadata
tmux-picker show  <session>       Print session metadata as TOML
tmux-picker auto  <session>       Auto-detect from session's pane cwd
tmux-picker --help / --version
```

### `label`

```
tmux-picker label <session> [flags]
  --label   <text>    Human label, e.g. "Refactoring auth middleware"
  --project <path>    Project root, e.g. "/home/u/git/app"
  --purpose <text>    Short purpose, e.g. "PR #234"
  --clear             Remove all tmux-picker metadata for this session
```

`--clear` is mutually exclusive with the other flags. Pipe characters (`|`)
are rejected in any value because the underlying `list-sessions` parser is
pipe-delimited.

### `show`

Prints metadata as TOML. Empty metadata prints just the `session = "..."`
line. Errors if the session does not exist.

```toml
session = "claude-app"
label = "Refactoring auth"
project = "/home/u/git/app"
purpose = "PR #234"
label_at = 1746201600
```

### `auto`

Reads the session's active-pane working directory, walks up to the nearest
`.git`, and sets:

- `project` → git root (or pane cwd if no `.git`)
- `label` → `basename(project)` *only if no label is set*
- `purpose` → `branch:<current-branch>` *only if no purpose is set and the
  project is a git repo*

Manually-set values are never overwritten.

## Claude Code integration

Inside a tmux session running Claude Code, the assistant can update its own
session label as it works:

```bash
tmux-picker label "$(tmux display -p '#S')" \
    --label "Investigating runner queue depth"
```

Or auto-derive from cwd:

```bash
tmux-picker auto "$(tmux display -p '#S')"
```

## Storage model

Metadata lives in tmux user-options on each session:

| Key | Meaning |
|---|---|
| `@tmux_picker_label` | Human label string |
| `@tmux_picker_project` | Absolute project path |
| `@tmux_picker_purpose` | Short purpose string |
| `@tmux_picker_label_at` | Epoch seconds when last updated |

`tmux kill-session` removes the metadata along with the session. There is no
external state file.

## Configuration

Optional TOML at `~/.config/tmux-picker/config.toml` (or
`$XDG_CONFIG_HOME/tmux-picker/config.toml` if set). All keys are optional, the
file may be absent, and parse errors fall back to defaults with a single
stderr warning — login auto-attach is never blocked by a malformed config.

```toml
# Auto-attach countdown for the most recent detached session, in seconds.
# 0 disables auto-attach (the picker waits for a manual choice).
timeout_secs = 10

[theme]
accent = "cyan"            # numbers, attached marker, prompt accents
warning = "red"            # kill-confirm prompt, error highlights
selection_bg = "darkgray"  # highlighted row background
```

Recognized color names (case-insensitive): `black`, `red`, `green`, `yellow`,
`blue`, `magenta`, `cyan`, `white`, `darkgray` (aliases: `gray`, `grey`),
`lightred`, `lightgreen`, `lightyellow`, `lightblue`, `lightmagenta`,
`lightcyan`. Hex (`"#rrggbb"` or `"#rgb"`) and 256-colour indexes (`"196"`
or `196`) are also accepted. Unknown values log a warning to stderr and
keep that field's default.

`tmux-picker --check-config` prints the parse warnings plus the effective
config so you can debug overrides without launching the picker.

### Process markers

Override or extend the built-in glyph table:

```toml
[markers]
# Drop the built-ins entirely; only your patterns apply.
disable_defaults = false

[markers.patterns]
foo = "★"     # any pane running a command containing "foo" gets ★
"my-tool" = "🚀"
```

Built-in defaults: `claude → 🤖`, `vim`/`nvim → ✏️`, `htop`/`btop`/`top → 📊`,
`cargo`/`rustc → 🦀`, `npm`/`pnpm`/`node → 📦`, `python → 🐍`, `git → 🌿`.

Send `SIGHUP` to a running picker (`kill -HUP $(pgrep tmux-picker)`) to
re-read the config without restarting.

## Test

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
bash tests/e2e.sh
```

## Releasing (maintainer notes)

1. Bump `version` in `Cargo.toml`.
2. Update `packaging/PKGBUILD`'s `pkgver` to match.
3. `cargo publish --dry-run` and resolve any warnings.
4. Tag and push: `git tag v$VERSION && git push --tags`.
5. Build per-target tarballs and attach them to the GitHub release so
   `cargo binstall` and the AUR `PKGBUILD` can find them. Each tarball
   should contain `tmux-picker`, `shell/tmux-autoattach.sh`, `LICENSE`,
   and `README.md` under a directory named
   `tmux-picker-$VERSION-$TARGET/`.
6. `cargo publish` to crates.io.
7. Submit / refresh the AUR package.

## License

MIT — see [`LICENSE`](LICENSE).
