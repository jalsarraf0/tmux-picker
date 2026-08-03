#!/usr/bin/env bash
# tmux session picker — every new interactive shell, SSH or local.
# Calls tmux-picker binary for TUI, cascades through fallbacks so the user
# ALWAYS ends up inside tmux unless they explicitly choose a bare shell.
#
# Fallback chain (per action):
#   attach → new-session with that name → new "main" → bare shell (last resort)
#   new    → new-session with that name → new "main" → bare shell (last resort)

[[ -n "$TMUX" ]] && return
[[ "$-" != *i* ]] && return
[[ -n "${NO_TMUX:-}" ]] && return
# tmux-picker needs a real terminal; bail if /dev/tty is not available
[[ ! -t 0 ]] && return

_TMUX="/usr/bin/tmux"
_PICKER="${HOME}/.local/bin/tmux-picker"

# By default the picker fires for local terminals too, not just SSH logins.
# Set `trigger_mode = "ssh_only"` in config.toml to restore SSH-only behaviour.
if [[ -z "$SSH_CONNECTION" ]]; then
    _mode="always"
    if [[ -x "$_PICKER" ]]; then
        _mode="$("$_PICKER" --print-trigger-mode 2>/dev/null)"
        [[ -z "$_mode" ]] && _mode="always"
    fi
    if [[ "$_mode" == "ssh_only" ]]; then
        return
    fi
fi

# Verify tmux is available — cannot proceed without it
if [[ ! -x "$_TMUX" ]]; then
    echo "tmux not found at $_TMUX — cannot attach" >&2
    return
fi

# ---------------------------------------------------------------------------
# _tmux_fallback: cascading last-resort — new-session with the given name,
# then new "main", then bare shell.
# ---------------------------------------------------------------------------
_tmux_fallback() {
    local sess="${1:-main}"
    if "$_TMUX" new-session -As "$sess" 2>/dev/null; then
        return 0
    fi
    if [[ "$sess" != "main" ]]; then
        echo "tmux new-session '$sess' failed — trying 'main'" >&2
        if "$_TMUX" new-session -As "main" 2>/dev/null; then
            return 0
        fi
    fi
    echo "all tmux attempts failed — bare shell" >&2
    return 1
}

# ---------------------------------------------------------------------------
# Run the picker TUI
# ---------------------------------------------------------------------------
if [[ -x "$_PICKER" ]]; then
    action="$("$_PICKER" 2>/dev/tty)"
    rc=$?
else
    # Picker binary missing — skip TUI, go straight to fallback
    echo "tmux-picker not found at $_PICKER — attaching directly" >&2
    _tmux_fallback "main"
    unset -f _tmux_fallback
    return
fi

if [[ $rc -ne 0 || -z "$action" ]]; then
    echo "tmux-picker exited $rc — attaching directly" >&2
    _tmux_fallback "main"
    unset -f _tmux_fallback
    return
fi

# Clear init guard so the new shell inside tmux runs .bashrc fully
unset __BASH_INIT_ONCE

case "$action" in
    attach:*)
        sess="${action#attach:}"
        # Try attach; if session vanished, cascade through fallbacks
        "$_TMUX" attach-session -dt "$sess" 2>/dev/null || _tmux_fallback "$sess"
        ;;
    new:*)
        sess="${action#new:}"
        if "$_TMUX" new-session -As "$sess" 2>/dev/null; then
            # Best-effort auto-label so the new session shows up with a
            # meaningful project / label / branch on the next picker run.
            "$_PICKER" auto "$sess" 2>/dev/null || true
        else
            _tmux_fallback "$sess"
        fi
        ;;
    shell)
        command -v fastfetch &>/dev/null && fastfetch
        ;;
    *)
        echo "Unknown action from tmux-picker: $action — attaching directly" >&2
        _tmux_fallback "main"
        ;;
esac

unset -f _tmux_fallback
