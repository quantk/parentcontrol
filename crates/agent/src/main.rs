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

use std::fs::OpenOptions;
use std::io::{self, IsTerminal as _};
use std::sync::Mutex;

use anyhow::Context as _;
use clap::Parser as _;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command, DataDir};
use crate::platform::Native;
use crate::store::Store;
use crate::sync::tls::grouped;

/// The log of a Windows service, which has no console, in its data directory.
const SERVICE_LOG: &str = "agent.log";

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Run(data_dir) => {
            log_to_stderr();
            let store = open_store(data_dir)?;
            runtime()?
                .block_on(service::run(Native, store))
                .context("running the agent")
        }
        Command::Service => {
            let store = open_store(DataDir { path: None })?;
            log_to_file(&store)?;
            let runtime = runtime()?;
            let body = move || {
                runtime
                    .block_on(service::run(Native, store))
                    .map_err(Into::into)
            };
            platform::run_service(Box::new(body)).context("running the service")
        }
        Command::Install => {
            log_to_stderr();
            platform::install().context("installing")?;
            println!(
                "Служба установлена и запущена. Теперь подключите устройство командой enroll."
            );
            Ok(())
        }
        Command::Uninstall => {
            log_to_stderr();
            platform::uninstall().context("uninstalling")?;
            println!("Служба удалена. Данные и журнал использования сохранены.");
            Ok(())
        }
        Command::Enroll {
            server,
            code,
            data_dir,
        } => {
            log_to_stderr();
            let store = open_store(data_dir)?;
            let enrolled = runtime()?
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

fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

fn log_to_stderr() {
    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
}

fn log_to_file(store: &Store) -> anyhow::Result<()> {
    let path = store.file(SERVICE_LOG);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(())
}

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")
}

fn open_store(data_dir: DataDir) -> anyhow::Result<Store> {
    Store::open(data_dir.path.unwrap_or_else(platform::default_data_dir))
        .context("opening the data directory")
}
