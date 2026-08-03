use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "tmux-picker",
    version,
    about = "TUI session picker for tmux, on SSH login and local terminals",
    long_about = "Run with no arguments for the picker TUI. \
                  Subcommands manage per-session metadata (label/project/purpose) \
                  stored as tmux user-options."
)]
pub struct Cli {
    /// Print the effective config plus any parse warnings, then exit.
    /// Useful when a user-side override is not taking effect.
    #[arg(long, global = false)]
    pub check_config: bool,

    /// Write a starter `~/.config/tmux-picker/config.toml`. Refuses to
    /// overwrite an existing file unless `--force` is also passed.
    #[arg(long)]
    pub init: bool,

    /// Overwrite an existing config file when used with `--init`.
    #[arg(long, requires = "init")]
    pub force: bool,

    /// Print the effective `trigger_mode` ("always" or "ssh_only") and
    /// exit. Used internally by the shell hook to decide whether to run
    /// on a local (non-SSH) terminal; not usually invoked by hand.
    #[arg(long, hide = true)]
    pub print_trigger_mode: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Set or clear metadata for a session.
    Label {
        /// Session name.
        session: String,
        /// Human label, e.g., "Refactoring auth middleware".
        #[arg(long)]
        label: Option<String>,
        /// Project root path.
        #[arg(long)]
        project: Option<String>,
        /// Short purpose, e.g., "PR #234".
        #[arg(long)]
        purpose: Option<String>,
        /// Remove all tmux-picker metadata for this session.
        #[arg(long, conflicts_with_all = ["label", "project", "purpose"])]
        clear: bool,
    },
    /// Print metadata for a session as TOML.
    Show {
        /// Session name.
        session: String,
    },
    /// Auto-detect metadata from the session's active pane working directory.
    Auto {
        /// Session name.
        session: String,
    },
}
