#!/usr/bin/env bash
# Regression tests for scripts/install.sh's trigger_mode consent gate.
#
# Covers: refusal to guess when non-interactive and no flag is given (the
# "stop and ask the human" contract for AI-assisted installs), the explicit
# --trigger-mode flag, bad values, and re-running against an existing config.
#
# Runs the installer against a throwaway $HOME so it never touches the real
# ~/.local/bin, ~/.bashrc.d, or ~/.config/tmux-picker.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_SH="$REPO_ROOT/scripts/install.sh"
PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "  FAIL: $1 — $2"; }

# cargo/rustup resolve their default toolchain from these; a fake $HOME
# without them makes `cargo build` fail for reasons unrelated to this
# installer, so pin them to the real ones explicitly.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

echo "═══ install.sh trigger_mode consent-gate tests ═══"
echo ""

# --- Test 1: non-interactive + no flag refuses and touches nothing ---
echo "Test 1: non-interactive, no --trigger-mode -> refuses"
TH="$(mktemp -d)"
out=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" </dev/null 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] \
    && grep -qi "STOP" <<<"$out" \
    && grep -qi "ask the person" <<<"$out" \
    && [[ ! -e "$TH/.local/bin/tmux-picker" ]] \
    && [[ ! -e "$TH/.bashrc.d/tmux-autoattach.sh" ]]; then
    pass "non-interactive install refuses and instructs agents to ask first"
else
    fail "non-interactive refusal" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 2: --trigger-mode=ssh_only installs and configures correctly ---
echo "Test 2: --trigger-mode=ssh_only"
TH="$(mktemp -d)"
HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" --trigger-mode=ssh_only </dev/null >/dev/null
mode=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" "$TH/.local/bin/tmux-picker" --print-trigger-mode)
if [[ "$mode" == "ssh_only" ]] && [[ -x "$TH/.local/bin/tmux-picker" ]] \
    && [[ -f "$TH/.bashrc.d/tmux-autoattach.sh" ]]; then
    pass "--trigger-mode=ssh_only installs with ssh_only in effect"
else
    fail "--trigger-mode=ssh_only" "got mode=$mode"
fi
rm -rf "$TH"

# --- Test 3: --trigger-mode=always installs and configures correctly ---
echo "Test 3: --trigger-mode=always"
TH="$(mktemp -d)"
HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" --trigger-mode=always </dev/null >/dev/null
mode=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" "$TH/.local/bin/tmux-picker" --print-trigger-mode)
if [[ "$mode" == "always" ]]; then
    pass "--trigger-mode=always installs with always in effect"
else
    fail "--trigger-mode=always" "got mode=$mode"
fi
rm -rf "$TH"

# --- Test 4: invalid --trigger-mode value is rejected ---
echo "Test 4: --trigger-mode=bogus is rejected"
TH="$(mktemp -d)"
out=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" --trigger-mode=bogus </dev/null 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] && grep -q "must be 'always' or 'ssh_only'" <<<"$out"; then
    pass "invalid --trigger-mode value rejected"
else
    fail "invalid --trigger-mode" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 5: re-running with a different mode updates config.toml in place ---
echo "Test 5: re-run switches trigger_mode without duplicating the key"
TH="$(mktemp -d)"
HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" --trigger-mode=always </dev/null >/dev/null
HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash "$INSTALL_SH" --trigger-mode=ssh_only </dev/null >/dev/null
count=$(grep -c '^trigger_mode' "$TH/.config/tmux-picker/config.toml")
mode=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" "$TH/.local/bin/tmux-picker" --print-trigger-mode)
if [[ "$count" -eq 1 ]] && [[ "$mode" == "ssh_only" ]]; then
    pass "re-run updates trigger_mode in place (no duplicate key)"
else
    fail "re-run update" "count=$count mode=$mode"
fi
rm -rf "$TH"

# --- Test 6: interactive prompt, Enter accepts the default (always) ---
echo "Test 6: interactive prompt, Enter -> default 'always'"
TH="$(mktemp -d)"
if command -v python3 >/dev/null; then
    python3 - "$TH" "$INSTALL_SH" <<'PY' >/dev/null
import pty, os, sys, time
testhome, install_sh = sys.argv[1], sys.argv[2]
env = dict(os.environ)
env["HOME"] = testhome
env["XDG_CONFIG_HOME"] = f"{testhome}/.config"
pid, fd = pty.fork()
if pid == 0:
    os.execvpe("bash", ["bash", install_sh], env)
else:
    time.sleep(1.5)
    os.write(fd, b"\n")
    try:
        while os.read(fd, 4096):
            pass
    except OSError:
        pass
    os.waitpid(pid, 0)
PY
    mode=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" "$TH/.local/bin/tmux-picker" --print-trigger-mode 2>/dev/null || echo "MISSING")
    if [[ "$mode" == "always" ]]; then
        pass "interactive Enter defaults to always"
    else
        fail "interactive Enter default" "got mode=$mode"
    fi
else
    echo "  SKIP: python3 not available for pty simulation"
fi
rm -rf "$TH"

# --- Test 7: interactive prompt, "2" selects ssh_only ---
echo "Test 7: interactive prompt, '2' -> ssh_only"
TH="$(mktemp -d)"
if command -v python3 >/dev/null; then
    python3 - "$TH" "$INSTALL_SH" <<'PY' >/dev/null
import pty, os, sys, time
testhome, install_sh = sys.argv[1], sys.argv[2]
env = dict(os.environ)
env["HOME"] = testhome
env["XDG_CONFIG_HOME"] = f"{testhome}/.config"
pid, fd = pty.fork()
if pid == 0:
    os.execvpe("bash", ["bash", install_sh], env)
else:
    time.sleep(1.5)
    os.write(fd, b"2\n")
    try:
        while os.read(fd, 4096):
            pass
    except OSError:
        pass
    os.waitpid(pid, 0)
PY
    mode=$(HOME="$TH" XDG_CONFIG_HOME="$TH/.config" "$TH/.local/bin/tmux-picker" --print-trigger-mode 2>/dev/null || echo "MISSING")
    if [[ "$mode" == "ssh_only" ]]; then
        pass "interactive '2' selects ssh_only"
    else
        fail "interactive '2' selection" "got mode=$mode"
    fi
else
    echo "  SKIP: python3 not available for pty simulation"
fi
rm -rf "$TH"

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
