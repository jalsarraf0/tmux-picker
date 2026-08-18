# Install with an AI coding agent

Using an AI agent to run this installer is explicitly supported and allowed
under the [MIT license](../README.md#license) — go ahead. The rule: **the
agent must ask you (1) `always` vs `ssh_only`, and (2) whether it's OK to
auto-install `tmux`/a Rust toolchain if either is missing, and wait for both
answers before running `scripts/install.sh`.** It must not guess either on
your behalf.

This isn't just a suggestion — `scripts/install.sh` itself enforces both: run
non-interactively without `--trigger-mode`, and it refuses and prints
instructions telling the agent to stop and ask about that; hit a missing
`tmux`/`cargo` non-interactively without `--auto-deps`/`--no-auto-deps`, and
it refuses the same way. That's what makes it safe to hand this whole task to
an agent unattended: the worst case is it stops and asks, never that it
silently changes what every terminal does or reaches for `sudo` on its own.

Both prompts below tell the agent to ask first. Paste one in as-is — both
agents can read the [README](../README.md) straight from the repo, so a
short pointer is enough; they'll pick up the exact commands from there
rather than improvising.

## Claude Code

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

## Codex CLI

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

Provided as-is, MIT-licensed, no warranty — see [License](../README.md#license).
You (and whichever agent you delegate to) are responsible for the
`trigger_mode` and `--auto-deps` choices you make and how this ends up
configured on your machines.

Either agent can do this unattended in a normal user account. With
`--no-auto-deps` (or when nothing's missing), the installer only touches
`~/.local/bin`, `~/.bashrc.d`, `~/.config/tmux-picker`, and — if missing —
appends the sourcing snippet to `~/.bashrc`; it never needs root. With
`--auto-deps` and something actually missing, it will invoke your package
manager (via `sudo`, unless already root) to install `tmux`, and/or run the
official `rustup` installer to get a Rust toolchain — see the README's
[Missing tmux or cargo?](../README.md#missing-tmux-or-cargo---auto-deps)
section for exactly what that runs.
