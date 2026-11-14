//! `parentcontrol-server`: HTTP API for agents, the parent's web panel and
//! the SQLite storage. Runs on the home server inside Docker Compose.

use std::io::{self, BufRead as _, IsTerminal as _};
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context as _, bail};
use clap::{Parser, Subcommand};
use parentcontrol_server::{Config, Server, check_health, set_password};
use tracing_subscriber::EnvFilter;

/// Parent control server: agent API and parent panel.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the agent API and the panel.
    Serve(Config),
    /// Set the panel password; reads it from the terminal, or from the
    /// first line of standard input when that is not a terminal.
    SetPassword {
        /// Directory with the database.
        #[arg(
            long,
            env = "PARENTCONTROL_DATA_DIR",
            default_value = "/var/lib/parentcontrol-server"
        )]
        data_dir: PathBuf,
    },
    /// Exit successfully if the local server answers; for container health checks.
    Healthcheck {
        /// The address the server listens on; only its port is used.
        #[arg(long, env = "PARENTCONTROL_LISTEN", default_value = "0.0.0.0:8443")]
        listen: SocketAddr,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let runtime = tokio::runtime::Runtime::new().context("starting the async runtime")?;
    match Cli::parse().command {
        Command::Serve(config) => runtime.block_on(async {
            let server = Server::start(config).await?;
            server.run_until(stop_signal()).await?;
            Ok(())
        }),
        Command::SetPassword { data_dir } => {
            let password = read_password()?;
            runtime.block_on(set_password(&data_dir, &password))?;
            eprintln!("Пароль задан.");
            Ok(())
        }
        Command::Healthcheck { listen } => Ok(runtime.block_on(check_health(listen))?),
    }
}

fn read_password() -> anyhow::Result<String> {
    if !io::stdin().is_terminal() {
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading the password")?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
    }
    let password = rpassword::prompt_password("Новый пароль: ").context("reading the password")?;
    let repeat =
        rpassword::prompt_password("Повторите пароль: ").context("reading the password")?;
    if password != repeat {
        bail!("пароли не совпадают");
    }
    Ok(password)
}

async fn stop_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
        }
        Err(_) => drop(tokio::signal::ctrl_c().await),
    }
}
