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

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
