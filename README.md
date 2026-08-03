# tmux-picker

[MIT licensed](#license) — free to use, modify, and redistribute, provided
**as-is with no warranty**; you're responsible for how you configure and run
it. Installing via a human's hands or an AI agent are both fine — see
[Install with an AI coding agent](#install-with-an-ai-coding-agent-claude-code--codex)
for the one rule that keeps that safe. See [`CHANGELOG.md`](CHANGELOG.md)
for what's new in each release.

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

Missing `tmux` or `cargo`? `scripts/install.sh` can install both for you —
see `--auto-deps` under [Everything at once](#everything-at-once-recommended)
below. It's opt-in for the same reason `trigger_mode` is: installing system
packages (possibly via `sudo`) isn't something the installer should do
without asking first.

## Install (by hand)

Pick whichever matches how you manage binaries. All of these only install the
`tmux-picker` binary — see [Enable the auto-attach hook](#enable-the-auto-attach-hook)
below to make it run automatically on login.

### From crates.io

```bash
cargo install tmux-picker
```

> Not published yet as of the last update to this section — `cargo publish`
> is blocked on an expired/invalid token, unrelated to the code. Use one of
> the options below in the meantime; this will be updated once it's live.

### Pre-built binary (cargo-binstall)

```bash
cargo binstall tmux-picker
```

### Fedora / RHEL / openSUSE (.rpm)

```bash
curl -LO https://github.com/jalsarraf0/tmux-picker/releases/download/v1.2.1/tmux-picker-1.2.1-1.x86_64.rpm
sudo dnf install ./tmux-picker-1.2.1-1.x86_64.rpm   # or: sudo zypper install ./tmux-picker-*.rpm
```

Wires the auto-attach hook into `/etc/bashrc` automatically (post-install
script) — no manual "enable the hook" step needed. Default `trigger_mode`
is `"always"`; see [Configuration](#configuration) to restrict to SSH only.

### Debian / Ubuntu (.deb)

```bash
curl -LO https://github.com/jalsarraf0/tmux-picker/releases/download/v1.2.1/tmux-picker_1.2.1_amd64.deb
sudo apt install ./tmux-picker_1.2.1_amd64.deb
```

Same deal — wires itself into `/etc/bash.bashrc` automatically on install.

### Arch Linux (AUR)

`packaging/PKGBUILD` + `packaging/tmux-picker.install` build the
`tmux-picker-bin` package (both files are required — the `.install` file is
what wires the hook into `/etc/bash.bashrc`):

```bash
git clone https://github.com/jalsarraf0/tmux-picker.git
cd tmux-picker/packaging
makepkg -si
```

Not yet submitted to the real AUR (needs an AUR account + registered SSH
key this environment doesn't have) — building locally from the repo works
today; `pacman -U` the resulting package or use the `makepkg -si` above.

### macOS / Linuxbrew (Homebrew)

No tap yet, so install straight from the formula file:

```bash
brew install --formula https://raw.githubusercontent.com/jalsarraf0/tmux-picker/main/packaging/homebrew/tmux-picker.rb
```

Builds from source via `cargo` (a couple of minutes). **Not build-tested on
real macOS** — the dependencies (ratatui/crossterm/etc.) all support macOS,
but this formula hasn't been run there yet; treat it as best-effort until
someone confirms. Homebrew formulae don't touch dotfiles, so you'll need to
add the sourcing line yourself — `brew info tmux-picker` after install shows
the exact snippet (also in `packaging/homebrew/tmux-picker.rb`'s `caveats`).

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

### Missing `tmux` or `cargo`? (`--auto-deps`)

If either is missing, the same pattern applies: interactively, you're asked
`Install missing dependencies now (may use sudo)? [y/N]`; non-interactively,
the script refuses and tells you (or your AI agent) to re-run with a flag:

```bash
bash scripts/install.sh --auto-deps      # install missing tmux/cargo automatically
bash scripts/install.sh --no-auto-deps   # skip; fail on missing cargo, warn on missing tmux
```

`--auto-deps` detects your package manager (`apt`, `dnf`, `pacman`,
`zypper`, `apk`, or `brew`) and installs `tmux` through it (via `sudo`
unless already root), and installs a Rust toolchain through
[rustup](https://rustup.rs) if `cargo` is missing — distro `cargo` packages
are frequently too old for this project's `edition = "2024"`, so rustup
(not the system package) is what gets used for that one.

Fully hands-off — no prompts at all, e.g. cloud-init or a first-boot script:

```bash
bash scripts/install.sh --trigger-mode=always --auto-deps
```

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
allowed under the MIT license below — go ahead. The rule: **the agent must
ask you (1) `always` vs `ssh_only`, and (2) whether it's OK to auto-install
`tmux`/a Rust toolchain if either is missing, and wait for both answers
before running `scripts/install.sh`.** It must not guess either on your
behalf. This isn't just a suggestion — `scripts/install.sh` itself enforces
both: run non-interactively without `--trigger-mode`, and it refuses and
prints instructions telling the agent to stop and ask about that; hit a
missing `tmux`/`cargo` non-interactively without `--auto-deps`/
`--no-auto-deps`, and it refuses the same way. That's what makes it safe to
hand this whole task to an agent unattended: the worst case is it stops and
asks, never that it silently changes what every terminal does or reaches
for `sudo` on its own.

Both prompts below tell the agent to ask first. Paste one in as-is — both
agents can read this README straight from the repo, so a short pointer is
enough; they'll pick up the exact commands above rather than improvising.

### Claude Code

```
Install tmux-picker from https://github.com/jalsarraf0/tmux-picker by
following that repo's README.md exactly:
1. Clone it into ~/git/tmux-picker (or ~/src if ~/git doesn't exist).
2. Before running anything, ask me two things and wait for both answers:
   a. "Should tmux-picker run on every local terminal too, or only over
      SSH?"
   b. "If tmux or a Rust toolchain (cargo) is missing, OK to install them
      automatically (may use sudo)?"
3. Run `scripts/install.sh --trigger-mode=always` or
   `scripts/install.sh --trigger-mode=ssh_only` to match (a), plus
   `--auto-deps` or `--no-auto-deps` to match (b) — e.g.
   `scripts/install.sh --trigger-mode=always --auto-deps`. Do not run
   install.sh without both flags, and do not choose either for me.
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
codex exec "Before doing anything, ask the user two things and wait for
both answers — do not guess either: (a) 'Should tmux-picker run on every
local terminal, or only over SSH?' and (b) 'If tmux or a Rust toolchain is
missing, OK to install them automatically (may use sudo)?'. Once answered,
clone https://github.com/jalsarraf0/tmux-picker, follow its README.md to
build and install tmux-picker via scripts/install.sh, passing
--trigger-mode=always or --trigger-mode=ssh_only to match (a) and
--auto-deps or --no-auto-deps to match (b) (never run install.sh without
both flags), ensure ~/.bashrc sources ~/.bashrc.d/*.sh (adding the
README's snippet if it doesn't), then verify with 'tmux-picker --version'
and 'tmux-picker --print-trigger-mode'. Report success/failure and any
manual step still needed (e.g. starting a new SSH session or terminal
window)."
```

Provided as-is, MIT-licensed, no warranty — see [License](#license). You
(and whichever agent you delegate to) are responsible for the `trigger_mode`
and `--auto-deps` choices you make and how this ends up configured on your
machines.

Either agent can do this unattended in a normal user account. With
`--no-auto-deps` (or when nothing's missing), the installer only touches
`~/.local/bin`, `~/.bashrc.d`, `~/.config/tmux-picker`, and — if
missing — appends the sourcing snippet to `~/.bashrc`; it never needs root.
With `--auto-deps` and something actually missing, it will invoke your
package manager (via `sudo`, unless already root) to install `tmux`, and/or
run the official `rustup` installer to get a Rust toolchain — see
[Missing tmux or cargo?](#missing-tmux-or-cargo---auto-deps) above for
exactly what that runs.

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
- **`install.sh` says cargo/tmux is missing and refuses to proceed** — either
  install them yourself and re-run, or re-run with
  `--auto-deps` to have it install them for you (may use `sudo`) — see
  [Missing tmux or cargo?](#missing-tmux-or-cargo---auto-deps).
- **`--auto-deps` failed partway through** — it shells out to your real
  package manager and to `rustup`; check their own error output (printed
  as-is, not swallowed). Re-running `--auto-deps` is safe — both
  `dnf`/`apt`/etc. installs and the rustup installer are idempotent.
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
4. Tag and push: `git tag -s -m "Release v$VERSION" v$VERSION && git push origin v$VERSION`.
5. Build the per-target release tarball and attach it to the GitHub release
   so `cargo binstall` and the AUR `PKGBUILD` can find it. It must contain
   `tmux-picker`, a **flat** `tmux-autoattach.sh` (not nested under
   `shell/`), `LICENSE`, and `README.md`, all directly inside a directory
   named `tmux-picker-$VERSION-$TARGET/`.
6. Build the native packages: `bash packaging/build-native-packages.sh`
   (needs [`fpm`](https://fpm.readthedocs.io) — produces both `.deb` and
   `.rpm` into `dist/`). Sanity-check them before attaching — the
   post-install/pre-remove scripts wire and unwire a system-wide bashrc
   block, so at minimum verify in a throwaway container:
   ```bash
   docker run --rm -v "$PWD/dist:/pkgs:ro" fedora:latest \
     bash -c 'dnf install -y /pkgs/tmux-picker-*.rpm && grep -A2 "BEGIN tmux-picker" /etc/bashrc'
   docker run --rm -v "$PWD/dist:/pkgs:ro" debian:latest \
     bash -c 'apt-get update -qq && apt-get install -y /pkgs/tmux-picker_*.deb && grep -A2 "BEGIN tmux-picker" /etc/bash.bashrc'
   ```
   Also check the *reinstall/upgrade* case doesn't strip the block (the
   `--before-remove` script only strips on a genuine removal — rpm passes
   `$1=0` for that, deb passes `remove`/`purge` — never on upgrade).
7. `gh release create v$VERSION dist/*.deb dist/*.rpm tmux-picker-$VERSION-$TARGET.tar.gz ...`
   (or `gh release upload` onto an already-created release).
8. `cargo publish` to crates.io.
9. Submit / refresh the AUR package — clone
   `ssh://aur@aur.archlinux.org/tmux-picker-bin.git`, copy in `PKGBUILD` +
   `tmux-picker.install`, regenerate `.SRCINFO`
   (`makepkg --printsrcinfo > .SRCINFO`), commit, push. Requires an AUR
   account with a registered SSH key.

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
