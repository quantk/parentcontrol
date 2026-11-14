//! `parentcontrol-agent`: the system service on a child's PC.
//!
//! It scans processes, accounts usage, closes programs that are not allowed,
//! keeps the local policy snapshot in sync with the server and feeds the tray.

mod actions;
mod cli;
mod clock;
mod limiter;
mod platform;
mod policy_file;
mod scan;
mod service;
mod store;
#[cfg(test)]
mod test_support;

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser as _;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::clock::SystemClock;
use crate::platform::Native;
use crate::store::Store;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Command::Run { data_dir } => run(data_dir.unwrap_or_else(platform::default_data_dir)),
    }
}

fn run(data_dir: PathBuf) -> anyhow::Result<()> {
    let store = Store::open(data_dir).context("opening the data directory")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    runtime
        .block_on(service::run(Native, SystemClock, store))
        .context("running the agent")
}
