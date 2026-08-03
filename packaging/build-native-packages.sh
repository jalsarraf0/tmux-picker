#!/usr/bin/env bash
# Build .deb and .rpm packages for tmux-picker using fpm.
#
# Both packages install:
#   /usr/bin/tmux-picker
#   /usr/share/tmux-picker/tmux-autoattach.sh
#   /usr/share/doc/tmux-picker/{LICENSE,README.md}
#
# and wire the auto-attach hook into whichever system-wide interactive bash
# rc file exists at install time (/etc/bashrc on Fedora/RHEL,
# /etc/bash.bashrc on Debian/Ubuntu) via packaging/scripts/postinstall.sh —
# NOT /etc/profile.d, which only fires for login shells and would silently
# fail to deliver local-terminal auto-attach on non-Fedora systems.
#
# Requires: fpm (https://fpm.readthedocs.io), a built release binary.
#
# Usage: packaging/build-native-packages.sh [output-dir]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/dist}"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BINARY="$TARGET_DIR/release/tmux-picker"

log() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

command -v fpm >/dev/null || die "fpm not found — https://fpm.readthedocs.io/en/latest/installing.html"
[[ -x "$BINARY" ]] || die "release binary not found at $BINARY — run 'cargo build --release' first"

VERSION="$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/version = "(.*)"/\1/')"
[[ -n "$VERSION" ]] || die "could not read version from Cargo.toml"
log "packaging tmux-picker $VERSION"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/usr/bin"
mkdir -p "$STAGE/usr/share/tmux-picker"
mkdir -p "$STAGE/usr/share/doc/tmux-picker"

install -m 0755 "$BINARY" "$STAGE/usr/bin/tmux-picker"
install -m 0644 "$REPO_ROOT/shell/tmux-autoattach.sh" "$STAGE/usr/share/tmux-picker/tmux-autoattach.sh"
install -m 0644 "$REPO_ROOT/LICENSE" "$STAGE/usr/share/doc/tmux-picker/LICENSE"
install -m 0644 "$REPO_ROOT/README.md" "$STAGE/usr/share/doc/tmux-picker/README.md"

mkdir -p "$OUT_DIR"

COMMON_ARGS=(
    -f
    -s dir
    -n tmux-picker
    -v "$VERSION"
    --license MIT
    --maintainer "jalsarraf <19882582+jalsarraf0@users.noreply.github.com>"
    --url "https://github.com/jalsarraf0/tmux-picker"
    --description "TUI session picker for tmux on SSH login and local terminals"
    --after-install "$REPO_ROOT/packaging/scripts/postinstall.sh"
    --before-remove "$REPO_ROOT/packaging/scripts/preremove.sh"
    -C "$STAGE"
)

log "building .deb"
fpm "${COMMON_ARGS[@]}" \
    -t deb \
    --depends tmux \
    -p "$OUT_DIR/tmux-picker_${VERSION}_amd64.deb" \
    usr

log "building .rpm"
fpm "${COMMON_ARGS[@]}" \
    -t rpm \
    --depends tmux \
    --rpm-summary "TUI session picker for tmux on SSH login and local terminals" \
    -p "$OUT_DIR/tmux-picker-${VERSION}-1.x86_64.rpm" \
    usr

log "done:"
ls -la "$OUT_DIR"/tmux-picker_"${VERSION}"_amd64.deb "$OUT_DIR"/tmux-picker-"${VERSION}"-1.x86_64.rpm
