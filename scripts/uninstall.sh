#!/usr/bin/env bash
# tmux-picker uninstaller.
#
# Removes the binary and shell hook installed by scripts/install.sh.
# Does NOT touch tmux sessions, user-options on existing sessions, or
# any user dotfiles outside ~/.bashrc.d/tmux-autoattach.sh.
set -euo pipefail

BIN_DEST="${HOME}/.local/bin/tmux-picker"
HOOK_DEST="${HOME}/.bashrc.d/tmux-autoattach.sh"

log() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }

removed=0

if [[ -e "$BIN_DEST" ]]; then
    log "removing $BIN_DEST"
    rm -f "$BIN_DEST"
    removed=$((removed + 1))
else
    log "binary already absent: $BIN_DEST"
fi

if [[ -e "$HOOK_DEST" ]]; then
    log "removing $HOOK_DEST"
    rm -f "$HOOK_DEST"
    removed=$((removed + 1))
else
    log "hook already absent: $HOOK_DEST"
fi

log "removed $removed file(s)."
log "tmux sessions and their @tmux_picker_* metadata are unchanged."
log "to fully reset: tmux kill-server"
