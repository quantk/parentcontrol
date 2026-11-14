//! Windows implementation of the tray platform layer: a notification-area
//! icon whose balloons Windows shows as toast notifications, the agent's
//! named pipe, and `WM_CLOSE` for soft closing.

mod close;
mod icon;

use std::io;
use std::time::Duration;

use parentcontrol_core::tray::WINDOWS_PIPE;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

pub(crate) use self::close::close_windows;
use self::icon::Icon;
use crate::text::{APP_NAME, Message};

/// How long to wait for a free pipe instance before retrying later.
const PIPE_BUSY_WAIT: Duration = Duration::from_millis(200);

/// The notification-area icon, if it could be created.
#[derive(Debug)]
pub(crate) struct Ui {
    icon: Option<Icon>,
}

#[expect(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    reason = "the interface is shared with Linux, where these calls are asynchronous"
)]
impl Ui {
    /// Creates the icon on its own UI thread.
    pub(crate) async fn start() -> Self {
        Self {
            icon: Icon::start()
                .inspect_err(|error| tracing::warn!(%error, "no tray icon"))
                .ok(),
        }
    }

    /// Shows `lines` in the icon's tooltip.
    pub(crate) async fn show_status(&self, lines: &[String]) {
        if let Some(icon) = &self.icon {
            icon.set_tooltip(&format!("{APP_NAME}\n{}", lines.join("\n")));
        }
    }

    /// Shows a notification.
    pub(crate) async fn notify(&self, message: &Message) {
        if let Some(icon) = &self.icon {
            icon.notify(&message.summary, &message.body);
        }
    }
}

/// Connects to the agent, waiting briefly if every pipe instance is busy.
pub(crate) async fn connect() -> io::Result<NamedPipeClient> {
    match ClientOptions::new().open(WINDOWS_PIPE) {
        Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
            tokio::time::sleep(PIPE_BUSY_WAIT).await;
            ClientOptions::new().open(WINDOWS_PIPE)
        }
        result => result,
    }
}

/// `ERROR_PIPE_BUSY`: all server instances are taken for the moment.
const ERROR_PIPE_BUSY: i32 = 231;
