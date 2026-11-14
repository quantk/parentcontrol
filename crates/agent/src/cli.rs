//! Command-line interface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::link::Endpoint;

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
    Run(DataDir),
    /// Join the server with a code from the parent's panel.
    Enroll {
        /// The server, such as `https://192.168.1.135:8443`.
        #[arg(long, value_parser = parse_endpoint)]
        server: Endpoint,
        /// The enrollment code shown in the panel.
        #[arg(long)]
        code: String,
        #[command(flatten)]
        data_dir: DataDir,
    },
}

/// Where the agent keeps its files.
#[derive(Debug, Args)]
pub(crate) struct DataDir {
    /// Directory with the policy snapshot, the usage journal and the
    /// server link; the platform default if omitted.
    #[arg(long = "data-dir", value_name = "DIR")]
    pub(crate) path: Option<PathBuf>,
}

fn parse_endpoint(url: &str) -> Result<Endpoint, String> {
    Endpoint::try_from(url.to_owned()).map_err(|error| error.to_string())
}
