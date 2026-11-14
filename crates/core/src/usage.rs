//! Accounting of usage on this device.
//!
//! Time is credited per scan tick from a monotonic clock, capped so that a
//! suspended or frozen machine does not turn into play time. Continuous use
//! of a group becomes one [`UsageRecord`]; records carry unique ids so the
//! server can merge them idempotently after an offline period.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::calendar::BudgetDay;
use crate::ids::GroupId;

/// How much of `elapsed` since the previous tick counts as usage.
///
/// More than two scan intervals between ticks means the machine slept or
/// stalled; that time is not credited.
#[must_use]
pub fn credit(elapsed: Duration, interval: Duration) -> Duration {
    elapsed.min(interval.saturating_mul(2))
}

/// Globally unique id of a usage record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsageRecordId(Uuid);

impl UsageRecordId {
    /// Wraps a UUID; the agent generates random ones.
    #[must_use]
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for UsageRecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One continuous stretch of use of a group within one budget day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Unique id for idempotent merging.
    pub id: UsageRecordId,
    /// The group used.
    pub group: GroupId,
    /// The budget day the use is charged to.
    pub day: BudgetDay,
    /// When the stretch began.
    pub start: Timestamp,
    /// Credited time so far.
    #[serde(rename = "duration_ms", with = "millis")]
    pub duration: Duration,
    /// Whether any of it was counted while the agent had no connection to
    /// the server, so other devices could not see it in time.
    #[serde(default)]
    pub offline: bool,
}

/// What happened in one tick, as needed for accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// The current moment.
    pub now: Timestamp,
    /// The current budget day.
    pub day: BudgetDay,
    /// Time to credit, already capped with [`credit`].
    pub credited: Duration,
    /// Whether the agent is connected to the server.
    pub online: bool,
}

/// This device's usage records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    entries: Vec<Entry>,
    /// Records still being extended, by group. Not persisted: after a
    /// restart new stretches start, which costs nothing but a record.
    #[serde(skip)]
    open: BTreeMap<GroupId, UsageRecordId>,
}

/// A record and how much of it the server has confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    #[serde(flatten)]
    record: UsageRecord,
    #[serde(default, rename = "acknowledged_ms", with = "millis")]
    acknowledged: Duration,
}

impl Entry {
    fn is_synced(&self) -> bool {
        self.acknowledged >= self.record.duration
    }
}

impl Journal {
    /// Credits `tick` to every group in `active` and closes the stretches
    /// of groups no longer active.
    ///
    /// `new_id` is called once for every record started.
    pub fn record(
        &mut self,
        active: &[GroupId],
        tick: Tick,
        mut new_id: impl FnMut() -> UsageRecordId,
    ) {
        self.open.retain(|group, _| active.contains(group));
        if tick.credited.is_zero() {
            return;
        }
        for &group in active {
            let open = self
                .open
                .get(&group)
                .and_then(|id| self.entries.iter_mut().find(|entry| entry.record.id == *id));
            match open {
                Some(entry) if entry.record.day == tick.day => {
                    entry.record.duration += tick.credited;
                    entry.record.offline |= !tick.online;
                }
                _ => {
                    let id = new_id();
                    let record = UsageRecord {
                        id,
                        group,
                        day: tick.day,
                        start: tick.now.checked_sub(tick.credited).unwrap_or(tick.now),
                        duration: tick.credited,
                        offline: !tick.online,
                    };
                    self.entries.push(Entry {
                        record,
                        acknowledged: Duration::ZERO,
                    });
                    self.open.insert(group, id);
                }
            }
        }
    }

    /// This device's total use of `group` during `day`.
    #[must_use]
    pub fn usage(&self, group: GroupId, day: BudgetDay) -> Duration {
        self.records()
            .filter(|record| record.group == group && record.day == day)
            .map(|record| record.duration)
            .sum()
    }

    /// All records, oldest first.
    pub fn records(&self) -> impl Iterator<Item = &UsageRecord> {
        self.entries.iter().map(|entry| &entry.record)
    }

    /// Records the server has not confirmed in full, in their current state.
    pub fn unsynced(&self) -> impl Iterator<Item = &UsageRecord> {
        self.entries
            .iter()
            .filter(|entry| !entry.is_synced())
            .map(|entry| &entry.record)
    }

    /// Notes that the server has stored record `id` with `duration`.
    ///
    /// Confirmations never shrink: an older, shorter confirmation arriving
    /// late does not undo a newer one.
    pub fn acknowledge(&mut self, id: UsageRecordId, duration: Duration) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.record.id == id) {
            entry.acknowledged = entry.acknowledged.max(duration);
        }
    }

    /// Drops fully confirmed records of days before `day`. They are kept by
    /// the server and no longer affect any decision here.
    pub fn prune_before(&mut self, day: BudgetDay) {
        self.entries
            .retain(|entry| entry.record.day >= day || !entry.is_synced());
    }
}

/// Serde helper storing a [`Duration`] as whole milliseconds.
pub(crate) mod millis {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(
        duration: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        // Saturates after 584 million years.
        serializer.serialize_u64(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer).map(Duration::from_millis)
    }
}

#[cfg(test)]
mod tests;
