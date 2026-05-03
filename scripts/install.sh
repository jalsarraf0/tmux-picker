#!/usr/bin/env bash
# tmux-picker installer.
#
# Steps:
#   1. cargo build --release
#   2. install binary to ~/.local/bin/tmux-picker
#   3. install shell hook to ~/.bashrc.d/tmux-autoattach.sh
#
# Idempotent: re-running upgrades the binary and overwrites the hook.
# Non-destructive: never touches existing tmux sessions or user dotfiles.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BINARY_SRC="$TARGET_DIR/release/tmux-picker"

BIN_DIR="${HOME}/.local/bin"
BASHRC_D="${HOME}/.bashrc.d"
BIN_DEST="$BIN_DIR/tmux-picker"
HOOK_DEST="$BASHRC_D/tmux-autoattach.sh"
HOOK_SRC="$REPO_ROOT/shell/tmux-autoattach.sh"

log()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

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

# Sanity check
"$BIN_DEST" --version >/dev/null || die "installed binary failed --version check"

log "installed:"
log "  $BIN_DEST  ($("$BIN_DEST" --version))"
log "  $HOOK_DEST"
log ""
log "next SSH login will land in the picker."
log "to bypass once: export NO_TMUX=1 before SSH'ing"
