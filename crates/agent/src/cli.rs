//! Command-line interface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Parent control agent: limits selected programs on this computer.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// What to do.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Run the service loop; normally started by the service manager.
    Run {
        /// Directory with the policy snapshot and the usage journal.
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
    },
}
