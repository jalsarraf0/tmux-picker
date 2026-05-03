#!/usr/bin/env bash
set -euo pipefail

# scripts/release.sh
#
# Drives the v1.1.0 (or whatever Cargo.toml says) release of tmux-picker:
#   1. verify clean tree on main + version sanity
#   2. kitchen-sink test gate (fmt, clippy, test, e2e)
#   3. build the release binary and pack the binstall-shaped tarball
#   4. cargo publish --dry-run (always), then the real publish (prompt)
#   5. signed git tag + push tag to origin (prompt)
#   6. gh release create with the tarball + LICENSE + README (prompt)
#   7. print AUR upload instructions (manual, no prompt)
#
# Each network/irreversible step has its own y/N confirm. Missing tools
# (gh, cargo-publish creds, signing key) abort with a clear message
# rather than silently skipping.
#
# REPO is derived from the script's own location, so `sudo bash ./release.sh`
# resolves to the same checkout as `./release.sh`.
#
# Usage:
#   scripts/release.sh
#   scripts/release.sh --skip-tests   # tests already green; just publish
#   scripts/release.sh --dry-run      # everything except push/publish

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="x86_64-unknown-linux-gnu"
SKIP_TESTS=0
DRY_RUN=0

for arg in "$@"; do
    case "$arg" in
        --skip-tests) SKIP_TESTS=1 ;;
        --dry-run)    DRY_RUN=1 ;;
        -h|--help)
            sed -n '4,25p' "$0"
            exit 0
            ;;
        *)
            echo "unknown flag: $arg" >&2
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

step()    { printf '\n=== %s ===\n' "$*"; }
note()    { printf '  %s\n' "$*"; }
abort()   { printf 'ABORT: %s\n' "$*" >&2; exit 1; }

confirm() {
    # confirm "<question>" -> exit 0 yes, exit 1 no
    local q="$1"
    local ans
    if (( DRY_RUN )); then
        note "(--dry-run) skipping: $q"
        return 1
    fi
    read -r -p "$q [y/N] " ans
    [[ "$ans" =~ ^[Yy]$ ]]
}

cargo_target_dir() {
    printf '%s' "${CARGO_TARGET_DIR:-${REPO}/target}"
}

# ---------------------------------------------------------------------------
# 1. Pre-flight
# ---------------------------------------------------------------------------

step "Pre-flight"
[[ -d "$REPO" ]] || abort "repo not found at $REPO"
cd "$REPO"

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || abort "not a git repo"

branch=$(git rev-parse --abbrev-ref HEAD)
[[ "$branch" == "main" ]] || abort "on branch '$branch', expected 'main'"
note "branch: $branch"

if ! git diff --quiet || ! git diff --cached --quiet; then
    abort "working tree is dirty; commit or stash before release"
fi
note "working tree: clean"

VERSION=$(awk -F\" '/^version *= *"/ {print $2; exit}' Cargo.toml)
[[ -n "$VERSION" ]] || abort "could not parse version from Cargo.toml"
note "Cargo.toml version: $VERSION"
TAG="v${VERSION}"

if git rev-parse --verify --quiet "refs/tags/${TAG}" >/dev/null; then
    abort "tag $TAG already exists locally — bump version or delete the tag"
fi
note "tag $TAG: not yet present"

if [[ ! -f LICENSE ]]; then
    abort "LICENSE missing — cargo publish will refuse"
fi

# ---------------------------------------------------------------------------
# 2. Kitchen-sink test gate
# ---------------------------------------------------------------------------

step "Test gate"
if (( SKIP_TESTS )); then
    note "(--skip-tests) skipping fmt/clippy/test/e2e"
else
    note "cargo fmt --check"
    cargo fmt --check
    note "cargo clippy -D warnings"
    cargo clippy --all-targets -- -D warnings
    note "cargo test"
    cargo test
    note "tests/e2e.sh"
    bash tests/e2e.sh
fi

# ---------------------------------------------------------------------------
# 3. Build + package
# ---------------------------------------------------------------------------

step "Release build"
cargo build --release
TARGET_DIR="$(cargo_target_dir)"
BIN="${TARGET_DIR}/release/tmux-picker"
[[ -x "$BIN" ]] || abort "release binary missing at $BIN"
note "binary: $BIN ($(du -h "$BIN" | cut -f1))"

step "Pack release tarball"
STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT
TAR_NAME="tmux-picker-${VERSION}-${TARGET_TRIPLE}"
TAR_DIR="${STAGE_ROOT}/${TAR_NAME}"
mkdir -p "$TAR_DIR"
cp "$BIN"                     "${TAR_DIR}/tmux-picker"
cp shell/tmux-autoattach.sh   "${TAR_DIR}/tmux-autoattach.sh"
cp LICENSE                    "${TAR_DIR}/LICENSE"
cp README.md                  "${TAR_DIR}/README.md"

TAR_PATH="${REPO}/dist/${TAR_NAME}.tar.gz"
mkdir -p "${REPO}/dist"
tar -C "$STAGE_ROOT" -czf "$TAR_PATH" "$TAR_NAME"
note "tarball: $TAR_PATH ($(du -h "$TAR_PATH" | cut -f1))"

CHECKSUM=$(sha256sum "$TAR_PATH" | awk '{print $1}')
note "sha256: $CHECKSUM"

# ---------------------------------------------------------------------------
# 4. Cargo publish (dry-run always; real publish on confirm)
# ---------------------------------------------------------------------------

step "cargo publish --dry-run"
cargo publish --dry-run --allow-dirty

if confirm "Run real cargo publish to crates.io? (irreversible — name + version are permanent)"; then
    cargo publish --allow-dirty
    note "published $VERSION to crates.io"
else
    note "skipped cargo publish"
fi

# ---------------------------------------------------------------------------
# 5. Tag + push
# ---------------------------------------------------------------------------

step "git tag $TAG"
SIGNING_KEY=$(git config --get user.signingkey || true)
if [[ -n "$SIGNING_KEY" ]]; then
    git tag -s "$TAG" -m "Release $TAG"
    note "signed tag created"
else
    git tag "$TAG" -m "Release $TAG"
    note "unsigned tag created (no user.signingkey set)"
fi

if confirm "Push tag $TAG to origin?"; then
    REMOTE=$(git config --get remote.origin.url || true)
    [[ -n "$REMOTE" ]] || abort "no origin remote configured"
    note "remote: $REMOTE"
    git push origin "$TAG"
    note "pushed tag $TAG"
else
    note "skipped tag push (delete with: git tag -d $TAG)"
fi

# ---------------------------------------------------------------------------
# 6. GitHub release
# ---------------------------------------------------------------------------

step "GitHub release"
if ! command -v gh >/dev/null 2>&1; then
    note "gh CLI not installed — skipping. Manual upload:"
    note "  https://github.com/jalsarraf0/tmux-picker/releases/new?tag=$TAG"
elif ! gh auth status >/dev/null 2>&1; then
    note "gh not authenticated — skipping. Run: gh auth login"
elif confirm "Create GitHub release $TAG and upload the tarball?"; then
    NOTES=$(cat <<EOF
tmux-picker $VERSION

See \`docs/superpowers/specs/2026-05-03-distribution-design.md\` and the
README for what's new in this release. Tarball below contains a
prebuilt \`tmux-picker\` for \`$TARGET_TRIPLE\` along with the bash
auto-attach stub, LICENSE, and README.

sha256: \`$CHECKSUM\`
EOF
)
    gh release create "$TAG" "$TAR_PATH" \
        --title "tmux-picker $VERSION" \
        --notes "$NOTES"
    note "release published"
else
    note "skipped GitHub release"
fi

# ---------------------------------------------------------------------------
# 7. AUR (manual)
# ---------------------------------------------------------------------------

step "AUR (manual)"
note "PKGBUILD: ${REPO}/packaging/PKGBUILD"
note "After the GitHub release exists:"
note "  1. cd ~/aur/tmux-picker-bin   (clone if needed: git clone ssh://aur@aur.archlinux.org/tmux-picker-bin.git)"
note "  2. cp ${REPO}/packaging/PKGBUILD ./PKGBUILD"
note "  3. updpkgsums                  # rewrite sha256sums in place"
note "  4. makepkg --printsrcinfo > .SRCINFO"
note "  5. git add PKGBUILD .SRCINFO"
note "  6. git commit -m 'tmux-picker-bin $VERSION-1'"
note "  7. git push"

step "Done"
note "v$VERSION released. Local artefact: $TAR_PATH"
