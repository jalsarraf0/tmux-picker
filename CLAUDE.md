# tmux-picker

Rust TUI session picker for tmux. Runs on SSH login, renders to stderr, prints action to stdout.

## Build
cargo build --release

## Test
cargo test

## Lint
cargo fmt --check && cargo clippy -- -D warnings

## Install
cp target/release/tmux-picker ~/.local/bin/tmux-picker
