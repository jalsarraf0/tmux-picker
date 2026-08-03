#!/usr/bin/env bash
# tmux-picker installer.
#
# Steps:
#   1. optionally install missing deps (tmux, a Rust toolchain) — opt-in
#   2. cargo build --release
#   3. install binary to ~/.local/bin/tmux-picker
#   4. install shell hook to ~/.bashrc.d/tmux-autoattach.sh
#   5. set trigger_mode (always vs ssh_only) in config.toml
#
# Idempotent: re-running upgrades the binary and overwrites the hook.
# Non-destructive: never touches existing tmux sessions or user dotfiles
# other than ~/.bashrc.d/tmux-autoattach.sh and the tmux-picker config.
# Package/toolchain installs (when opted into) are the only exception —
# see --auto-deps below.
#
# Usage:
#   scripts/install.sh                          interactive prompts (humans, tty)
#   scripts/install.sh --trigger-mode=always     SSH logins AND local terminals
#   scripts/install.sh --trigger-mode=ssh_only   SSH logins only (servers)
#   scripts/install.sh --auto-deps               install missing tmux/cargo automatically
#   scripts/install.sh --no-auto-deps            fail fast on missing deps, don't ask
#
# Fully hands-off (e.g. cloud-init, an AI agent that already has your answers):
#   scripts/install.sh --trigger-mode=always --auto-deps
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
AUTO_DEPS="${TMUX_PICKER_AUTO_DEPS:-}"
for arg in "$@"; do
    case "$arg" in
        --trigger-mode=*) TRIGGER_MODE="${arg#*=}" ;;
        --trigger-mode)   die "--trigger-mode requires a value: --trigger-mode=always or --trigger-mode=ssh_only" ;;
        --auto-deps)      AUTO_DEPS="yes" ;;
        --no-auto-deps)   AUTO_DEPS="no" ;;
    esac
done

if [[ -n "$TRIGGER_MODE" && "$TRIGGER_MODE" != "always" && "$TRIGGER_MODE" != "ssh_only" ]]; then
    die "--trigger-mode must be 'always' or 'ssh_only', got '$TRIGGER_MODE'"
fi

if [[ -n "$AUTO_DEPS" && "$AUTO_DEPS" != "yes" && "$AUTO_DEPS" != "no" ]]; then
    die "--auto-deps / TMUX_PICKER_AUTO_DEPS must be 'yes' or 'no', got '$AUTO_DEPS'"
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

[[ -f "$HOOK_SRC" ]] || die "shell hook missing at $HOOK_SRC"

# ---------------------------------------------------------------------------
# Dependencies (tmux, a Rust toolchain). Installing system packages / a
# toolchain is a bigger deal than writing a config file, so it gets its own
# opt-in, same shape as trigger_mode above: an explicit flag, or an
# interactive yes/no, or — non-interactively with no flag — refuse and
# explain, rather than silently reaching for sudo.
# ---------------------------------------------------------------------------
detect_pm() {
    if command -v apt-get >/dev/null; then echo apt
    elif command -v dnf >/dev/null; then echo dnf
    elif command -v pacman >/dev/null; then echo pacman
    elif command -v zypper >/dev/null; then echo zypper
    elif command -v apk >/dev/null; then echo apk
    elif command -v brew >/dev/null; then echo brew
    else echo unknown
    fi
}

maybe_sudo() {
    if [[ "$(id -u)" -eq 0 ]]; then
        "$@"
    elif command -v sudo >/dev/null; then
        sudo "$@"
    else
        die "need root privileges to run '$*' but no sudo is available; install manually and re-run"
    fi
}

install_tmux_via_pm() {
    local pm="$1"
    log "installing tmux via $pm"
    case "$pm" in
        apt)    maybe_sudo apt-get update -qq && maybe_sudo apt-get install -y tmux ;;
        dnf)    maybe_sudo dnf install -y tmux ;;
        pacman) maybe_sudo pacman -Sy --noconfirm tmux ;;
        zypper) maybe_sudo zypper --non-interactive install tmux ;;
        apk)    maybe_sudo apk add tmux ;;
        brew)   brew install tmux ;;
        *) die "no supported package manager found (looked for apt/dnf/pacman/zypper/apk/brew); install tmux manually and re-run" ;;
    esac || die "tmux install via $pm failed — see the error above, or install tmux manually and re-run"
}

install_rust_via_rustup() {
    log "installing a Rust toolchain via rustup (distro cargo packages are often too old for this project's edition = \"2024\")"
    command -v curl >/dev/null || die "curl not found; needed to fetch rustup. Install curl (or cargo yourself) and re-run"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal \
        || die "rustup install failed — see the curl/sh error above, or install a Rust toolchain manually and re-run"
    [[ -f "$HOME/.cargo/env" ]] || die "rustup reported success but $HOME/.cargo/env is missing"
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
}

join_with() {
    local sep="$1" out="$2"; shift 2
    for x in "$@"; do out+="$sep$x"; done
    printf '%s' "$out"
}

NEED_CARGO=0; command -v cargo >/dev/null || NEED_CARGO=1
NEED_TMUX=0;  command -v tmux  >/dev/null || NEED_TMUX=1

if [[ $NEED_CARGO -eq 1 || $NEED_TMUX -eq 1 ]]; then
    missing=()
    [[ $NEED_CARGO -eq 1 ]] && missing+=("a Rust toolchain (cargo)")
    [[ $NEED_TMUX -eq 1 ]] && missing+=("tmux")
    warn "missing: $(join_with ', ' "${missing[@]}")"

    if [[ -z "$AUTO_DEPS" ]]; then
        if [[ -t 0 && -t 1 ]]; then
            read -rp "Install missing dependencies now (may use sudo)? [y/N]: " _dep_choice
            case "$_dep_choice" in
                y|Y|yes|YES) AUTO_DEPS="yes" ;;
                *)            AUTO_DEPS="no" ;;
            esac
        else
            die "Missing dependencies ($(join_with ', ' "${missing[@]}")), and not an interactive terminal.
Re-run with one of:
  scripts/install.sh --auto-deps       (install tmux/rustup automatically, may use sudo)
  scripts/install.sh --no-auto-deps    (skip; fail on missing cargo, warn on missing tmux)

If an AI coding agent is running this installer: STOP. Ask the person you're
installing for whether it's OK to install system packages (via $(detect_pm),
possibly with sudo) before re-running with --auto-deps or --no-auto-deps."
        fi
    fi

    if [[ "$AUTO_DEPS" == "yes" ]]; then
        [[ $NEED_TMUX -eq 1 ]]  && install_tmux_via_pm "$(detect_pm)"
        [[ $NEED_CARGO -eq 1 ]] && install_rust_via_rustup
    else
        log "--no-auto-deps: leaving missing dependencies alone"
    fi
fi

# Verify prerequisites (again — --auto-deps may have just installed them)
command -v cargo >/dev/null || die "cargo not found in PATH"
command -v tmux  >/dev/null || warn "tmux not found in PATH (install will continue, but the picker won't work without tmux)"

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
