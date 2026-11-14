//! Messages between the agent and the tray in the user's session.
//!
//! They travel as one JSON object per line. The agent tells; the tray only
//! shows and, on Windows, closes windows when asked. The tray's only reply
//! is a delivery confirmation, which the agent merely logs: nothing the
//! tray sends can change a decision.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::budget::{DenyReason, StopReason};
use crate::calendar::TimeOfDay;
use crate::units::Minutes;

/// Where the agent listens for trays on Unix.
pub const UNIX_SOCKET: &str = "/run/parentcontrol/tray.sock";

/// The named pipe the agent listens on for trays on Windows.
pub const WINDOWS_PIPE: &str = r"\\.\pipe\parentcontrol-tray";

/// Environment variable overriding [`UNIX_SOCKET`], for development
/// without root.
pub const SOCKET_ENV: &str = "PARENTCONTROL_TRAY_SOCKET";

/// The state of one group as the tray shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayGroup {
    /// The group's name.
    pub name: String,
    /// Quota left today.
    #[serde(rename = "remaining_ms", with = "crate::usage::millis")]
    pub remaining: Duration,
    /// Whether it may be used now.
    pub state: TrayState,
}

/// Whether a group may be used now, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrayState {
    /// Allowed for `stop_in` more.
    Allowed {
        /// Time until the permission ends.
        #[serde(rename = "stop_in_ms", with = "crate::usage::millis")]
        stop_in: Duration,
        /// What ends it.
        reason: StopReason,
    },
    /// Not allowed now.
    Denied {
        /// Why.
        reason: DenyReason,
        /// When the next window opens today, if any.
        next_window: Option<TimeOfDay>,
    },
}

/// What the tray shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayStatus {
    /// Whether this session's account is limited at all; if not, the tray
    /// shows that nothing is limited.
    pub restricted: bool,
    /// Every limited group.
    pub groups: Vec<TrayGroup>,
}

/// A notification for the child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrayNotice {
    /// The group stops in `left`.
    Warning {
        /// The group's name.
        group: String,
        /// Time left, rounded up.
        left: Minutes,
        /// What will stop it.
        reason: StopReason,
    },
    /// The group's programs are being closed.
    Closing {
        /// The group's name.
        group: String,
        /// Why.
        reason: DenyReason,
    },
    /// A program was closed because it was started when not allowed.
    LaunchDenied {
        /// The group's name.
        group: String,
        /// Why.
        reason: DenyReason,
    },
    /// The group became available again before its programs were forced
    /// to close.
    CloseCancelled {
        /// The group's name.
        group: String,
    },
}

/// A message from the agent to a tray.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToTray {
    /// The current state; sent on connection and whenever it changes.
    Status(TrayStatus),
    /// Show a notification; confirm with [`TrayToAgent::Delivered`].
    Notice {
        /// Identifies the notice in the confirmation.
        id: u64,
        /// What to show.
        notice: TrayNotice,
    },
    /// Ask the windows of these processes to close, so the programs can
    /// save first. Only Windows needs this: a service cannot reach the
    /// windows of a user session.
    CloseWindows {
        /// Process ids.
        processes: Vec<u32>,
    },
}

/// A message from a tray to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrayToAgent {
    /// The notice with this id was shown.
    Delivered {
        /// The notice's id.
        id: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips() {
        let message = AgentToTray::Status(TrayStatus {
            restricted: true,
            groups: vec![TrayGroup {
                name: "Игры".to_owned(),
                remaining: Duration::from_mins(25),
                state: TrayState::Denied {
                    reason: DenyReason::OutsideWindow,
                    next_window: Some("16:00".parse().unwrap()),
                },
            }],
        });
        let json = serde_json::to_string(&message).unwrap();
        assert!(!json.contains('\n'), "one message per line");
        assert_eq!(serde_json::from_str::<AgentToTray>(&json).unwrap(), message);
    }

    #[test]
    fn notice_is_tagged() {
        let message = AgentToTray::Notice {
            id: 7,
            notice: TrayNotice::CloseCancelled {
                group: "Игры".to_owned(),
            },
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(
            json,
            r#"{"type":"notice","id":7,"notice":{"kind":"close_cancelled","group":"Игры"}}"#
        );
    }
}
