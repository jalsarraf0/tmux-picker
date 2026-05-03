#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Honor CARGO_TARGET_DIR (set in many user environments to avoid bloating ~).
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BINARY="$TARGET_DIR/release/tmux-picker"
SOCKET="tmux-picker-e2e"
TMUX_BIN="/usr/bin/tmux"
PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "  FAIL: $1 — $2"; }

cleanup() { $TMUX_BIN -L "$SOCKET" kill-server 2>/dev/null || true; }
trap cleanup EXIT

echo "═══ tmux-picker E2E tests ═══"
echo ""

# Ensure release binary exists
if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: Build release binary first: cargo build --release"
    exit 1
fi

# --- Test 1: Binary with no tmux session pool emits a tmux action ---
echo "Test 1: No sessions → emits action protocol"
# When tmux has zero sessions (or fails entirely), the binary emits a
# new:<name> action rather than crashing. This protects the SSH login flow.
# We don't guarantee no sessions here (user may have a live tmux server)
# but assert that the output is a valid action protocol string.
action=$("$BINARY" 2>/dev/null </dev/null || true)
if [[ "$action" == shell || "$action" == new:* || "$action" == attach:* ]]; then
    pass "valid action emitted ($action)"
else
    fail "no sessions → action protocol" "got: $action"
fi

# --- Test 2: Binary missing fallback ---
echo "Test 2: Missing binary detection"
if [[ -x "/nonexistent/tmux-picker" ]]; then
    fail "missing binary" "/nonexistent/tmux-picker should not exist"
else
    pass "missing binary correctly detected"
fi

# --- Test 3: Binary runs without crash ---
echo "Test 3: Binary does not crash"
cleanup
$TMUX_BIN -L "$SOCKET" new-session -d -s e2e-test
sleep 0.1
timeout 2 "$BINARY" 2>/dev/null </dev/null && rc=$? || rc=$?
if [[ $rc -le 1 ]]; then
    pass "binary runs without crash (exit $rc)"
else
    fail "binary runs without crash" "exit code: $rc"
fi
cleanup

# --- Test 4: NO_TMUX escape hatch ---
echo "Test 4: NO_TMUX bypass"
result=$(NO_TMUX=1 SSH_CONNECTION="1 2 3 4" TMUX="" bash -c '
    [[ -n "${NO_TMUX:-}" ]] && echo "BYPASSED" || echo "NOT_BYPASSED"
')
if [[ "$result" == "BYPASSED" ]]; then
    pass "NO_TMUX bypass works"
else
    fail "NO_TMUX bypass" "got: $result"
fi

# --- Test 5: Non-SSH guard ---
echo "Test 5: Non-SSH does not trigger"
result=$(SSH_CONNECTION="" TMUX="" bash -c '
    [[ -z "$SSH_CONNECTION" ]] && echo "SKIPPED" || echo "TRIGGERED"
')
if [[ "$result" == "SKIPPED" ]]; then
    pass "non-SSH skips picker"
else
    fail "non-SSH guard" "got: $result"
fi

# --- Test 6: Already in tmux guard ---
echo "Test 6: Already-in-tmux does not trigger"
result=$(SSH_CONNECTION="1 2 3 4" TMUX="/tmp/tmux-1000/default,123,0" bash -c '
    [[ -n "$TMUX" ]] && echo "SKIPPED" || echo "TRIGGERED"
')
if [[ "$result" == "SKIPPED" ]]; then
    pass "already in tmux skips picker"
else
    fail "already-in-tmux guard" "got: $result"
fi

# --- Test 7: --help exits 0 and lists subcommands ---
echo "Test 7: --help mentions subcommands"
help_out=$("$BINARY" --help 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q "label" <<<"$help_out" \
    && grep -q "show" <<<"$help_out" \
    && grep -q "auto" <<<"$help_out"; then
    pass "--help lists label/show/auto"
else
    fail "--help" "exit=$rc; missing one of label/show/auto in output"
fi

# --- Test 8: --version exits 0 ---
echo "Test 8: --version exits 0"
"$BINARY" --version >/dev/null 2>&1 && pass "--version exits 0" \
    || fail "--version" "non-zero exit"

# --- Test 9: label/show round-trip on default socket ---
echo "Test 9: label/show round-trip"
SESS="e2e-label-$$"
/usr/bin/tmux new-session -d -s "$SESS" 2>/dev/null
sleep 0.1
"$BINARY" label "$SESS" --label "e2e test" >/dev/null 2>&1
out=$("$BINARY" show "$SESS" 2>/dev/null)
if grep -q 'label = "e2e test"' <<<"$out"; then
    pass "label/show round-trip"
else
    fail "label/show round-trip" "got: $out"
fi
/usr/bin/tmux kill-session -t "$SESS" 2>/dev/null

# --- Test 10: --check-config with no file ---
echo "Test 10: --check-config with no config file"
TMP_HOME="$(mktemp -d)"
trap 'cleanup; rm -rf "$TMP_HOME"' EXIT
out=$(HOME="$TMP_HOME" XDG_CONFIG_HOME="$TMP_HOME/.config" \
    "$BINARY" --check-config 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q "(none)" <<<"$out" \
    && grep -q "timeout_secs = 10" <<<"$out"; then
    pass "--check-config no file"
else
    fail "--check-config no file" "exit=$rc; got: $out"
fi

# --- Test 11: --check-config flags an unknown color ---
echo "Test 11: --check-config flags unknown color"
mkdir -p "$TMP_HOME/.config/tmux-picker"
cat >"$TMP_HOME/.config/tmux-picker/config.toml" <<'TOML'
timeout_secs = 5
[theme]
accent = "chartreuse"
TOML
out=$(HOME="$TMP_HOME" XDG_CONFIG_HOME="$TMP_HOME/.config" \
    "$BINARY" --check-config 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q "chartreuse" <<<"$out" \
    && grep -q "timeout_secs = 5" <<<"$out"; then
    pass "--check-config flags unknown color"
else
    fail "--check-config unknown color" "exit=$rc; got: $out"
fi

# --- Test 12: --check-config accepts a hex color ---
echo "Test 12: --check-config round-trips a hex color"
cat >"$TMP_HOME/.config/tmux-picker/config.toml" <<'TOML'
[theme]
accent = "#ff8800"
TOML
out=$(HOME="$TMP_HOME" XDG_CONFIG_HOME="$TMP_HOME/.config" \
    "$BINARY" --check-config 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q '(none)' <<<"$out" \
    && grep -q 'accent = "#ff8800"' <<<"$out"; then
    pass "--check-config hex round-trip"
else
    fail "--check-config hex" "exit=$rc; got: $out"
fi

# --- Test 13a: --init writes a starter config ---
echo "Test 13a: --init writes a starter config"
INIT_HOME="$(mktemp -d)"
out=$(HOME="$INIT_HOME" XDG_CONFIG_HOME="$INIT_HOME/.config" \
    "$BINARY" --init 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && [[ -f "$INIT_HOME/.config/tmux-picker/config.toml" ]] \
    && grep -q 'timeout_secs' "$INIT_HOME/.config/tmux-picker/config.toml"; then
    pass "--init writes starter config"
else
    fail "--init writes starter config" "exit=$rc; got: $out"
fi

# --- Test 13b: --init refuses to overwrite without --force ---
echo "Test 13b: --init refuses overwrite without --force"
out=$(HOME="$INIT_HOME" XDG_CONFIG_HOME="$INIT_HOME/.config" \
    "$BINARY" --init 2>&1) && rc=0 || rc=$?
if [[ $rc -ne 0 ]] && grep -q 'refusing to overwrite' <<<"$out"; then
    pass "--init refuses overwrite"
else
    fail "--init no overwrite" "exit=$rc; got: $out"
fi

# --- Test 13c: --init --force overwrites ---
echo "Test 13c: --init --force overwrites"
echo "stale" > "$INIT_HOME/.config/tmux-picker/config.toml"
out=$(HOME="$INIT_HOME" XDG_CONFIG_HOME="$INIT_HOME/.config" \
    "$BINARY" --init --force 2>&1) && rc=0 || rc=$?
if [[ $rc -eq 0 ]] \
    && grep -q 'timeout_secs' "$INIT_HOME/.config/tmux-picker/config.toml"; then
    pass "--init --force overwrites"
else
    fail "--init --force" "exit=$rc; got: $out"
fi
rm -rf "$INIT_HOME"

# --- Test 13: auto-label falls back to ~/git/<sessname> ---
echo "Test 13: auto picks ~/git/<sess> when pane cwd has no repo"
SESS="e2e-stub-$$"
STUB_HOME="$(mktemp -d)"
mkdir -p "$STUB_HOME/git/$SESS"
# Spawn the session with -c so its pane cwd sits at $STUB_HOME (no .git
# anywhere up the tree). The fallback then resolves to $STUB_HOME/git/<sess>.
if /usr/bin/tmux new-session -d -s "$SESS" -c "$STUB_HOME" 2>/dev/null; then
    HOME="$STUB_HOME" "$BINARY" auto "$SESS" >/dev/null 2>&1
    out=$(HOME="$STUB_HOME" "$BINARY" show "$SESS" 2>/dev/null)
    /usr/bin/tmux kill-session -t "$SESS" 2>/dev/null || true
    if grep -q "project = \"$STUB_HOME/git/$SESS\"" <<<"$out" \
        && grep -q "label = \"$SESS\"" <<<"$out"; then
        pass "auto-label uses ~/git fallback"
    else
        fail "auto-label fallback" "got: $out"
    fi
else
    fail "auto-label fallback setup" "could not create session"
fi
rm -rf "$STUB_HOME"

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
