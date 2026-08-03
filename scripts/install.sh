#!/usr/bin/env bash
# tmux-picker installer.
#
# Steps:
#   1. cargo build --release
#   2. install binary to ~/.local/bin/tmux-picker
#   3. install shell hook to ~/.bashrc.d/tmux-autoattach.sh
#   4. set trigger_mode (always vs ssh_only) in config.toml
#
# Idempotent: re-running upgrades the binary and overwrites the hook.
# Non-destructive: never touches existing tmux sessions or user dotfiles
# other than ~/.bashrc.d/tmux-autoattach.sh and the tmux-picker config.
#
# Usage:
#   scripts/install.sh                       interactive prompt (humans, tty)
#   scripts/install.sh --trigger-mode=always     SSH logins AND local terminals
#   scripts/install.sh --trigger-mode=ssh_only   SSH logins only (servers)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BINARY_SRC="$TARGET_DIR/release/tmux-picker"

BIN_DIR="${HOME}/.local/bin"
BASHRC_D="${HOME}/.bashrc.d"
BIN_DEST="$BIN_DIR/tmux-picker"
HOOK_DEST="$BASHRC_D/tmux-autoattach.sh"
HOOK_SRC="$REPO_ROOT/shell/tmux-autoattach.sh"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/tmux-picker"
CONFIG_FILE="$CONFIG_DIR/config.toml"

log()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Decide trigger_mode BEFORE touching anything. This is a behaviour choice
# for the human sitting at (or SSHing into) this machine, not something an
# installer — human or AI agent — should guess on their behalf.
# ---------------------------------------------------------------------------
TRIGGER_MODE="${TMUX_PICKER_TRIGGER_MODE:-}"
for arg in "$@"; do
    case "$arg" in
        --trigger-mode=*) TRIGGER_MODE="${arg#*=}" ;;
        --trigger-mode)   die "--trigger-mode requires a value: --trigger-mode=always or --trigger-mode=ssh_only" ;;
    esac
done

if [[ -n "$TRIGGER_MODE" && "$TRIGGER_MODE" != "always" && "$TRIGGER_MODE" != "ssh_only" ]]; then
    die "--trigger-mode must be 'always' or 'ssh_only', got '$TRIGGER_MODE'"
fi

if [[ -z "$TRIGGER_MODE" ]]; then
    if [[ -t 0 && -t 1 ]]; then
        echo ""
        echo "Where should tmux-picker auto-run?"
        echo "  1) Everywhere  — SSH logins AND local terminal windows (any emulator)"
        echo "  2) SSH only    — the original behaviour; local terminals are untouched"
        echo ""
        read -rp "Choose [1/2] (default: 1): " _choice
        case "$_choice" in
            2) TRIGGER_MODE="ssh_only" ;;
            *) TRIGGER_MODE="always" ;;
        esac
    else
        die "Not an interactive terminal, and no --trigger-mode was given.
Re-run with one of:
  scripts/install.sh --trigger-mode=always      (SSH logins AND local terminals)
  scripts/install.sh --trigger-mode=ssh_only    (SSH logins only)

If an AI coding agent is running this installer: STOP. Ask the person you're
installing for which of the two they want — do not choose on their behalf —
then re-run with the matching --trigger-mode flag."
    fi
fi
log "trigger_mode = $TRIGGER_MODE"

# Verify prerequisites
command -v cargo  >/dev/null || die "cargo not found in PATH"
command -v tmux   >/dev/null || warn "tmux not found in PATH (install will continue, but the picker won't work without tmux)"
[[ -f "$HOOK_SRC" ]] || die "shell hook missing at $HOOK_SRC"

# Build
log "building release binary"
( cd "$REPO_ROOT" && cargo build --release ) >/dev/null

[[ -x "$BINARY_SRC" ]] || die "release binary not found at $BINARY_SRC"

# Install binary
log "installing binary to $BIN_DEST"
mkdir -p "$BIN_DIR"
install -m 0755 "$BINARY_SRC" "$BIN_DEST"

# Install hook
log "installing shell hook to $HOOK_DEST"
mkdir -p "$BASHRC_D"
install -m 0644 "$HOOK_SRC" "$HOOK_DEST"

# Write trigger_mode into config.toml, preserving any existing config
log "writing trigger_mode = \"$TRIGGER_MODE\" to $CONFIG_FILE"
mkdir -p "$CONFIG_DIR"
if [[ ! -f "$CONFIG_FILE" ]]; then
    "$BIN_DEST" --init >/dev/null
fi
if grep -q '^trigger_mode' "$CONFIG_FILE"; then
    sed -i "s/^trigger_mode.*/trigger_mode = \"$TRIGGER_MODE\"/" "$CONFIG_FILE"
else
    printf '\ntrigger_mode = "%s"\n' "$TRIGGER_MODE" >> "$CONFIG_FILE"
fi

# Sanity check
"$BIN_DEST" --version >/dev/null || die "installed binary failed --version check"
[[ "$("$BIN_DEST" --print-trigger-mode)" == "$TRIGGER_MODE" ]] \
    || die "trigger_mode did not take effect — check $CONFIG_FILE"

log "installed:"
log "  $BIN_DEST  ($("$BIN_DEST" --version))"
log "  $HOOK_DEST"
log "  $CONFIG_FILE  (trigger_mode = \"$TRIGGER_MODE\")"
log ""
if [[ "$TRIGGER_MODE" == "always" ]]; then
    log "next SSH login OR local terminal window will land in the picker."
else
    log "next SSH login will land in the picker (local terminals are untouched)."
fi
log "to bypass once: export NO_TMUX=1 before SSH'ing / opening a terminal"
