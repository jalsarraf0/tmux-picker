# tmux-picker

A Rust TUI session picker for `tmux`. It runs the moment you SSH in, shows
every tmux session with live status (attached/detached, running command,
idle time, per-session label/project/purpose), and lands you in the right
one — or a fresh one — without you ever touching `tmux ls`.

The picker renders to `stderr` and prints the chosen action to `stdout`. A
small bash hook (`shell/tmux-autoattach.sh`) reads that action and execs
`tmux attach` / `tmux new-session` (or drops to a bare shell if you
explicitly ask for one). Everything is designed to fail open: if the binary
is missing, misconfigured, or errors out, the hook falls back to a plain
`tmux new-session -As main` rather than leaving you stranded.

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

## Requirements

- Linux or macOS with `bash` and `tmux` installed.
- A Rust toolchain (`cargo` — stable, 2024 edition support) **only** if you're
  building from source. Pre-built binaries and crates.io don't need this on
  the install path, but `cargo install`/`cargo binstall` still shell out to
  cargo.
- SSH access to the box you want session-picking on (this is meant to run on
  every SSH login, but works fine as a plain interactive-shell tool too).

## Install (by hand)

Pick whichever matches how you manage binaries. All of these only install the
`tmux-picker` binary — see [Enable the SSH auto-attach hook](#enable-the-ssh-auto-attach-hook)
below to make it run automatically on login.

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

### Build from source, step by step

```bash
git clone https://github.com/jalsarraf0/tmux-picker.git
cd tmux-picker
cargo build --release
install -m 0755 target/release/tmux-picker ~/.local/bin/tmux-picker
```

Make sure `~/.local/bin` is on your `PATH` (`echo $PATH | tr ':' '\n' | grep local/bin`).
Confirm it works:

```bash
tmux-picker --version
```

### Everything at once (recommended)

`scripts/install.sh` does the clone-free equivalent of the steps above *and*
installs the SSH auto-attach hook in one shot. From inside a checkout:

```bash
bash scripts/install.sh
```

It builds the release binary, installs it to `~/.local/bin/tmux-picker`,
copies `shell/tmux-autoattach.sh` to `~/.bashrc.d/tmux-autoattach.sh`, and
runs a sanity check (`tmux-picker --version`). It's idempotent — re-run it
any time to upgrade after `git pull`.

## Enable the SSH auto-attach hook

The hook only fires if your `~/.bashrc` actually sources
`~/.bashrc.d/*.sh`. Fedora/RHEL ship this by default; other distros
(Debian/Ubuntu, Arch, macOS+bash) usually don't, so add it once:

```bash
grep -q 'bashrc.d' ~/.bashrc || cat >> ~/.bashrc <<'EOF'

# Source drop-in scripts (tmux-picker, etc.)
if [ -d ~/.bashrc.d ]; then
  for rc in ~/.bashrc.d/*.sh; do
    [ -r "$rc" ] && . "$rc"
  done
fi
EOF
```

Then open a new SSH session — you should land straight in the picker. To
skip it once (e.g. for a script or a `scp`-only session), set `NO_TMUX=1`
before connecting, or `export NO_TMUX=1` in an existing shell before
re-sourcing.

### First-run config

```bash
tmux-picker --init    # writes ~/.config/tmux-picker/config.toml
```

The starter file documents every available knob; pass `--force` to
overwrite a hand-edited config. See [Configuration](#configuration) for the
full key reference.

### Verifying it's working

```bash
tmux-picker --check-config   # parse warnings + effective config, no TUI
tmux new-session -d -s smoke # create a detached session to pick from
# then open a fresh SSH connection and confirm "smoke" shows up
```

### Uninstall

```bash
rm -f ~/.local/bin/tmux-picker ~/.bashrc.d/tmux-autoattach.sh
rm -rf ~/.config/tmux-picker
```

Existing tmux sessions and their user-option metadata are untouched; they
just won't be picked anymore.

## Install with an AI coding agent (Claude Code / Codex)

If you'd rather have an agent do the clone-build-install-verify sequence
(and safely handle the `~/.bashrc.d` sourcing check for your specific
shell setup), hand it one of these prompts. Both agents can read this
README directly from the repo, so a short pointer is enough — they'll pick
up the exact commands above rather than improvising.

### Claude Code

```
Install tmux-picker from https://github.com/jalsarraf0/tmux-picker by
following that repo's README.md exactly:
1. Clone it into ~/git/tmux-picker (or ~/src if ~/git doesn't exist).
2. Run scripts/install.sh.
3. Check whether ~/.bashrc sources ~/.bashrc.d/*.sh; if not, add the
   snippet from the README's "Enable the SSH auto-attach hook" section.
4. Run `tmux-picker --version` and `tmux-picker --check-config` to confirm
   the install is healthy.
5. Tell me whether it's ready and what, if anything, needs a fresh SSH
   session to take effect.
Don't touch any other dotfiles or existing tmux sessions.
```

### Codex CLI

```
codex exec "Clone https://github.com/jalsarraf0/tmux-picker, follow its
README.md to build and install tmux-picker via scripts/install.sh, ensure
~/.bashrc sources ~/.bashrc.d/*.sh (adding the README's snippet if it
doesn't), then verify with tmux-picker --version. Report success/failure
and any manual step I still need to do (e.g. starting a new SSH session)."
```

Either agent should be able to do this unattended in a normal user account —
the installer only touches `~/.local/bin`, `~/.bashrc.d`, `~/.config/tmux-picker`,
and (if missing) appends the sourcing snippet to `~/.bashrc`. It never needs
root and never modifies system files.

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

## Troubleshooting

- **Nothing happens on SSH login** — check `~/.bashrc` sources
  `~/.bashrc.d/*.sh` (see above), and that you're in an *interactive* shell
  over SSH (`echo $SSH_CONNECTION`, `echo $-` should contain `i`).
- **"tmux not found at /usr/bin/tmux"** — the hook hardcodes that path;
  symlink your tmux there or edit `_TMUX` in
  `~/.bashrc.d/tmux-autoattach.sh`.
- **Picker pegs CPU / won't exit** — this was a real bug (detached-PTY
  wedge) fixed in the commit tagged `fix: detached-pty wedge`; make sure
  you're on a version at or after that fix.
- **Want to skip the picker just once** — `NO_TMUX=1 ssh host`.

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
