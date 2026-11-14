//! Linux implementation of the tray platform layer: a `StatusNotifierItem`
//! icon, freedesktop notifications, and the agent's Unix socket.
//!
//! Desktops without a tray host (GNOME without an extension) still show
//! notifications; the icon is simply absent.

use std::collections::HashMap;
use std::path::PathBuf;
use std::{fmt, io};

use ksni::menu::StandardItem;
use ksni::{MenuItem, ToolTip, TrayMethods as _};
use parentcontrol_core::tray::{SOCKET_ENV, UNIX_SOCKET};
use tokio::net::UnixStream;
use tracing::warn;
use zbus::zvariant::Value;

use crate::text::{APP_NAME, Message};

/// Icon from the freedesktop icon theme.
const ICON: &str = "appointment-soon";

/// The tray icon and the notification service, whichever are available.
pub(crate) struct Ui {
    icon: Option<ksni::Handle<StatusIcon>>,
    notifications: Option<zbus::Connection>,
}

impl fmt::Debug for Ui {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ui")
            .field("icon", &self.icon.is_some())
            .field("notifications", &self.notifications.is_some())
            .finish()
    }
}

impl Ui {
    /// Registers the icon and connects to the notification service.
    pub(crate) async fn start() -> Self {
        let icon = StatusIcon {
            lines: vec!["Подключение к службе контроля…".to_owned()],
        }
        .spawn()
        .await
        .inspect_err(|error| warn!(%error, "no tray host; only notifications will be shown"))
        .ok();
        let notifications = zbus::Connection::session()
            .await
            .inspect_err(|error| warn!(%error, "no session bus; notifications are off"))
            .ok();
        Self {
            icon,
            notifications,
        }
    }

    /// Shows `lines` in the icon's tooltip and menu.
    pub(crate) async fn show_status(&self, lines: &[String]) {
        if let Some(icon) = &self.icon {
            let lines = lines.to_vec();
            icon.update(|icon| icon.lines = lines).await;
        }
    }

    /// Shows a desktop notification.
    pub(crate) async fn notify(&self, message: &Message) {
        let Some(connection) = &self.notifications else {
            return;
        };
        let arguments = (
            APP_NAME,
            0_u32,
            ICON,
            message.summary.as_str(),
            message.body.as_str(),
            Vec::<&str>::new(),
            HashMap::<&str, Value<'_>>::new(),
            -1_i32,
        );
        let sent = connection
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &arguments,
            )
            .await;
        if let Err(error) = sent {
            warn!(%error, "cannot show a notification");
        }
    }
}

/// Connects to the agent.
pub(crate) async fn connect() -> io::Result<UnixStream> {
    let path =
        std::env::var_os(SOCKET_ENV).map_or_else(|| PathBuf::from(UNIX_SOCKET), PathBuf::from);
    UnixStream::connect(path).await
}

/// Nothing to do: on Linux the agent closes programs softly itself.
pub(crate) fn close_windows(_: &[u32]) {}

#[derive(Debug)]
struct StatusIcon {
    lines: Vec<String>,
}

impl ksni::Tray for StatusIcon {
    fn id(&self) -> String {
        "parentcontrol".to_owned()
    }

    fn title(&self) -> String {
        APP_NAME.to_owned()
    }

    fn icon_name(&self) -> String {
        ICON.to_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: APP_NAME.to_owned(),
            description: self.lines.join("\n"),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.lines
            .iter()
            .map(|line| {
                StandardItem {
                    // A single underscore would mark an access key.
                    label: line.replace('_', "__"),
                    enabled: false,
                    ..StandardItem::default()
                }
                .into()
            })
            .collect()
    }
}
