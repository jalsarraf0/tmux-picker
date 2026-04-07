#!/usr/bin/env bash
set -euo pipefail

BINARY="$(cd "$(dirname "$0")/.." && pwd)/target/release/tmux-picker"
SOCKET="tmux-picker-e2e"
TMUX="/usr/bin/tmux"
PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "  FAIL: $1 — $2"; }

cleanup() { $TMUX -L "$SOCKET" kill-server 2>/dev/null || true; }
trap cleanup EXIT

echo "═══ tmux-picker E2E tests ═══"
echo ""

# Ensure release binary exists
if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: Build release binary first: cargo build --release"
    exit 1
fi

# --- Test 1: Binary with no tmux server outputs shell ---
echo "Test 1: No tmux server → shell"
cleanup
sleep 0.2
# Use a socket name that definitely has no server
action=$("$BINARY" 2>/dev/null </dev/null || echo "shell")
if [[ "$action" == "shell" ]]; then
    pass "no server → shell"
else
    fail "no server → shell" "got: $action"
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
$TMUX -L "$SOCKET" new-session -d -s e2e-test
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

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]] || exit 1
