//! `parentcontrol-agent`: the system service on a child's PC.
//!
//! It scans processes, accounts usage, closes programs that are not allowed,
//! keeps the local policy snapshot in sync with the server and feeds the tray.

mod actions;
mod cli;
mod clock;
mod enroll;
mod limiter;
mod link;
mod platform;
mod policy_file;
mod scan;
mod service;
mod store;
mod sync;
#[cfg(test)]
mod test_support;

use std::io::IsTerminal as _;

use anyhow::Context as _;
use clap::Parser as _;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command, DataDir};
use crate::platform::Native;
use crate::store::Store;
use crate::sync::tls::grouped;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;

    match Cli::parse().command {
        Command::Run(data_dir) => {
            let store = open_store(data_dir)?;
            runtime
                .block_on(service::run(Native, store))
                .context("running the agent")
        }
        Command::Enroll {
            server,
            code,
            data_dir,
        } => {
            let store = open_store(data_dir)?;
            let enrolled = runtime
                .block_on(enroll::enroll(&store, server, &code))
                .context("enrolling")?;
            println!("Устройство подключено (номер {}).", enrolled.device);
            println!(
                "Отпечаток сертификата сервера: {}",
                grouped(&enrolled.fingerprint)
            );
            println!("Сверьте его с отпечатком на странице «Устройства» в панели.");
            Ok(())
        }
    }
}

fn open_store(data_dir: DataDir) -> anyhow::Result<Store> {
    Store::open(data_dir.path.unwrap_or_else(platform::default_data_dir))
        .context("opening the data directory")
}
