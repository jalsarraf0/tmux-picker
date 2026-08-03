#!/bin/sh
# tmux-picker native-package pre-remove hook (rpm %preun / deb prerm).
# Strips the guarded block this package's postinstall script added to the
# system-wide bash rc file — but ONLY on a genuine removal, not an upgrade.
#
# rpm's %preun receives $1 = number of package instances that will remain
# after this transaction (0 = genuine erase, >=1 = upgrade in progress).
# deb's prerm receives $1 = "remove"/"purge" (genuine removal) or
# "upgrade"/"deconfigure"/"failed-upgrade" (not a real removal).
#
# This distinction matters because rpm/dnf run the NEW package's %post
# before the OLD package's %preun during an upgrade — an unconditional
# strip-on-remove would run after postinstall's re-add and leave the hook
# permanently stripped after every single upgrade. Verified against a real
# `dnf reinstall` in a container before this guard was added.
set -e

case "${1:-}" in
    0|remove|purge) : ;;   # genuine removal — proceed to strip below
    *) exit 0 ;;           # upgrade/deconfigure/failed-upgrade — leave it alone
esac

MARKER_START="# BEGIN tmux-picker auto-attach hook (managed by the tmux-picker package)"
MARKER_END="# END tmux-picker auto-attach hook"

for RC_FILE in /etc/bashrc /etc/bash.bashrc; do
    [ -f "$RC_FILE" ] || continue
    if grep -qF "$MARKER_START" "$RC_FILE" 2>/dev/null; then
        sed -i.tmux-picker-bak "/^$(printf '%s' "$MARKER_START" | sed 's/[.[\*^$/]/\\&/g')$/,/^$(printf '%s' "$MARKER_END" | sed 's/[.[\*^$/]/\\&/g')$/d" "$RC_FILE"
        rm -f "$RC_FILE.tmux-picker-bak"
    fi
done

exit 0
