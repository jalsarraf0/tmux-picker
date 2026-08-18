# Changelog

All notable changes to tmux-picker are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/); versions match
git tags.

## [Unreleased]

Optimization, elegance, and presentation pass — no behavior change.

### Changed

- `list_sessions_with_markers` folds marker discovery into the existing
  `list-panes` query, removing a second subprocess spawn from every picker
  refresh tick.
- Sort paths that key on `name.to_lowercase()` switched from `sort_by_key`
  to `sort_by_cached_key`, so the lowercasing allocation happens once per
  element instead of on every comparison.
- Reduced string clones across `tmux.rs`/`ui.rs` parsing and rendering
  paths in favor of borrowing.
- Full `clippy::pedantic` pass: zero warnings on `cargo clippy --all-targets
  --all-features -- -D warnings`, all fixes applied idiomatically rather
  than suppressed.
- Added module- and public-API-level doc comments across `src/`.
- Added a CI workflow (`.github/workflows/ci.yml`): fmt, clippy, build+test
  on Linux and macOS, e2e tests, and shellcheck on every push/PR.
- Rewrote `README.md` for a punchier first screen (tagline, badges, a
  static TUI preview, a feature/comparison-table lead) and moved the
  AI-agent install prompts and maintainer release checklist to
  `docs/AI_AGENT_INSTALL.md` and `docs/RELEASING.md` respectively.

## [1.2.1] - 2026-08-03

The repo went public today and picked up a full local-terminal auto-attach
feature, a consent-gated installer, and native packages (rpm/deb/AUR),
alongside a security/correctness review of all of it.

### Added

- **`trigger_mode` config key** (`"always"` | `"ssh_only"`, default
  `"always"`): the auto-attach hook now fires on every new interactive
  shell — SSH login **and** local terminal windows in any emulator — not
  just SSH like before. `"ssh_only"` restores the original behaviour.
  New `--print-trigger-mode` CLI flag (internal, used by the shell hook).
- **`scripts/install.sh --auto-deps`**: detects a missing `tmux` or Rust
  toolchain and installs them — `tmux` via whichever package manager is
  present (`apt`/`dnf`/`pacman`/`zypper`/`apk`/`brew`), a Rust toolchain via
  [rustup](https://rustup.rs) (distro `cargo` packages are frequently too
  old for this project's `edition = "2024"`).
- **Consent gates on both of the above**: `scripts/install.sh` asks
  interactively (numbered menu / y-N prompt), accepts explicit
  `--trigger-mode=`/`--auto-deps`/`--no-auto-deps` flags for non-interactive
  use, and — run non-interactively with neither — refuses and prints
  instructions telling an AI agent to stop and ask the human first rather
  than guess. `--trigger-mode=always --auto-deps` is a fully hands-off
  install for automation.
- **Native packages**: `.rpm` (Fedora/RHEL/openSUSE) and `.deb`
  (Debian/Ubuntu) built via `packaging/build-native-packages.sh` (uses
  `fpm`), attached to the GitHub release. Both wire the hook into the
  system-wide interactive bash rc (`/etc/bashrc` / `/etc/bash.bashrc`)
  automatically on install — verified end-to-end in real containers
  (install, upgrade, and genuine removal).
- **Homebrew formula** (`packaging/homebrew/tmux-picker.rb`) — builds from
  source via cargo; sha256-pinned to the real v1.2.1 source tarball, but
  not build-tested on real macOS (none available in this environment).
- `tests/install_test.sh`: 16 regression tests covering the `trigger_mode`
  and `--auto-deps` consent gates, both dependency-install dispatch paths,
  and both failure paths — all mocked so the suite never touches real
  system packages.
- `CHANGELOG.md` (this file).

### Fixed

- **AUR `PKGBUILD`**: previously wired the hook via `/etc/profile.d`, which
  only fires for *login* shells — silently failing to deliver local-terminal
  auto-attach anywhere except Fedora (where `/etc/bashrc` happens to source
  `profile.d` too). Replaced with `packaging/tmux-picker.install` using
  pacman's dedicated `post_install`/`post_upgrade`/`pre_remove` hooks.
- **rpm/deb upgrade race**: an early version of the native-package
  `--before-remove` script unconditionally stripped the rc-file hook block
  on any invocation. Since rpm/dnf run the *new* package's `%post` before
  the *old* package's `%preun` during an upgrade, this left the hook
  permanently stripped after every single upgrade. Fixed by checking the
  remove-vs-upgrade argument each format passes (rpm: `$1`, deb: `$1` word)
  before stripping — caught via real container testing, not inspection.
- `${array[*]}` joined with a multi-char `IFS` in `install.sh`'s
  missing-dependency message — bash only honours the first `IFS` character
  for joins, so it printed `"cargo,tmux"` with no space. Added a proper
  `join_with()` helper.
- A failed `tmux` install or rustup fetch under `--auto-deps` previously
  aborted via a raw `set -e` exit with no explanation. Both now produce a
  clear error message.

### Changed

- Repository visibility: private → public.
- README substantially expanded: by-hand install instructions for every
  format (crates.io, cargo-binstall, rpm, deb, AUR, Homebrew, source),
  a dedicated AI-agent install section (Claude Code / Codex prompts that
  explicitly instruct the agent to ask before choosing `trigger_mode` or
  `--auto-deps`), an MIT/no-warranty callout, and a corrected maintainer
  release checklist.
- `packaging/PKGBUILD`: hook now installs to `/usr/share/tmux-picker/` (data
  location) instead of directly to `/etc/profile.d/`.

### Known gaps

- Not published to crates.io — `cargo publish` is blocked on an
  expired/invalid account token, unrelated to the code.
- Not submitted to the live AUR — needs an AUR account (email verification
  + CAPTCHA) and a registered SSH key, both of which have to go through the
  repo owner directly. A dedicated SSH keypair is generated and ready
  (`~/.ssh/aur` on the maintainer's machine) for whenever that's done.

## [1.1.0] - 2026-05-03

Distribution prep: LICENSE, `--init`, PKGBUILD, cargo-binstall metadata,
fuzzy filter + mouse + SIGHUP config reload, process markers/activity dot,
rename/sort/yank/auto-label, user TOML config with theme overrides, preview
pane, filter mode, and kill-with-confirm. See `git log v1.0.0..v1.1.0` for
the full commit-by-commit history (39 commits).

## [1.0.0] - 2026-04-07

Initial release: TUI session picker for tmux with SSH-login auto-attach,
per-session metadata (label/project/purpose), and the core picker/attach/
new-session flow.
