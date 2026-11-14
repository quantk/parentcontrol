//! Accounting of usage on this device.
//!
//! Time is credited per scan tick from a monotonic clock, capped so that a
//! suspended or frozen machine does not turn into play time. Continuous use
//! of a group becomes one [`UsageRecord`]; records carry unique ids so the
//! server can merge them idempotently after an offline period.

use std::collections::BTreeMap;
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
}

/// This device's usage records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    records: Vec<UsageRecord>,
    /// Records still being extended, by group. Not persisted: after a
    /// restart new stretches start, which costs nothing but a record.
    #[serde(skip)]
    open: BTreeMap<GroupId, UsageRecordId>,
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
                .and_then(|id| self.records.iter_mut().find(|r| r.id == *id));
            match open {
                Some(record) if record.day == tick.day => record.duration += tick.credited,
                _ => {
                    let id = new_id();
                    self.records.push(UsageRecord {
                        id,
                        group,
                        day: tick.day,
                        start: tick.now.checked_sub(tick.credited).unwrap_or(tick.now),
                        duration: tick.credited,
                    });
                    self.open.insert(group, id);
                }
            }
        }
    }

    /// This device's total use of `group` during `day`.
    #[must_use]
    pub fn usage(&self, group: GroupId, day: BudgetDay) -> Duration {
        self.records
            .iter()
            .filter(|record| record.group == group && record.day == day)
            .map(|record| record.duration)
            .sum()
    }

    /// All records, oldest first.
    #[must_use]
    pub fn records(&self) -> &[UsageRecord] {
        &self.records
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
