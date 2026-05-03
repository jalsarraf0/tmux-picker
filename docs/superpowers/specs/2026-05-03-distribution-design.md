# tmux-picker: Distribution

**Date:** 2026-05-03
**Status:** Draft
**Author:** jalsarraf + Claude
**Sub-project:** #9 of 9

## Problem

Phases 5–8 took the picker from a personal tool to something everyone could
plausibly use. To actually let "everyone" install it, four pieces are
missing:

1. **Crates.io readiness.** `Cargo.toml` lacks the metadata required by
   `cargo publish` (description is present, but no license, repo URL,
   keywords, categories, or authors).
2. **License file.** No `LICENSE` in the repo root means downstream
   packagers cannot redistribute. MIT is the natural choice for a small
   developer tool.
3. **Friendly first-run.** `tmux-picker --init` should write a
   commented starter `~/.config/tmux-picker/config.toml` so users can
   discover what they can tune without reading source code.
4. **Distribution side-paths.** Even on platforms where `cargo install`
   works, users prefer their package manager. `cargo-binstall` metadata
   plus an AUR `PKGBUILD` cover Rust users and Arch users without us
   maintaining anything beyond a GitHub release.

## Solution

Five pieces of work, all in one phase because they are tightly coupled
release prep:

1. **`Cargo.toml` package metadata.** Set `license = "MIT"`,
   `repository`, `homepage`, `keywords` (≤5), `categories`, `authors`,
   and `readme = "README.md"`. Bump `version = "1.1.0"` to reflect the
   feature work since 1.0.0.
2. **`LICENSE` file** — MIT, copyright the project author.
3. **`tmux-picker --init`.** Top-level CLI flag (parallel to
   `--check-config`). Writes the starter config to the resolved path
   (`$XDG_CONFIG_HOME/tmux-picker/config.toml` if set, else
   `$HOME/.config/tmux-picker/config.toml`). Refuses to overwrite an
   existing file unless the user passes `--force`.
4. **`packaging/PKGBUILD`** for AUR — `pkgname=tmux-picker-bin`,
   pulls the GitHub release tarball, drops the binary in
   `/usr/bin/tmux-picker`, installs the bash stub to
   `/etc/profile.d/tmux-autoattach.sh`. License = MIT.
5. **`cargo-binstall` metadata.** Adds
   `[package.metadata.binstall]` block in `Cargo.toml` so `cargo
   binstall tmux-picker` Just Works once a release is uploaded.
6. **README distribution section.** Documents `cargo install
   tmux-picker`, `cargo binstall tmux-picker`, AUR install, and the
   manual install path. Adds a "Releasing" subsection for the
   maintainer (cargo publish + tag + GitHub release notes).

Out of scope (deferred entirely):

- `asciinema` recording. Generating one requires actually running the
  picker against a live tmux server in a controlled terminal; the
  artifact would be stale by the next feature anyway. README links to
  a placeholder section.
- Running `cargo publish` itself. That's an out-of-band operation by
  the maintainer (network, credential side effect).
- Homebrew formula. AUR + binstall + cargo install + manual is
  enough.

## Architecture

### `--init`

```rust
#[derive(Parser, Debug)]
pub struct Cli {
    #[arg(long)]
    pub check_config: bool,

    /// Write a starter ~/.config/tmux-picker/config.toml. Refuses to
    /// overwrite an existing file unless `--force` is also passed.
    #[arg(long)]
    pub init: bool,

    /// With `--init`, overwrite an existing config file.
    #[arg(long, requires = "init")]
    pub force: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}
```

`run_init(force: bool) -> ExitCode`:

- Resolve config path (reuses `Config::config_path` — needs to be made
  `pub` or duplicated in main).
- Create the parent directory if missing.
- If the file exists and `!force`, print a clear error and exit 1.
- Write the starter content (a `const &str` baked into the binary).
- Print the resolved path so the user knows where to look.

Starter content lives in `src/config.rs::STARTER_TOML` so the field
list stays close to the parser:

```toml
# tmux-picker — starter config
# Every key is optional. Delete or comment any line to fall back to the
# default.

# Auto-attach countdown for the most-recent detached session, in seconds.
# 0 disables auto-attach (the picker waits for a manual choice).
timeout_secs = 10

# Theme overrides. Values: black|red|green|yellow|blue|magenta|cyan|white,
# darkgray (alias gray/grey), light{red,green,yellow,blue,magenta,cyan},
# 256-colour indexes ("196" or 196), or hex like "#ff8800" / "#abc".
[theme]
accent = "cyan"            # numbers, attached marker, prompt accents
warning = "red"            # kill-confirm prompt, error highlights
selection_bg = "darkgray"  # highlighted row background

# Process markers. The first matching pattern wins; user patterns are
# checked before the built-in defaults. Set disable_defaults to drop
# the built-ins entirely.
[markers]
disable_defaults = false

# [markers.patterns]
# foo = "★"          # any pane running `foo` gets a ★
# "my-tool" = "🚀"
```

### Cargo.toml metadata

```toml
[package]
name = "tmux-picker"
version = "1.1.0"
edition = "2024"
description = "TUI session picker for tmux on SSH login"
authors = ["jalsarraf"]
license = "MIT"
repository = "https://github.com/jalsarraf/tmux-picker"
homepage = "https://github.com/jalsarraf/tmux-picker"
readme = "README.md"
keywords = ["tmux", "tui", "ratatui", "session", "picker"]
categories = ["command-line-utilities"]

[package.metadata.binstall]
pkg-url = "{repo}/releases/download/v{version}/{name}-{version}-{target}.tar.gz"
bin-dir = "{name}-{version}-{target}/{bin}"
pkg-fmt = "tgz"
```

### PKGBUILD

```bash
# Maintainer: jalsarraf
pkgname=tmux-picker-bin
pkgver=1.1.0
pkgrel=1
pkgdesc="TUI session picker for tmux on SSH login"
arch=('x86_64')
url="https://github.com/jalsarraf/tmux-picker"
license=('MIT')
depends=('tmux')
provides=('tmux-picker')
conflicts=('tmux-picker')
source=(
    "$pkgname-$pkgver.tar.gz::$url/releases/download/v$pkgver/tmux-picker-$pkgver-x86_64-unknown-linux-gnu.tar.gz"
)
sha256sums=('SKIP')

package() {
    install -Dm755 "$srcdir/tmux-picker" "$pkgdir/usr/bin/tmux-picker"
    install -Dm644 "$srcdir/tmux-autoattach.sh" \
        "$pkgdir/etc/profile.d/tmux-autoattach.sh"
    install -Dm644 "$srcdir/LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

### LICENSE

Standard MIT, copyright `jalsarraf`.

### README updates

New sections:
- **Install** with three subsections: `cargo install`, `cargo binstall`,
  AUR, manual. The existing build/install snippet becomes the manual
  path.
- **Releasing** (maintainer-only) with the steps to tag + push +
  publish.
- The existing "Build / Install" header becomes "Build from source"
  and lives under Install.

## Edge cases

| Case | Behavior |
|---|---|
| `--init` with no `$HOME` | Print an error explaining the missing variable; exit 1. |
| `--init` with config dir read-only | Bubble up the create_dir error verbatim and exit 1. |
| `--init --force` overwrites a hand-edited file | Documented in the flag help and the overwrite error. The user opted in. |
| `--force` without `--init` | clap rejects with a usage error (the `requires = "init"` constraint). |
| `cargo binstall` on a target without a release | Falls back to `cargo install`. binstall handles this internally. |

## Tests

Unit (`src/config.rs`):
- `STARTER_TOML` parses cleanly through `Config::from_str` (smoke
  test: it produces the same effective config as `Config::default`).

Unit (`src/main.rs` helper):
- `run_init` writes the file when none exists (use a tempdir + custom
  HOME).
- `run_init` refuses to overwrite without `--force`.
- `run_init --force` overwrites.

E2E (`tests/e2e.sh`):
- `tmux-picker --init` writes the file and exits 0.
- `tmux-picker --init` against an existing file exits non-zero.
- `tmux-picker --init --force` overwrites.

## Validation

- [ ] Manual: `cargo install --path .` then `tmux-picker --version`.
- [ ] Manual: `tmux-picker --init` writes
      `~/.config/tmux-picker/config.toml`.
- [ ] Manual: `tmux-picker --init` again returns non-zero with a
      clear "already exists, pass --force to overwrite" error.
- [ ] Manual: `tmux-picker --check-config` after init shows the
      effective defaults (the starter file's values match defaults).
- [ ] Maintainer: `cargo publish --dry-run` succeeds (real publish is
      an out-of-band step; this design does not perform it).
