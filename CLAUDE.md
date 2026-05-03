# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints
action to stdout. Plus CLI subcommands for per-session metadata.

## Build
cargo build --release

## Test
cargo test
bash tests/e2e.sh

## Lint
cargo fmt --check && cargo clippy -- -D warnings

## Install
cp target/release/tmux-picker ~/.local/bin/tmux-picker
# or: bash scripts/install.sh

## CLI
tmux-picker                     # TUI picker
tmux-picker label <s> [flags]   # set/clear metadata
tmux-picker show  <s>           # dump metadata as TOML
tmux-picker auto  <s>           # auto-detect from pane cwd
