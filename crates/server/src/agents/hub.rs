//! Who is connected, what runs where, and change notifications.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use jiff::Timestamp;
use parentcontrol_core::ids::{DeviceId, GroupId};
use tokio::sync::watch;

/// One connection of a device, to tell an old connection from its
/// replacement when both briefly coexist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConnectionId(u64);

/// What a connected device reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    pub(crate) connected_since: Timestamp,
    /// Groups with a running program, as of the last report.
    pub(crate) active_groups: Vec<GroupId>,
}

#[derive(Debug)]
struct Entry {
    connection: ConnectionId,
    live: Live,
}

/// Registry of connected agents.
#[derive(Debug)]
pub(crate) struct Hub {
    /// Bumped whenever anything that may affect a device policy changes;
    /// every session then rebuilds and pushes its policy.
    generation: watch::Sender<u64>,
    connections: Mutex<(u64, HashMap<DeviceId, Entry>)>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            generation: watch::Sender::new(0),
            connections: Mutex::new((0, HashMap::new())),
        }
    }
}

impl Hub {
    /// Tells every session to resend its policy.
    pub(crate) fn policy_changed(&self) {
        self.generation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    /// Notifications of policy changes.
    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.generation.subscribe()
    }

    /// Registers a new connection of `device`, replacing any older one.
    pub(crate) fn connected(&self, device: DeviceId, now: Timestamp) -> ConnectionId {
        let mut guard = self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (counter, connections) = &mut *guard;
        *counter += 1;
        let connection = ConnectionId(*counter);
        let live = Live {
            connected_since: now,
            active_groups: Vec::new(),
        };
        connections.insert(device, Entry { connection, live });
        connection
    }

    /// Unregisters `connection`, unless a newer one has replaced it.
    pub(crate) fn disconnected(&self, device: DeviceId, connection: ConnectionId) {
        let mut guard = self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if guard
            .1
            .get(&device)
            .is_some_and(|entry| entry.connection == connection)
        {
            guard.1.remove(&device);
        }
    }

    /// Records the groups running on `device`; `true` if they changed.
    pub(crate) fn set_active(
        &self,
        device: DeviceId,
        connection: ConnectionId,
        mut groups: Vec<GroupId>,
    ) -> bool {
        groups.sort_unstable();
        groups.dedup();
        let mut guard = self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match guard.1.get_mut(&device) {
            Some(entry) if entry.connection == connection && entry.live.active_groups != groups => {
                entry.live.active_groups = groups;
                true
            }
            _ => false,
        }
    }

    /// Every connected device with what it reports.
    pub(crate) fn live(&self) -> HashMap<DeviceId, Live> {
        let guard = self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard
            .1
            .iter()
            .map(|(device, entry)| (*device, entry.live.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PC: DeviceId = DeviceId::new(1);

    fn now() -> Timestamp {
        "2026-11-14T10:00:00Z".parse().unwrap()
    }

    #[test]
    fn replaced_connection_does_not_unregister_its_successor() {
        let hub = Hub::default();
        let old = hub.connected(PC, now());
        let new = hub.connected(PC, now());
        hub.disconnected(PC, old);
        assert!(hub.live().contains_key(&PC));
        hub.disconnected(PC, new);
        assert!(hub.live().is_empty());
    }

    #[test]
    fn active_groups_report_changes_only() {
        let hub = Hub::default();
        let connection = hub.connected(PC, now());
        assert!(hub.set_active(PC, connection, vec![GroupId::new(2), GroupId::new(1)]));
        assert!(!hub.set_active(PC, connection, vec![GroupId::new(1), GroupId::new(2)]));
        assert_eq!(
            hub.live()[&PC].active_groups,
            [GroupId::new(1), GroupId::new(2)]
        );
    }

    #[tokio::test]
    async fn subscribers_hear_about_changes() {
        let hub = Hub::default();
        let mut changes = hub.subscribe();
        hub.policy_changed();
        assert!(changes.changed().await.is_ok());
    }
}
