#!/usr/bin/env bash
# tmux session picker — interactive SSH logins only.
# Calls tmux-picker binary for TUI, falls back to shell on any failure.

[[ -z "$SSH_CONNECTION" ]] && return
[[ -n "$TMUX" ]] && return
[[ "$-" != *i* ]] && return
[[ -n "${NO_TMUX:-}" ]] && return

_PICKER="${HOME}/.local/bin/tmux-picker"

if [[ -x "$_PICKER" ]]; then
    action="$("$_PICKER" 2>/dev/tty)"
    rc=$?
else
    echo "tmux-picker not found at $_PICKER — dropping to shell" >&2
    return
fi

if [[ $rc -ne 0 || -z "$action" ]]; then
    echo "tmux-picker exited $rc — dropping to shell" >&2
    return
fi

# Clear init guard so the new shell inside tmux runs .bashrc fully
unset __BASH_INIT_ONCE

case "$action" in
    attach:*)
        sess="${action#attach:}"
        exec /usr/bin/tmux attach-session -dt "$sess"
        echo "Failed to attach to '$sess'" >&2
        ;;
    new:*)
        sess="${action#new:}"
        exec /usr/bin/tmux new-session -s "$sess"
        echo "Failed to create session '$sess'" >&2
        ;;
    shell)
        command -v fastfetch &>/dev/null && fastfetch
        ;;
    *)
        echo "Unknown action from tmux-picker: $action" >&2
        ;;
esac
