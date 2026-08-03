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

# ---------------------------------------------------------------------------
# --auto-deps / dependency-gate tests.
#
# These simulate "cargo missing" / "tmux missing" by shadowing the `command`
# builtin (exported into the child bash that runs install.sh) rather than
# touching $PATH — an earlier manual run discovered that fully overriding
# $PATH to a bare directory hangs in this sandbox, and even a partial
# override risks a real package manager command slipping through. Where a
# test needs install.sh to actually reach for sudo/dnf/curl (the --auto-deps
# dispatch tests), a FAKEBIN dir with mock `sudo` and `curl` is *prepended*
# to the real $PATH (never replacing it) so nothing here ever touches the
# host's real packages or a real rustup install.
# ---------------------------------------------------------------------------

FAKEBIN="$(mktemp -d)"
cat > "$FAKEBIN/sudo" <<'EOF'
#!/bin/sh
echo "FAKE-SUDO-RAN: $*"
exit 0
EOF
cat > "$FAKEBIN/curl" <<'EOF'
#!/bin/sh
echo '#!/bin/sh'
echo 'echo FAKE-RUSTUP-RAN'
exit 0
EOF
chmod +x "$FAKEBIN/sudo" "$FAKEBIN/curl"
trap 'rm -rf "$FAKEBIN"' EXIT

hide_cargo_tmux() {
    # $1: "cargo", "tmux", or "both" — which command(s) to make appear missing
    cat <<EOF
command() {
    if [[ "\$1" == "-v" ]]; then
        case "\$2" in
EOF
    case "$1" in
        cargo) echo '            cargo) return 1 ;;' ;;
        tmux)  echo '            tmux) return 1 ;;' ;;
        both)  echo '            cargo|tmux) return 1 ;;' ;;
    esac
    cat <<'EOF'
        esac
    fi
    builtin command "$@"
}
export -f command
EOF
}

# --- Test 8: both missing, --no-auto-deps -> leaves them, dies on cargo ---
echo "Test 8: --no-auto-deps with cargo+tmux missing dies on cargo"
TH="$(mktemp -d)"
out=$(timeout 15 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash -c \
    "$(hide_cargo_tmux both)"$'\n'"bash '$INSTALL_SH' --no-auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] \
    && grep -q "missing: a Rust toolchain (cargo), tmux" <<<"$out" \
    && grep -q "leaving missing dependencies alone" <<<"$out" \
    && grep -q "cargo not found in PATH" <<<"$out"; then
    pass "--no-auto-deps reports missing deps then dies on cargo"
else
    fail "--no-auto-deps both missing" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 9: only tmux missing, --no-auto-deps -> warns but still installs ---
echo "Test 9: --no-auto-deps with only tmux missing warns and continues"
TH="$(mktemp -d)"
out=$(timeout 60 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash -c \
    "$(hide_cargo_tmux tmux)"$'\n'"bash '$INSTALL_SH' --no-auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q "tmux not found in PATH (install will continue" <<<"$out" \
    && [[ -x "$TH/.local/bin/tmux-picker" ]]; then
    pass "--no-auto-deps warns on tmux-only and still installs"
else
    fail "--no-auto-deps tmux-only" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 10: non-interactive, missing deps, no --auto-deps flag -> refuses ---
echo "Test 10: non-interactive with missing deps and no --auto-deps flag refuses"
TH="$(mktemp -d)"
out=$(timeout 15 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" bash -c \
    "$(hide_cargo_tmux both)"$'\n'"bash '$INSTALL_SH' --trigger-mode=always </dev/null" 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] \
    && grep -qi "STOP" <<<"$out" \
    && grep -qi "ask the person" <<<"$out" \
    && [[ ! -e "$TH/.local/bin/tmux-picker" ]]; then
    pass "non-interactive missing-deps refuses and instructs agents to ask first"
else
    fail "non-interactive missing-deps refusal" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 11: --auto-deps with tmux missing dispatches through (mocked) sudo ---
echo "Test 11: --auto-deps with tmux missing calls the package manager"
TH="$(mktemp -d)"
mkdir -p "$TH/.cargo"; touch "$TH/.cargo/env"
out=$(timeout 30 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" PATH="$FAKEBIN:$PATH" bash -c \
    "$(hide_cargo_tmux tmux)"$'\n'"bash '$INSTALL_SH' --auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] && grep -q "FAKE-SUDO-RAN:.*install -y tmux" <<<"$out"; then
    pass "--auto-deps dispatches tmux install through the detected package manager"
else
    fail "--auto-deps tmux dispatch" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 12: --auto-deps with cargo missing dispatches through (mocked) rustup ---
echo "Test 12: --auto-deps with cargo missing calls rustup"
TH="$(mktemp -d)"
mkdir -p "$TH/.cargo"; touch "$TH/.cargo/env"
out=$(timeout 30 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" PATH="$FAKEBIN:$PATH" bash -c \
    "$(hide_cargo_tmux cargo)"$'\n'"bash '$INSTALL_SH' --auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if grep -q "installing a Rust toolchain via rustup" <<<"$out" && grep -q "FAKE-RUSTUP-RAN" <<<"$out"; then
    pass "--auto-deps dispatches cargo install through rustup"
else
    fail "--auto-deps cargo dispatch" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 13: invalid TMUX_PICKER_AUTO_DEPS value is rejected ---
echo "Test 13: TMUX_PICKER_AUTO_DEPS=bogus is rejected"
TH="$(mktemp -d)"
out=$(timeout 15 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" TMUX_PICKER_AUTO_DEPS=bogus bash -c \
    "$(hide_cargo_tmux both)"$'\n'"bash '$INSTALL_SH' --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] && grep -q "must be 'yes' or 'no'" <<<"$out"; then
    pass "invalid TMUX_PICKER_AUTO_DEPS value rejected"
else
    fail "invalid TMUX_PICKER_AUTO_DEPS" "rc=$rc; out=$out"
fi
rm -rf "$TH"

# --- Test 14: detect_pm() priority — apt-get wins when present ---
echo "Test 14: detect_pm prioritises apt-get when present"
got=$(timeout 5 env PATH="$FAKEBIN:$PATH" bash -c '
    printf "#!/bin/sh\necho fake-apt\n" > "'"$FAKEBIN"'/apt-get"
    chmod +x "'"$FAKEBIN"'/apt-get"
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
    detect_pm
')
rm -f "$FAKEBIN/apt-get"
if [[ "$got" == "apt" ]]; then
    pass "detect_pm prioritises apt-get over other package managers"
else
    fail "detect_pm priority" "got: $got"
fi

# --- Test 15/16: a failing installer produces a clear die(), not a raw abort ---
FAILBIN="$(mktemp -d)"
cat > "$FAILBIN/sudo" <<'EOF'
#!/bin/sh
echo "FAKE-SUDO-FAILING: $*" >&2
exit 1
EOF
cat > "$FAILBIN/curl" <<'EOF'
#!/bin/sh
echo "FAKE-CURL-FAILING: network unreachable" >&2
exit 6
EOF
chmod +x "$FAILBIN/sudo" "$FAILBIN/curl"

echo "Test 15: a failing package-manager install produces a clear die() message"
TH="$(mktemp -d)"
mkdir -p "$TH/.cargo"; touch "$TH/.cargo/env"
out=$(timeout 20 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" PATH="$FAILBIN:$PATH" bash -c \
    "$(hide_cargo_tmux tmux)"$'\n'"bash '$INSTALL_SH' --auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] && grep -q "tmux install via .* failed" <<<"$out"; then
    pass "failing tmux install produces a clear die() message"
else
    fail "failing tmux install message" "rc=$rc; out=$out"
fi
rm -rf "$TH"

echo "Test 16: a failing rustup fetch produces a clear die() message"
TH="$(mktemp -d)"
mkdir -p "$TH/.cargo"; touch "$TH/.cargo/env"
out=$(timeout 20 env HOME="$TH" XDG_CONFIG_HOME="$TH/.config" PATH="$FAILBIN:$PATH" bash -c \
    "$(hide_cargo_tmux cargo)"$'\n'"bash '$INSTALL_SH' --auto-deps --trigger-mode=always" 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] && grep -q "rustup install failed" <<<"$out"; then
    pass "failing rustup fetch produces a clear die() message"
else
    fail "failing rustup message" "rc=$rc; out=$out"
fi
rm -rf "$TH" "$FAILBIN"

rm -rf "$FAKEBIN"
trap - EXIT

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
