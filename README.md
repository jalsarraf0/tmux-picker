<div align="center">

# tmux-picker

*The login picker for tmux. SSH in, land in the right session.*

[![CI](https://github.com/jalsarraf0/tmux-picker/actions/workflows/ci.yml/badge.svg)](https://github.com/jalsarraf0/tmux-picker/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jalsarraf0/tmux-picker?label=release)](https://github.com/jalsarraf0/tmux-picker/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![built with ratatui](https://img.shields.io/badge/built%20with-ratatui-1fa8c9)](https://github.com/ratatui/ratatui)

</div>

No more `tmux ls`, no more guessing which of your twelve unnamed sessions
has the thing you were doing. `tmux-picker` fires the moment you open a
shell — SSH login or a local terminal, any emulator — shows every tmux
session with live status, and lands you in the right one, or a fresh one,
without you lifting a finger.

```
┌ tmux sessions ──────────────────────────────────────────────────┐
│                                                                    │
│  1  🤖 auth-refactor (api)          3w   ●  claude       2m idle  │
│     ↳ ~/git/api  ·  PR #234                                      │
│  2  🦀 tmux-picker                  2w   ●  cargo         14s idle│
│     ↳ ~/git/tmux-picker  ·  branch:optimize                      │
│  3     scratch                      1w      bash          4h idle│
│  4  ✏️  notes                        1w      nvim         1d idle │
│                                                                    │
├ preview (Tab: windows) ───────────────────────────────────────────┤
│  $ cargo test --release                                          │
│  running 84 tests                                                │
│  test result: ok. 84 passed; 0 failed                            │
└────────────────────────────────────────────────────────────────┘
  ↑↓ move   ⏎ attach   / filter   n new   K kill   r rename   ? help
```

## Why not `tmux ls` / a shell alias / fzf one-liner?

|                                     | `tmux ls`     | fzf + `tmux ls` script | [sesh](https://github.com/joshmedeski/sesh) | [tmux-sessionizer](https://github.com/ThePrimeagen/tmux-sessionizer) | **tmux-picker** |
|-------------------------------------|:--------------:|:-----------------------:|:--:|:--:|:--:|
| Fires automatically on login        | ✗              | ✗                       | ✗ | ✗ | ✅ |
| Works over SSH *and* local terminals| ✗              | ✗                       | ✗ | ✗ | ✅ |
| Fuzzy filter                        | ✗              | ✅ (needs `fzf`)         | ✅ | ✅ (needs `fzf`) | ✅ (built in, no `fzf`) |
| Per-session label / project / purpose | ✗            | ✗                       | partial | ✗ | ✅ (in tmux user-options, no state file) |
| Live pane preview                   | ✗              | ✗                       | ✅ | ✗ | ✅ |
| Extra runtime deps                  | none           | `fzf`                   | `fzf`, zoxide | `fzf`, zoxide | just `tmux` |

## Features

- **Auto-fires on login** — SSH and local terminals alike (GNOME Terminal,
  Konsole, kitty, Alacritty, xterm, iTerm2, …), zero per-terminal setup.
  Restrict to SSH-only with one config key if you'd rather.
- **Built-in fuzzy filter (`/`)** — subsequence match on name, label, and
  project, ranked by score. No `fzf` dependency.
- **Per-session context** — label, project path, and purpose live directly
  in tmux user-options (`tmux-picker label/show/auto`); no external state
  file to get out of sync.
- **Live pane preview** — last 6 lines of the highlighted session, or `Tab`
  for a per-window summary.
- **Process markers at a glance** — 🤖 claude, 🦀 cargo, ✏️ vim/nvim, 📊 htop,
  📦 node, 🐍 python, 🌿 git — fully customizable.
- **Quick ops** — kill (`K`, confirm-gated), rename (`r`), yank name to
  clipboard (`y`), sort cycle (`o`).
- **10-second auto-attach** to the most recently active detached session —
  tunable, or disable for a manual-only picker.
- **Fails open** — binary missing, misconfigured, or erroring never strands
  you; the shell hook falls back to a plain `tmux new-session -As main`.

## Quickstart

```bash
cargo install --git https://github.com/jalsarraf0/tmux-picker --locked
bash <(curl -fsSL https://raw.githubusercontent.com/jalsarraf0/tmux-picker/main/scripts/install.sh) --trigger-mode=always
# open a new shell — you're in the picker
```

`scripts/install.sh` builds the binary, wires the auto-attach hook into
`~/.bashrc.d/`, and writes `~/.config/tmux-picker/config.toml`. See
[Installation](#installation) below for `.deb`/`.rpm`/AUR/Homebrew and for
what `--trigger-mode` means.

## Installation

<details>
<summary><b>From a release binary / native package</b> (click to expand)</summary>

**Fedora / RHEL / openSUSE (`.rpm`)**

```bash
curl -LO https://github.com/jalsarraf0/tmux-picker/releases/download/v1.2.1/tmux-picker-1.2.1-1.x86_64.rpm
sudo dnf install ./tmux-picker-1.2.1-1.x86_64.rpm   # or: sudo zypper install ./tmux-picker-*.rpm
```

**Debian / Ubuntu (`.deb`)**

```bash
curl -LO https://github.com/jalsarraf0/tmux-picker/releases/download/v1.2.1/tmux-picker_1.2.1_amd64.deb
sudo apt install ./tmux-picker_1.2.1_amd64.deb
```

Both packages wire the auto-attach hook into the system-wide bashrc
automatically — no separate hook step needed. Default `trigger_mode` is
`"always"`; see [Configuration](#configuration) to restrict to SSH only.

**Arch Linux (AUR)**

```bash
git clone https://github.com/jalsarraf0/tmux-picker.git
cd tmux-picker/packaging && makepkg -si
```

Not yet submitted to the official AUR — build locally from the repo as
above until it is.

**macOS / Linuxbrew**

```bash
brew install --formula https://raw.githubusercontent.com/jalsarraf0/tmux-picker/main/packaging/homebrew/tmux-picker.rb
```

Builds from source via `cargo`. Not yet confirmed on real macOS hardware —
treat as best-effort. Homebrew doesn't touch dotfiles, so add the sourcing
snippet from [Enable the auto-attach hook](#enable-the-auto-attach-hook)
yourself.

**crates.io**

Not published yet (blocked on an unrelated token issue) — use `cargo
install --git` from [Quickstart](#quickstart) in the meantime.

</details>

<details>
<summary><b>Build from source, step by step</b> (click to expand)</summary>

```bash
git clone https://github.com/jalsarraf0/tmux-picker.git
cd tmux-picker
cargo build --release
install -m 0755 target/release/tmux-picker ~/.local/bin/tmux-picker
```

Make sure `~/.local/bin` is on `PATH`, then confirm with `tmux-picker
--version`.

</details>

<details>
<summary><b>Everything at once — <code>scripts/install.sh</code></b> (click to expand)</summary>

From inside a checkout:

```bash
bash scripts/install.sh
```

It asks **you** where the picker should run before touching anything:

```
Where should tmux-picker auto-run?
  1) Everywhere  — SSH logins AND local terminal windows (any emulator)
  2) SSH only    — the original behaviour; local terminals are untouched

Choose [1/2] (default: 1):
```

Skip the prompt by passing the choice up front (also what a non-interactive
run, e.g. an AI agent, must do — it refuses to guess):

```bash
bash scripts/install.sh --trigger-mode=always     # SSH + local terminals
bash scripts/install.sh --trigger-mode=ssh_only   # SSH only
```

It's idempotent — re-run any time (e.g. after `git pull`) to upgrade.

**Missing `tmux` or `cargo`?** Interactively it asks before installing
anything (may use `sudo`); non-interactively it refuses and tells you (or
your agent) to add a flag:

```bash
bash scripts/install.sh --auto-deps      # install missing tmux/cargo
bash scripts/install.sh --no-auto-deps   # skip; fail on missing cargo
```

`--auto-deps` detects your package manager (`apt`/`dnf`/`pacman`/`zypper`/
`apk`/`brew`) for `tmux`, and uses [rustup](https://rustup.rs) for a Rust
toolchain if `cargo` is missing (distro packages are often too old for this
project's `edition = "2024"`).

Fully hands-off, e.g. cloud-init: `bash scripts/install.sh
--trigger-mode=always --auto-deps`.

</details>

<details>
<summary><b>Installing via an AI coding agent</b> (click to expand)</summary>

Explicitly supported — see [docs/AI_AGENT_INSTALL.md](docs/AI_AGENT_INSTALL.md)
for ready-to-paste prompts for Claude Code and Codex CLI. The one rule: the
agent must ask you `always` vs `ssh_only` and whether it may auto-install
missing deps, and wait for both answers — `scripts/install.sh` itself
enforces this by refusing to guess when run non-interactively.

</details>

### Enable the auto-attach hook

The hook fires on every new interactive bash shell as long as `~/.bashrc`
sources `~/.bashrc.d/*.sh` (Fedora/RHEL do this by default; other distros
usually need it added once):

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

Skip it for one session with `NO_TMUX=1 ssh host` (or `export NO_TMUX=1`
before opening a local terminal). **zsh/fish:** the hook is bash-specific;
bridge it via `bash -ic '...; exit'` from your rc file — unverified, only
the bash/`~/.bashrc.d` path is tested.

### Verifying it's working

```bash
tmux-picker --check-config   # parse warnings + effective config, no TUI
tmux new-session -d -s smoke # create a detached session to pick from
# open a fresh SSH connection / terminal window and confirm "smoke" shows up
```

### Uninstall

```bash
rm -f ~/.local/bin/tmux-picker ~/.bashrc.d/tmux-autoattach.sh
rm -rf ~/.config/tmux-picker
```

Existing tmux sessions and their metadata are untouched; they just won't be
picked anymore.

## Keybindings

| Key | Action | | Key | Action |
|---|---|---|---|---|
| `↑`/`↓`, `j`/`k`, `1`-`9` | Move / jump | | `K` | Kill session (`y`/`Y` confirms) |
| `Enter` / double-click | Attach | | `r` | Rename |
| `n` | New session | | `o` | Cycle sort mode |
| `/` | Fuzzy filter | | `y` | Yank session name |
| `Esc` | Clear filter / cancel | | `Tab` | Toggle preview / windows view |
| `?` | Help | | `SIGHUP` | Reload config in place |

## CLI

```
tmux-picker                       Run the picker TUI (default)
tmux-picker label <session> ...   Set or clear session metadata
tmux-picker show  <session>       Print session metadata as TOML
tmux-picker auto  <session>       Auto-detect label/project from pane cwd
tmux-picker --help / --version
```

`label` takes `--label`, `--project`, `--purpose`, or `--clear` (mutually
exclusive with the others). `auto` walks up from the session's active-pane
cwd to the nearest `.git` and sets `project`/`label`/`purpose:branch:<b>` —
manually-set values are never overwritten. Handy from inside a Claude Code
session to self-label its own tmux session:

```bash
tmux-picker label "$(tmux display -p '#S')" --label "Investigating queue depth"
```

Metadata lives entirely in tmux user-options (`@tmux_picker_label`,
`@tmux_picker_project`, `@tmux_picker_purpose`, `@tmux_picker_label_at`) —
no external state file, and it's removed automatically when the session is
killed.

## Configuration

Optional TOML at `~/.config/tmux-picker/config.toml`. Every key is
optional; a missing or malformed file falls back to defaults with a single
stderr warning — login auto-attach is never blocked.

```toml
timeout_secs = 10          # auto-attach countdown, seconds; 0 disables it
trigger_mode = "always"    # "always" | "ssh_only"

[theme]
accent = "cyan"
warning = "red"
selection_bg = "darkgray"

[markers]
disable_defaults = false
# [markers.patterns]
# foo = "★"
```

`tmux-picker --init` writes a fully-commented starter file; `tmux-picker
--check-config` prints parse warnings plus the effective config.
`kill -HUP $(pgrep tmux-picker)` reloads it without restarting.

## Troubleshooting

- **Nothing happens on login** — check `~/.bashrc` sources
  `~/.bashrc.d/*.sh`, and that the shell is interactive (`echo $-` contains
  `i`). For SSH, also check `$SSH_CONNECTION` is non-empty.
- **Works over SSH but not locally** — `tmux-picker --print-trigger-mode`;
  if it prints `ssh_only`, remove/change `trigger_mode` in
  `config.toml`.
- **"tmux not found at /usr/bin/tmux"** — the hook hardcodes that path;
  symlink your tmux there or edit `_TMUX` in `tmux-autoattach.sh`.
- **Picker pegs CPU / won't exit** — a real bug (detached-PTY wedge), fixed
  in the commit tagged `fix: detached-pty wedge`; make sure you're on that
  version or later.

Release notes: [`CHANGELOG.md`](CHANGELOG.md).

## Contributing / Test

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
bash tests/e2e.sh
```

CI runs all four on every push/PR (`.github/workflows/ci.yml`). Maintainer
release checklist: [docs/RELEASING.md](docs/RELEASING.md).

## License

[MIT](LICENSE) — free to use, modify, and redistribute, with attribution
and **no warranty**. That covers the auto-attach hook and `trigger_mode`
behavior too: you (or an AI agent acting for you) choosing how this runs on
your machines is done at your own discretion and risk.
