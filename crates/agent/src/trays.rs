//! Feeding the trays in users' sessions.
//!
//! Every connected tray gets the current status and the notices meant for
//! its account. Nothing a tray sends affects the agent beyond a log line.

use std::collections::BTreeSet;

use parentcontrol_core::budget::{Availability, GroupStatus};
use parentcontrol_core::enforcement::Notice;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::process::{OsUser, ProcessId};
use parentcontrol_core::tray::{
    AgentToTray, TrayGroup, TrayNotice, TrayState, TrayStatus, TrayToAgent,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::sync::{broadcast, mpsc, watch};
use tracing::{debug, info, warn};

use crate::actions::group_name;
use crate::platform::{self, PlatformError, TrayClient};

/// Messages queued per tray before a slow one starts missing notices.
const BACKLOG: usize = 32;

/// The status shared by all trays and who it concerns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Shared {
    groups: Vec<TrayGroup>,
    restricted: BTreeSet<OsUser>,
}

/// A message for the trays of some accounts.
#[derive(Debug, Clone)]
struct Targeted {
    users: BTreeSet<OsUser>,
    message: AgentToTray,
}

/// Connected trays.
#[derive(Debug)]
pub(crate) struct Trays {
    shared: watch::Sender<Shared>,
    targeted: broadcast::Sender<Targeted>,
    next_notice: u64,
}

impl Trays {
    /// Starts accepting trays.
    pub(crate) fn start() -> Result<Self, PlatformError> {
        let (clients, mut connected) = mpsc::channel(4);
        platform::listen_for_trays(clients)?;
        let shared = watch::Sender::new(Shared::default());
        let (targeted, _) = broadcast::channel(BACKLOG);
        let (status, events) = (shared.clone(), targeted.clone());
        tokio::spawn(async move {
            while let Some(client) = connected.recv().await {
                tokio::spawn(serve(client, status.subscribe(), events.subscribe()));
            }
        });
        Ok(Self {
            shared,
            targeted,
            next_notice: 0,
        })
    }

    /// Publishes the statuses of the last step.
    pub(crate) fn update(&self, policy: &Policy, statuses: &[GroupStatus]) {
        let shared = Shared {
            groups: statuses
                .iter()
                .map(|status| tray_group(policy, status))
                .collect(),
            restricted: policy.accounts().clone(),
        };
        self.shared.send_if_modified(|current| {
            let changed = *current != shared;
            *current = shared;
            changed
        });
    }

    /// Shows `notice` to the limited accounts.
    pub(crate) fn notify(&mut self, policy: &Policy, notice: Notice) {
        self.next_notice += 1;
        let message = AgentToTray::Notice {
            id: self.next_notice,
            notice: tray_notice(policy, notice),
        };
        // No receivers just means no tray is running.
        let _ = self.targeted.send(Targeted {
            users: policy.accounts().clone(),
            message,
        });
    }

    /// Asks the tray of `owner` to close the windows of `process`.
    pub(crate) fn close_windows(&self, owner: OsUser, process: ProcessId) {
        let message = AgentToTray::CloseWindows {
            processes: vec![process.pid()],
        };
        let _ = self.targeted.send(Targeted {
            users: BTreeSet::from([owner]),
            message,
        });
    }
}

async fn serve(
    client: TrayClient,
    mut shared: watch::Receiver<Shared>,
    mut targeted: broadcast::Receiver<Targeted>,
) {
    let TrayClient { user, stream } = client;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    let mine = |users: &BTreeSet<OsUser>| user.as_ref().is_some_and(|user| users.contains(user));
    info!(user = ?user, "tray connected");

    let initial = status_for(&shared.borrow_and_update(), mine);
    if send(&mut writer, &initial).await.is_err() {
        return;
    }
    loop {
        let message = tokio::select! {
            changed = shared.changed() => match changed {
                Ok(()) => status_for(&shared.borrow_and_update(), mine),
                Err(_) => return,
            },
            event = targeted.recv() => match event {
                Ok(event) if mine(&event.users) => event.message,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "a tray is too slow; notices were dropped");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            line = lines.next_line() => {
                let Ok(Some(line)) = line else {
                    info!(user = ?user, "tray disconnected");
                    return;
                };
                if let Ok(TrayToAgent::Delivered { id }) = serde_json::from_str(&line) {
                    debug!(id, "tray showed a notice");
                }
                continue;
            }
        };
        if send(&mut writer, &message).await.is_err() {
            return;
        }
    }
}

fn status_for(shared: &Shared, mine: impl Fn(&BTreeSet<OsUser>) -> bool) -> AgentToTray {
    let restricted = mine(&shared.restricted);
    AgentToTray::Status(TrayStatus {
        restricted,
        groups: if restricted {
            shared.groups.clone()
        } else {
            Vec::new()
        },
    })
}

async fn send(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    message: &AgentToTray,
) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    line.push(b'\n');
    writer.write_all(&line).await
}

fn tray_group(policy: &Policy, status: &GroupStatus) -> TrayGroup {
    TrayGroup {
        name: group_name(policy, status.group).to_owned(),
        remaining: status.remaining,
        state: match status.availability {
            Availability::Allowed { stop_in, reason } => TrayState::Allowed { stop_in, reason },
            Availability::Denied(reason) => TrayState::Denied {
                reason,
                next_window: status.next_window,
            },
        },
    }
}

fn tray_notice(policy: &Policy, notice: Notice) -> TrayNotice {
    let name = |group| group_name(policy, group).to_owned();
    match notice {
        Notice::Warning {
            group,
            left,
            reason,
        } => TrayNotice::Warning {
            group: name(group),
            left,
            reason,
        },
        Notice::Closing { group, reason } => TrayNotice::Closing {
            group: name(group),
            reason,
        },
        Notice::LaunchDenied { group, reason } => TrayNotice::LaunchDenied {
            group: name(group),
            reason,
        },
        Notice::CloseCancelled { group } => TrayNotice::CloseCancelled { group: name(group) },
    }
}
