//! Core library for the `tmux-picker` session picker.

/// User-visible actions emitted by the picker loop.
pub mod action;
/// Picker state, sorting, filtering, and input-independent operations.
pub mod app;
/// Command-line argument types.
pub mod cli;
pub mod clipboard;
/// Configuration parsing and theme/marker definitions.
pub mod config;
/// Keyboard and mouse event dispatch.
pub mod input;
/// Session metadata stored in tmux user options.
pub mod metadata;
/// Session data and validation helpers.
pub mod session;
/// tmux process integration and output parsers.
pub mod tmux;
/// Ratatui rendering helpers.
pub mod ui;
