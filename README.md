# tmux-picker

[MIT licensed](#license) — free to use, modify, and redistribute, provided
**as-is with no warranty**; you're responsible for how you configure and run
it. Installing via a human's hands or an AI agent are both fine — see
[Install with an AI coding agent](#install-with-an-ai-coding-agent-claude-code--codex)
for the one rule that keeps that safe.

A Rust TUI session picker for `tmux`. It runs the moment you open a shell —
SSH login or a local terminal window, whichever emulator you use — shows
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

- Fires on every new interactive shell by default — SSH login **and** local
  terminals (GNOME Terminal, Konsole, kitty, Alacritty, xterm, iTerm2, …),
  no per-terminal setup needed. Restrict it back to SSH-only via
  `trigger_mode` — see [Configuration](#configuration).
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
`tmux-picker` binary — see [Enable the auto-attach hook](#enable-the-auto-attach-hook)
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
installs the auto-attach hook in one shot. From inside a checkout:

```bash
bash scripts/install.sh
```

The first thing it does — before touching any file — is ask **you** where
the picker should run:

```
Where should tmux-picker auto-run?
  1) Everywhere  — SSH logins AND local terminal windows (any emulator)
  2) SSH only    — the original behaviour; local terminals are untouched

Choose [1/2] (default: 1):
```

Press Enter for "everywhere" (a workstation you sit at), or `2` for
SSH-only (a headless server / shared box you'd rather not have grabbing
every local login shell on). You can change your mind later without
reinstalling — see `trigger_mode` in [Configuration](#configuration).

Prefer not to be asked (scripting a fresh machine, cloud-init, etc.)? Pass
the choice up front and the prompt is skipped:

```bash
bash scripts/install.sh --trigger-mode=always     # SSH + local terminals
bash scripts/install.sh --trigger-mode=ssh_only   # SSH only
```

The script builds the release binary, installs it to
`~/.local/bin/tmux-picker`, copies `shell/tmux-autoattach.sh` to
`~/.bashrc.d/tmux-autoattach.sh`, writes your `trigger_mode` choice to
`~/.config/tmux-picker/config.toml`, and runs a sanity check. It's
idempotent — re-run it any time (with a new `--trigger-mode` if you want to
change it) to upgrade after `git pull`.

**If run non-interactively with no `--trigger-mode` flag** (no real
terminal attached to stdin/stdout — the case an AI agent running the
installer for you will usually hit), the script refuses to guess and exits
with instructions to re-run with an explicit flag. That's intentional: this
is a "will this run on every terminal I open" decision for you to make, not
one the installer — or an AI acting on your behalf — should make silently.

## Enable the auto-attach hook

The hook fires on **every new interactive bash shell** — an SSH login or a
local terminal window (any emulator: GNOME Terminal, Konsole, kitty,
Alacritty, xterm, iTerm2, Terminal.app, …) — as long as your `~/.bashrc`
sources `~/.bashrc.d/*.sh`. Fedora/RHEL ship that by default; other distros
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

Then open a new SSH session, or just a fresh local terminal window — either
way you should land straight in the picker. To skip it once (e.g. for a
script or a `scp`-only session), set `NO_TMUX=1` before connecting, or
`export NO_TMUX=1` in an existing shell before re-sourcing.

Only want the picker over SSH, like the original behaviour? Set
`trigger_mode = "ssh_only"` in `~/.config/tmux-picker/config.toml` — see
[Configuration](#configuration).

**zsh / fish users:** the hook is bash-specific (`[[ ]]`, `$SSH_CONNECTION`/
`$TMUX` semantics) and a plain `source` from zsh/fish won't parse. Bridging
it by shelling out to `bash -ic '...; exit'` from `~/.zshrc` is the general
pattern, but this project only tests the bash/`~/.bashrc.d` path documented
above — if you're on zsh or fish, that's the supported route; treat any
bridge as unverified and expect to debug it yourself.

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

Using an AI agent to run this installer is explicitly supported and
allowed under the MIT license below — go ahead. The one rule: **the agent
must ask you `always` vs `ssh_only` and wait for your answer before running
`scripts/install.sh`.** It must not guess on your behalf. This isn't just a
suggestion — `scripts/install.sh` itself enforces it: run non-interactively
without `--trigger-mode`, and it refuses to install anything and prints
instructions telling the agent to stop and ask you. That's what makes it
safe to hand this whole task to an agent unattended: the worst case is it
stops and asks, never that it silently makes local terminals do something
you didn't want.

Both prompts below tell the agent to ask first. Paste one in as-is — both
agents can read this README straight from the repo, so a short pointer is
enough; they'll pick up the exact commands above rather than improvising.

### Claude Code

```
Install tmux-picker from https://github.com/jalsarraf0/tmux-picker by
following that repo's README.md exactly:
1. Clone it into ~/git/tmux-picker (or ~/src if ~/git doesn't exist).
2. Before running anything, ask me: "Should tmux-picker run on every local
   terminal too, or only over SSH?" Wait for my answer.
3. Run `scripts/install.sh --trigger-mode=always` or
   `scripts/install.sh --trigger-mode=ssh_only` to match what I said — do
   not run install.sh without that flag, and do not choose for me.
4. Check whether ~/.bashrc sources ~/.bashrc.d/*.sh; if not, add the
   snippet from the README's "Enable the auto-attach hook" section.
5. Run `tmux-picker --version` and `tmux-picker --check-config` to confirm
   the install is healthy, and confirm `tmux-picker --print-trigger-mode`
   matches what I chose.
6. Tell me whether it's ready and what, if anything, needs a fresh SSH
   session or terminal window to take effect.
Don't touch any other dotfiles or existing tmux sessions.
```

### Codex CLI

```
codex exec "Before doing anything, ask the user: 'Should tmux-picker run on
every local terminal, or only over SSH?' and wait for the answer — do not
guess. Once answered, clone https://github.com/jalsarraf0/tmux-picker,
follow its README.md to build and install tmux-picker via
'scripts/install.sh --trigger-mode=always' or
'scripts/install.sh --trigger-mode=ssh_only' matching the answer (never run
install.sh without that flag), ensure ~/.bashrc sources ~/.bashrc.d/*.sh
(adding the README's snippet if it doesn't), then verify with
'tmux-picker --version' and 'tmux-picker --print-trigger-mode'. Report
success/failure and any manual step still needed (e.g. starting a new SSH
session or terminal window)."
```

Provided as-is, MIT-licensed, no warranty — see [License](#license). You
(and whichever agent you delegate to) are responsible for the `trigger_mode`
you pick and how this ends up configured on your machines.

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

# When the shell hook fires. "always" (default) runs on every new
# interactive shell, SSH or local terminal alike. "ssh_only" restores the
# original SSH-only behaviour.
trigger_mode = "always"

[theme]
accent = "cyan"            # numbers, attached marker, prompt accents
warning = "red"            # kill-confirm prompt, error highlights
selection_bg = "darkgray"  # highlighted row background
```

`trigger_mode` is read by the shell hook itself (via the internal
`tmux-picker --print-trigger-mode` plumbing flag) before it decides whether
to run on a non-SSH shell, so a `ssh_only` override takes effect on the very
next new terminal — no reinstall needed.

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

- **Nothing happens on SSH login or in a new local terminal** — check
  `~/.bashrc` sources `~/.bashrc.d/*.sh` (see above), and that you're in an
  *interactive* shell (`echo $-` should contain `i`). For SSH specifically,
  also check `echo $SSH_CONNECTION` is non-empty.
- **Local terminals don't trigger it, but SSH does** — check
  `tmux-picker --print-trigger-mode`; if it prints `ssh_only`, either
  remove/comment `trigger_mode` in `~/.config/tmux-picker/config.toml` or
  set it to `"always"`.
- **"tmux not found at /usr/bin/tmux"** — the hook hardcodes that path;
  symlink your tmux there or edit `_TMUX` in
  `~/.bashrc.d/tmux-autoattach.sh`.
- **Picker pegs CPU / won't exit** — this was a real bug (detached-PTY
  wedge) fixed in the commit tagged `fix: detached-pty wedge`; make sure
  you're on a version at or after that fix.
- **Want to skip the picker just once** — `NO_TMUX=1 ssh host`, or
  `export NO_TMUX=1` before opening a local terminal tab/window.

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

MIT — see [`LICENSE`](LICENSE). In short: free to use, copy, modify,
merge, publish, distribute, sublicense, and sell, with attribution
(keep the copyright notice), and:

> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

That applies to the auto-attach hook and the trigger-mode behaviour above,
too — you (or your AI agent) choosing `trigger_mode` and running the
installer is done at your own discretion and risk; the author takes no
responsibility for how it's configured or used on your systems.
