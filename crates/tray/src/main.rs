//! `parentcontrol-tray`: runs in the child's desktop session.
//!
//! Shows the remaining time and notifications, and on Windows performs the
//! soft close of windows on the agent's request. It decides nothing: killing
//! it does not affect any limit.

// No console window on Windows; ignored elsewhere.
#![windows_subsystem = "windows"]

mod platform;
mod text;

use std::io::{self, IsTerminal as _};
use std::time::Duration;

use anyhow::Context as _;
use parentcontrol_core::tray::{AgentToTray, TrayToAgent};
use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use crate::platform::Ui;

/// Delay between attempts to reach the agent.
const RECONNECT: Duration = Duration::from_secs(5);

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    runtime.block_on(run());
    Ok(())
}

/// Shows what the agent says, reconnecting whenever it goes away.
async fn run() {
    let ui = Ui::start().await;
    loop {
        match platform::connect().await {
            Ok(stream) => {
                info!("connected to the agent");
                if let Err(error) = session(stream, &ui).await {
                    debug!(%error, "connection to the agent lost");
                }
                ui.show_status(&["Нет связи со службой контроля".to_owned()])
                    .await;
            }
            Err(error) => debug!(%error, "the agent is not reachable"),
        }
        tokio::time::sleep(RECONNECT).await;
    }
}

async fn session(stream: impl AsyncRead + AsyncWrite, ui: &Ui) -> io::Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(message) = serde_json::from_str::<AgentToTray>(&line) else {
            debug!(line, "unreadable message from the agent");
            continue;
        };
        match message {
            AgentToTray::Status(status) => ui.show_status(&text::status_lines(&status)).await,
            AgentToTray::Notice { id, notice } => {
                ui.notify(&text::notice_message(&notice)).await;
                let mut reply =
                    serde_json::to_vec(&TrayToAgent::Delivered { id }).map_err(io::Error::other)?;
                reply.push(b'\n');
                writer.write_all(&reply).await?;
            }
            AgentToTray::CloseWindows { processes } => platform::close_windows(&processes),
        }
    }
    Ok(())
}
