#!/usr/bin/env bash
# End-to-end regression tests for the interactive picker.
#
# The release binary runs inside a pane of a private "driver" tmux server,
# so it gets a real terminal; keys, mouse sequences, signals and resizes are
# injected through tmux and the rendered screen is read back with
# capture-pane. The sessions it picks from live on a second private server.
# Nothing touches the user's own tmux server, config, or dotfiles.
#
#   bash tests/tui_e2e.sh                  (after: cargo build --release)
#   TMUX_PICKER_BIN=/path/to/bin bash tests/tui_e2e.sh
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BIN="${TMUX_PICKER_BIN:-$TARGET_DIR/release/tmux-picker}"
TMUX_BIN="$(command -v tmux || true)"
[[ -x "$BIN" ]] || { echo "ERROR: build first: cargo build --release ($BIN)"; exit 1; }
[[ -n "$TMUX_BIN" ]] || { echo "ERROR: tmux not found in PATH"; exit 1; }

# Short base path: unix socket paths are capped at ~104 bytes.
WORK="$(mktemp -d /tmp/tpe2e.XXXXXX)"
CFG="$WORK/cfg"
mkdir -p "$CFG/tmux-picker"
export TMUX_TMPDIR="$WORK"
unset TMUX TMUX_PANE

PASS=0
FAIL=0
pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  FAIL: $1 — $2"; }

data() { "$TMUX_BIN" "$@"; }        # private default server: the sessions under test
drv() { "$TMUX_BIN" -L drv "$@"; }  # private driver server: the picker's terminal

cleanup() {
    drv kill-server 2>/dev/null
    data kill-server 2>/dev/null
    rm -rf "$WORK"
}
trap cleanup EXIT

config() { printf '%s\n' "$@" >"$CFG/tmux-picker/config.toml"; }

# stop SERVER_FN — kill a private server and wait until it is gone: a
# new-session that races a dying server fails with "server exited
# unexpectedly".
stop() {
    "$1" kill-server 2>/dev/null
    local i
    for ((i = 0; i < 50; i++)); do
        "$1" list-sessions >/dev/null 2>&1 || return 0
        sleep 0.1
    done
}

# fresh_sessions NAME... — restart the data server with these sessions.
fresh_sessions() {
    stop data
    local i s
    for s in "$@"; do
        for ((i = 0; i < 20; i++)); do
            data new-session -d -s "$s" -x 80 -y 24 sh 2>/dev/null && break
            sleep 0.1
        done
        has_session "$s" || { echo "    (could not create session $s)"; return 1; }
    done
}

# start_picker [WIDTH HEIGHT] — run the picker in a fresh driver pane and
# wait for its first frame.
start_picker() {
    stop drv
    rm -f "$WORK/action" "$WORK/rc"
    drv new-session -d -s p -x "${1:-100}" -y "${2:-30}" \
        "env -u TMUX TMUX_TMPDIR='$WORK' XDG_CONFIG_HOME='$CFG' TERM=xterm-256color COLORTERM=truecolor \
         '$BIN' >'$WORK/action'; echo \$? >'$WORK/rc'; sleep 600"
    wait_screen 'SESSIONS|TMUX//PICKER|NO SIGNAL' 8 || {
        fail "start picker" "never drew"
        echo "    (screen:)"
        screen | sed '/^ *$/d; s/^/    | /'
        return 1
    }
}

screen() { drv capture-pane -p -t p 2>/dev/null; }

# wait_screen REGEX [SECONDS] — poll until the screen matches.
wait_screen() {
    local i
    for ((i = 0; i < ${2:-5} * 10; i++)); do
        screen | grep -qE "$1" && return 0
        sleep 0.1
    done
    return 1
}

# wait_gone REGEX [SECONDS] — poll until the screen stops matching.
wait_gone() {
    local i
    for ((i = 0; i < ${2:-5} * 10; i++)); do
        screen | grep -qE "$1" || return 0
        sleep 0.1
    done
    return 1
}

keys() {
    local k
    for k in "$@"; do
        drv send-keys -t p "$k"
        sleep 0.15
    done
}
text() { drv send-keys -t p -l "$1"; sleep 0.2; }

# raw BYTES — inject raw bytes (escape sequences) into the picker's input.
raw() {
    # shellcheck disable=SC2046
    drv send-keys -t p -H $(printf '%b' "$1" | od -An -tx1)
    sleep 0.15
}

# action [SECONDS] — the line the picker printed on exit ("" on timeout).
action() {
    local i
    for ((i = 0; i < ${1:-5} * 10; i++)); do
        if [[ -s "$WORK/rc" ]]; then
            cat "$WORK/action"
            return 0
        fi
        sleep 0.1
    done
    echo "(timeout)"
}

picker_pid() { pgrep -P "$(drv display-message -p -t p '#{pane_pid}')" 2>/dev/null | head -1; }

has_session() { data has-session -t "=$1" 2>/dev/null; }

# wait_until CMD... — poll a command for up to 5 s.
wait_until() {
    local i
    for ((i = 0; i < 50; i++)); do
        "$@" && return 0
        sleep 0.1
    done
    return 1
}

expect_action() { # NAME EXPECTED
    local got
    got="$(action)"
    if [[ "$got" == "$2" ]]; then pass "$1"; else fail "$1" "expected '$2', got '$got'"; fi
}

by_name() { keys o o o; wait_screen 'SORT ▸ by name' 3; }

echo "═══ tmux-picker TUI E2E tests ($("$BIN" --version), $("$TMUX_BIN" -V)) ═══"

# --- exits ------------------------------------------------------------------
config 'timeout_secs = 0'
fresh_sessions alpha beta gamma
for k in q Escape s C-c; do
    echo "Test: '$k' drops to shell"
    start_picker && keys "$k" && expect_action "'$k' → shell" "shell"
done

# --- selection --------------------------------------------------------------
echo "Test: 'o' cycles sort modes, Enter attaches to the highlighted row"
start_picker && by_name && keys Enter && expect_action "Enter after sort-by-name" "attach:alpha"

echo "Test: digit jumps to row N and attaches"
start_picker && by_name && keys 2 && expect_action "'2' → second row" "attach:beta"

echo "Test: j/k and arrow keys move the selection"
start_picker && by_name && keys j j k Down Up Enter && expect_action "j j k ↓ ↑ ⏎" "attach:beta"

echo "Test: fuzzy filter narrows, counts matches, and attaches"
if start_picker; then
    keys /
    text "gam"
    if wait_screen '1/3 MATCH' 3 && screen | grep -q '/gam'; then pass "filter shows /gam, 1/3 MATCH"; else fail "filter count" "no '/gam' + '1/3 MATCH'"; fi
    keys Enter
    expect_action "filter + ⏎" "attach:gamma"
fi

echo "Test: filter with no match shows NO SIGNAL; Esc restores the list"
if start_picker; then
    keys /
    text "zzz"
    if wait_screen 'NO SIGNAL' 3; then pass "no-match state"; else fail "no-match state" "no NO SIGNAL"; fi
    keys Escape
    if wait_screen 'SELECT TARGET' 3 && screen | grep -q gamma; then pass "Esc restores list"; else fail "Esc restores list" "list not back"; fi
    keys q
    expect_action "q after filter" "shell"
fi

echo "Test: a burst of keys in one write is handled in full (paste, fast typing)"
start_picker && raw "nburst\r" && expect_action "'nburst⏎' as one write" "new:burst"

# --- new session --------------------------------------------------------------
echo "Test: n + name creates a session"
start_picker && keys n && text "fresh" && keys Enter && expect_action "n fresh ⏎" "new:fresh"

echo "Test: n + existing name attaches"
start_picker && keys n && text "beta" && keys Enter && expect_action "n beta ⏎" "attach:beta"

echo "Test: n + a prefix of an existing name creates it (exact match)"
fresh_sessions alpha main
start_picker && keys n && text "ma" && keys Enter && expect_action "n ma ⏎ while 'main' exists" "new:ma"

echo "Test: n + an invalid name is sanitized"
start_picker && keys n && text "my proj!" && keys Enter && expect_action "n 'my proj!' ⏎" "new:my-proj"

# --- kill / rename --------------------------------------------------------------
fresh_sessions alpha beta gamma
echo "Test: K then y kills the highlighted session"
if start_picker && by_name; then
    keys j j K
    if wait_screen 'TERMINATE SESSION' 3; then pass "kill confirm card"; else fail "kill confirm card" "not shown"; fi
    keys y
    if wait_until bash -c "! '$TMUX_BIN' has-session -t =gamma 2>/dev/null"; then pass "gamma killed"; else fail "kill" "gamma still exists"; fi
    if wait_gone 'gamma' 3; then pass "list refreshed after kill"; else fail "list refresh" "gamma still listed"; fi
    if has_session alpha && has_session beta; then pass "other sessions untouched"; else fail "kill scope" "alpha/beta gone"; fi
    keys q
    expect_action "q after kill" "shell"
fi

fresh_sessions alpha beta gamma
echo "Test: K then any other key cancels"
if start_picker && by_name; then
    keys j j K n
    if wait_gone 'TERMINATE SESSION' 3 && has_session gamma; then pass "kill cancelled"; else fail "kill cancel" "gamma gone or card stuck"; fi
    keys q
fi

echo "Test: r renames the highlighted session"
if start_picker && by_name; then
    keys j r BSpace BSpace BSpace BSpace BSpace BSpace
    text "beta2"
    keys Enter
    if wait_until has_session beta2 && ! has_session beta; then pass "beta → beta2"; else fail "rename" "beta2 missing or beta remains"; fi
    if wait_screen 'beta2' 3; then pass "list shows renamed session"; else fail "rename refresh" "beta2 not listed"; fi
    keys q
fi

# --- views ------------------------------------------------------------------------
fresh_sessions alpha beta gamma
echo "Test: ? opens the help card, Esc closes it"
if start_picker; then
    keys '?'
    if wait_screen 'COMMAND REFERENCE' 3 && screen | grep -q 'PICK MODE'; then pass "help card"; else fail "help card" "not shown"; fi
    keys Escape
    if wait_gone 'PICK MODE' 3; then pass "help closes"; else fail "help close" "still shown"; fi
    keys q
    expect_action "q after help" "shell"
fi

echo "Test: Tab toggles the feed and the windows view"
if start_picker; then
    keys Tab
    if wait_screen 'WINDOWS ▸' 3; then pass "windows view"; else fail "windows view" "not shown"; fi
    keys Tab
    if wait_screen 'FEED ▸' 3; then pass "feed view"; else fail "feed view" "not shown"; fi
    keys q
fi

echo "Test: the feed follows a busy pane live"
data new-session -d -s ticker -x 80 -y 24 "i=0; while :; do i=\$((i+1)); echo tick-\$i; sleep 0.3; done"
if start_picker; then
    keys /
    text "ticker"
    if wait_screen 'tick-[0-9]+' 4; then
        first="$(screen | grep -oE 'tick-[0-9]+' | sed 's/tick-//' | sort -n | tail -1)"
        sleep 2
        second="$(screen | grep -oE 'tick-[0-9]+' | sed 's/tick-//' | sort -n | tail -1)"
        if [[ "${second:-0}" -gt "${first:-0}" ]]; then pass "feed advanced tick-$first → tick-$second"; else fail "live feed" "stuck at tick-$first"; fi
    else
        fail "live feed" "no tick lines"
    fi
    keys Escape q
fi
data kill-session -t =ticker 2>/dev/null

# --- mouse ----------------------------------------------------------------------------
row_of() { screen | grep -n "$1" | head -1 | cut -d: -f1; }

echo "Test: double-click attaches to the clicked row"
if start_picker && by_name; then
    y="$(row_of gamma)"
    click="\e[<0;20;${y}M\e[<0;20;${y}m"
    raw "$click$click"
    expect_action "double-click on gamma (row $y)" "attach:gamma"
fi

echo "Test: mouse wheel moves the selection"
if start_picker && by_name; then
    raw "\e[<65;20;10M"
    keys Enter
    expect_action "wheel down + ⏎" "attach:beta"
fi

# --- auto-attach ------------------------------------------------------------------------
echo "Test: auto-attach countdown never picks an attached session"
config 'timeout_secs = 1'
stop drv
drv new-session -d -s client -x 80 -y 24 "env -u TMUX TMUX_TMPDIR='$WORK' '$TMUX_BIN' attach -t =alpha"
wait_until bash -c "'$TMUX_BIN' list-clients -F '#{session_name}' | grep -qx alpha"
rm -f "$WORK/action" "$WORK/rc"
drv new-window -d -t client: -n p "env -u TMUX TMUX_TMPDIR='$WORK' XDG_CONFIG_HOME='$CFG' TERM=xterm-256color '$BIN' >'$WORK/action'; echo \$? >'$WORK/rc'; sleep 600"
got="$(action 6)"
case "$got" in
    attach:beta | attach:gamma) pass "auto-attached to a detached session ($got)" ;;
    *) fail "auto-attach" "got '$got'" ;;
esac
config 'timeout_secs = 0'

# --- robustness ------------------------------------------------------------------------------
echo "Test: SIGHUP reloads the config in place"
if start_picker; then
    kill -HUP "$(picker_pid)"
    if wait_screen 'config reloaded' 3; then pass "SIGHUP reload flash"; else fail "SIGHUP reload" "no flash"; fi
    keys q
    expect_action "q after reload" "shell"
fi

echo "Test: closing the terminal ends the picker (no detached-pty wedge)"
if start_picker; then
    pid="$(picker_pid)"
    drv kill-server
    gone=0
    for _ in $(seq 1 30); do
        # A zombie has exited too (its shell parent died with the pty, and
        # some containers have no init to reap orphans).
        if ! kill -0 "$pid" 2>/dev/null || [[ "$(ps -o stat= -p "$pid" 2>/dev/null)" == Z* ]]; then
            gone=1
            break
        fi
        sleep 0.1
    done
    if [[ $gone -eq 1 ]]; then pass "picker exited after hangup"; else
        fail "hangup" "pid $pid still running"
        kill -9 "$pid" 2>/dev/null
    fi
fi

echo "Test: resizing keeps rendering"
if start_picker 100 30; then
    drv resize-window -t p -x 50 -y 12
    sleep 0.4
    if wait_screen 'alpha' 3; then pass "shrink to 50x12"; else fail "shrink" "list gone"; fi
    drv resize-window -t p -x 160 -y 45
    if wait_screen 'FEED ▸' 3 && screen | grep -q 'NETRUNNER'; then pass "grow to 160x45 (banner + split)"; else fail "grow" "layout missing"; fi
    keys q
    expect_action "q after resizes" "shell"
fi

echo "Test: tiny terminal still works"
if start_picker 24 6; then
    keys q
    expect_action "q at 24x6" "shell"
fi

echo "Test: animations off + 256 colours send no truecolor"
config 'timeout_secs = 0' '[theme]' 'animations = false' 'color = "256"'
if start_picker; then
    sleep 0.5
    out="$(drv capture-pane -e -p -t p)"
    if ! grep -qE '\[(38|48);2;' <<<"$out" && grep -qE '\[38;5;' <<<"$out"; then
        pass "only 256-colour escapes"
    else
        fail "256-colour mode" "truecolor escapes present"
    fi
    keys q
fi
config 'timeout_secs = 0'

if [[ -r /proc/self/stat ]]; then
    echo "Test: idle CPU stays low with effects on"
    if start_picker; then
        pid="$(picker_pid)"
        hz="$(getconf CLK_TCK)"
        t0="$(awk '{print $14 + $15}' "/proc/$pid/stat")"
        sleep 2
        t1="$(awk '{print $14 + $15}' "/proc/$pid/stat")"
        pct=$(((t1 - t0) * 100 / (2 * hz)))
        if [[ $pct -lt 15 ]]; then pass "idle CPU ${pct}%"; else fail "idle CPU" "${pct}%"; fi
        keys q
    fi
fi

echo "Test: no tmux server → new:main"
stop data
got="$(env XDG_CONFIG_HOME="$CFG" "$BIN" </dev/null 2>/dev/null)"
if [[ "$got" == "new:main" ]]; then pass "no server → new:main"; else fail "no server" "got '$got'"; fi

# --- the login hook, end to end ----------------------------------------------------------------
# The hook hard-codes /usr/bin/tmux, so this part needs tmux there.
if [[ -x /usr/bin/tmux ]]; then
    FAKEHOME="$WORK/home"
    mkdir -p "$FAKEHOME/.local/bin"
    ln -sf "$BIN" "$FAKEHOME/.local/bin/tmux-picker"
    printf 'source %q\n' "$REPO_ROOT/shell/tmux-autoattach.sh" >"$WORK/rc.sh"
    login_shell() {
        stop drv
        drv new-session -d -s p -x 100 -y 30 \
            "env -u TMUX -u SSH_CONNECTION HOME='$FAKEHOME' XDG_CONFIG_HOME='$CFG' TMUX_TMPDIR='$WORK' TERM=xterm-256color \
             bash --noprofile --rcfile '$WORK/rc.sh' -i"
        wait_screen 'SESSIONS' 8 || {
            echo "    (login shell never showed the picker; screen:)"
            screen | sed '/^ *$/d; s/^/    | /'
            return 1
        }
    }
    client_on() { "$TMUX_BIN" list-clients -F '#{session_name}' 2>/dev/null | grep -qx "$1"; }

    echo "Test: hook attaches the picked session in a real login shell"
    fresh_sessions alpha beta main
    if login_shell; then
        keys /
        text "beta"
        keys Enter
        if wait_until client_on beta; then pass "hook attached to beta"; else fail "hook attach" "no client on beta"; fi
    else
        fail "hook attach" "picker never appeared"
    fi

    echo "Test: hook creates a new session whose name prefixes an existing one"
    if login_shell; then
        keys n
        text "ma"
        keys Enter
        if wait_until client_on ma && has_session main; then pass "hook created 'ma' (not 'main')"; else fail "hook new" "no client on 'ma'"; fi
    else
        fail "hook new" "picker never appeared"
    fi
else
    echo "  SKIP: login-hook tests need /usr/bin/tmux"
fi

echo ""
echo "═══ Results: $PASS passed, $FAIL failed ═══"
[[ $FAIL -eq 0 ]]
