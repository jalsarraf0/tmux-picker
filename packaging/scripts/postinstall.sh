#!/bin/sh
# tmux-picker native-package post-install hook (rpm %post / deb postinst /
# pacman post_install+post_upgrade). Wires the auto-attach hook into the
# system-wide interactive-shell rc file so it fires for every new SSH login
# AND every new local terminal window — not just login shells.
#
# Idempotent: safe to run on every install/upgrade. Detects Fedora/RHEL
# (/etc/bashrc) vs Debian/Ubuntu (/etc/bash.bashrc) at install time rather
# than hardcoding per package format, since the same binary/script pair
# ships in both.
set -e

HOOK_SRC="/usr/share/tmux-picker/tmux-autoattach.sh"
MARKER_START="# BEGIN tmux-picker auto-attach hook (managed by the tmux-picker package)"
MARKER_END="# END tmux-picker auto-attach hook"

detect_system_rc() {
    if [ -f /etc/bashrc ]; then
        echo /etc/bashrc
    elif [ -f /etc/bash.bashrc ]; then
        echo /etc/bash.bashrc
    else
        echo ""
    fi
}

RC_FILE="$(detect_system_rc)"

if [ -z "$RC_FILE" ]; then
    echo "tmux-picker: no system-wide bash rc file found (looked for /etc/bashrc, /etc/bash.bashrc)." >&2
    echo "tmux-picker: the auto-attach hook is installed at $HOOK_SRC but nothing sources it yet." >&2
    echo "tmux-picker: source it from your shell's rc file to enable auto-attach." >&2
    exit 0
fi

if [ ! -f "$HOOK_SRC" ]; then
    echo "tmux-picker: expected hook at $HOOK_SRC but it's missing; skipping rc wiring." >&2
    exit 0
fi

if grep -qF "$MARKER_START" "$RC_FILE" 2>/dev/null; then
    exit 0
fi

{
    echo ""
    echo "$MARKER_START"
    echo "[ -r \"$HOOK_SRC\" ] && . \"$HOOK_SRC\""
    echo "$MARKER_END"
} >> "$RC_FILE"

echo "tmux-picker: wired auto-attach into $RC_FILE"
echo "tmux-picker: default trigger_mode is \"always\" (SSH + local terminals)."
echo "tmux-picker: to restrict to SSH only, set trigger_mode = \"ssh_only\" in"
echo "tmux-picker: ~/.config/tmux-picker/config.toml (run 'tmux-picker --init' for a starter file)."

exit 0
