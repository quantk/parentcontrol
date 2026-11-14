//! Whether a device is connected, and what runs on it.

use std::time::Duration;

use jiff::Timestamp;

use super::format;
use crate::agents::Live;
use crate::db::devices::Device;

/// A device that has not reported for this long is shown as lost.
const STALE_AFTER: Duration = Duration::from_hours(1);

/// What the panel says about a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Presence {
    /// Only an enrollment code exists.
    Waiting,
    /// Connected, with programs of these groups running.
    Playing(Vec<String>),
    /// Connected, nothing limited is running.
    Online,
    /// Disconnected recently, as expected when the computer is off.
    Offline { seen: String },
    /// Silent for long: the computer may be off, or the agent cut off.
    Lost { seen: String },
}

impl Presence {
    /// The presence of `device` at `now`; `group_name` names groups.
    pub(super) fn of<'a>(
        device: &Device,
        live: Option<&Live>,
        now: Timestamp,
        group_name: impl Fn(parentcontrol_core::ids::GroupId) -> &'a str,
    ) -> Self {
        if !device.enrolled {
            return Self::Waiting;
        }
        if let Some(live) = live {
            return if live.active_groups.is_empty() {
                Self::Online
            } else {
                let mut groups: Vec<String> = live
                    .active_groups
                    .iter()
                    .map(|&group| group_name(group).to_owned())
                    .collect();
                groups.sort();
                Self::Playing(groups)
            };
        }
        let seen = device
            .last_seen
            .map_or_else(|| "никогда".to_owned(), |seen| format::ago(seen, now));
        let stale = device
            .last_seen
            .is_none_or(|seen| now.duration_since(seen).unsigned_abs() > STALE_AFTER);
        if stale {
            Self::Lost { seen }
        } else {
            Self::Offline { seen }
        }
    }

    /// A short state for a badge.
    pub(super) const fn label(&self) -> &'static str {
        match self {
            Self::Waiting => "Ждёт подключения",
            Self::Playing(_) => "Играет",
            Self::Online => "В сети",
            Self::Offline { .. } => "Не в сети",
            Self::Lost { .. } => "Нет связи",
        }
    }

    /// The badge class: its color, and a pulse while playing.
    pub(super) const fn tone(&self) -> &'static str {
        match self {
            Self::Waiting => "warn",
            Self::Playing(_) => "ok live",
            Self::Online => "ok",
            Self::Offline { .. } => "",
            Self::Lost { .. } => "bad",
        }
    }

    /// What runs, or when the device was last heard from.
    pub(super) fn detail(&self) -> String {
        match self {
            Self::Waiting => "код подключения выдан".to_owned(),
            Self::Playing(groups) => groups.join(", "),
            Self::Online => "ничего из групп не запущено".to_owned(),
            Self::Offline { seen } | Self::Lost { seen } => format!("связь: {seen}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use parentcontrol_core::ids::{DeviceId, GroupId};

    use super::*;

    fn device(enrolled: bool, seen_minutes_ago: Option<i64>, now: Timestamp) -> Device {
        Device {
            id: DeviceId::new(1),
            name: "ПК".to_owned(),
            platform: None,
            enrolled,
            last_seen: seen_minutes_ago.map(|minutes| {
                now.checked_sub(jiff::SignedDuration::from_mins(minutes))
                    .unwrap()
            }),
            local_users: Vec::new(),
            bindings: BTreeSet::new(),
        }
    }

    #[test]
    fn tells_states_apart() {
        let now: Timestamp = "2026-11-14T12:00:00Z".parse().unwrap();
        let name = |_| "Игры";
        let live = |groups: Vec<GroupId>| Live {
            connected_since: now,
            active_groups: groups,
        };
        assert_eq!(
            Presence::of(&device(false, None, now), None, now, name),
            Presence::Waiting
        );
        assert_eq!(
            Presence::of(
                &device(true, Some(0), now),
                Some(&live(vec![GroupId::new(1)])),
                now,
                name
            ),
            Presence::Playing(vec!["Игры".to_owned()])
        );
        assert_eq!(
            Presence::of(&device(true, Some(0), now), Some(&live(vec![])), now, name),
            Presence::Online
        );
        assert!(matches!(
            Presence::of(&device(true, Some(10), now), None, now, name),
            Presence::Offline { .. }
        ));
        assert!(matches!(
            Presence::of(&device(true, Some(120), now), None, now, name),
            Presence::Lost { .. }
        ));
        assert!(matches!(
            Presence::of(&device(true, None, now), None, now, name),
            Presence::Lost { .. }
        ));
    }
}
