# Changelog

All notable changes to tmux-picker are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/); versions match
git tags.

## [Unreleased]

### Neon UI and a faster core

The picker is redrawn as a cyberpunk HUD in the style of the `update`
dashboard, and every tmux round trip on the login path got cheaper.

#### Added

- **Neon renderer** (`src/ui/`): gradient heavy-line panels with tabbed
  titles, a block-letter `TMUX // PICKER` banner that decrypts on start and
  glitches now and then, a pulsing status line, CRT scanlines, a selection
  beam (hot pink at the left edge fading to violet, pink/cyan caps in the
  box gutters, muted columns brightened, a light sweep), a gradient
  auto-attach gauge that names the session it will pick, a `NO SIGNAL` rain
  state, and modal cards for help and kill confirmation. Wide terminals
  (≥140 columns) put the feed beside the list; short ones drop the banner,
  then the boxed gauge, then the feed.
- **Theme keys**: `primary`, `secondary`, `highlight`, `success`, `text`,
  `background` (accepts `"reset"`), `animations`, `scanlines`, and
  `color = "auto" | "truecolor" | "256"`. Auto mode uses truecolor when
  `$COLORTERM`/`$TERM` advertise it and the nearest xterm-256 colour
  otherwise. The old starter values (`accent = "cyan"`, `warning = "red"`,
  `selection_bg = "darkgray"`) are read as "use the default", so existing
  configs get the new look.
- **Live feed**: the preview re-captures the selected pane every second
  and is fetched immediately on selection change (it used to wait for the
  next tick and showed "(unavailable)" meanwhile).
- **List scrolling** with a scrollbar, so selections past the visible rows
  stay on screen; mouse clicks map through the scroll offset; the mouse
  wheel moves the selection.

#### Fixed

- **Prefix matching**: every session target is now exact (`-t =name`).
  tmux falls back to prefix matching, so typing a new name like `ma` while
  `main` existed attached to `main`, and `show`/`label`/`kill` could hit the
  wrong session. The shell hook's `attach-session` is exact too.
- **Stale preview**: `capture-pane -S -6` started six lines up in the
  scrollback, so the preview showed history above the screen instead of the
  newest output. It now tails the visible screen.
- **Trailing `;` in values**: tmux reads an argument ending in `;` as a
  command separator, so `label --purpose "fix;"` stored `fix`. Values are
  escaped now.
- Window names containing `|` no longer break the windows view.
- **Dropped input**: when one read from the terminal held several events
  (a double-click, a paste, fast typing over SSH), only the first was
  handled until the next keypress; `/gamma` typed quickly filtered on `g`.
  The loop now drains every queued event, pausing only to re-fetch the list
  after a kill or rename.
- **tmux 3.4** (Ubuntu 24.04) octal-escapes control characters in `-p`
  output; chained-command separators are printable now, so the startup
  query works there.

#### Tests

- `tests/tui_e2e.sh`: 46 end-to-end checks that run the release binary in
  a real terminal (a private tmux server) and drive it with keys, mouse
  sequences, signals and resizes: every exit path, sorting, digit and
  arrow navigation, filtering, new/kill/rename against a live tmux, help
  and windows views, the live feed, double-click and wheel, auto-attach,
  SIGHUP reload, hangup, resizes, 256-colour mode, idle CPU, and the login
  hook attaching for real.
- CI installs tmux before running the tests (macOS integration tests had
  never run), the tests find tmux on PATH, and CI runs the TUI suite.

#### Performance

- `run_tmux` no longer sleep-polls `try_wait` in 10 ms steps: it drains
  stdout/stderr with `poll(2)` and returns as soon as tmux exits (this also
  removes a latent deadlock on outputs larger than a pipe buffer).
- The startup query runs `list-panes` and `list-sessions` in one tmux
  process. Measured time to first frame: 32.6 ms → 8.8 ms.
- `show` is one `display-message` call (was five processes): 52 ms → 2 ms.
  `label` and `auto` are two calls each: 42 ms → 3.5 ms and 62 ms → 3.5 ms.
- The windows view captures every pane in one chained tmux call (was one
  process per window).
- Fuzzy filtering reuses one decode buffer (no per-keystroke allocation);
  marker lookup no longer lowercases patterns per comparison.
- Release profile: fat LTO, one codegen unit, stripped. Binary
  2.36 MB → 1.47 MB.
- Effects redraw at 10 fps with banner colour drift quantised to 2 Hz:
  about 25 KiB/s of terminal output in truecolor, 10 KiB/s in 256-colour,
  and the old ~1 KiB/s with `animations = false`.

### Optimization, elegance, and presentation pass

No behavior change.

#### Changed

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
