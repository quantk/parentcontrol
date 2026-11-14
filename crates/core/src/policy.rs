//! The policy snapshot: everything an agent needs to make decisions alone.
//!
//! The agent decides only from its last stored snapshot; syncing with the
//! server merely replaces it. Online and offline therefore share one path.

use std::collections::BTreeSet;
use std::time::Duration;

use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::calendar::{BudgetDay, Calendar, TimeOfDay};
use crate::ids::GroupId;
use crate::process::OsUser;
use crate::rules::Rule;
use crate::schedule::{DaySchedule, TimeWindow, WeekSchedule};
use crate::units::{MinuteDelta, Minutes};

/// A policy that cannot be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyError {
    /// A window would cross the start of the budget day.
    #[error("group {group}: window {}-{} crosses the day start {day_start}", .window.start(), .window.end())]
    WindowCrossesDayStart {
        /// The group owning the window.
        group: GroupId,
        /// The offending window.
        window: TimeWindow,
        /// The configured day start.
        day_start: TimeOfDay,
    },
    /// Two groups share an id.
    #[error("group {0} appears more than once")]
    DuplicateGroup(GroupId),
}

/// Time the parent added to or took from one budget day's quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adjustment {
    /// The day the adjustment applies to.
    pub day: BudgetDay,
    /// Minutes added (positive) or taken away (negative).
    pub minutes: MinuteDelta,
}

/// Time allowed during `day`: the quota of its weekday changed by the
/// parent's adjustments for that day, never below zero.
///
/// Adjustments change only the amount of time; they never open a window.
#[must_use]
pub fn allowance(schedule: &WeekSchedule, adjustments: &[Adjustment], day: BudgetDay) -> Minutes {
    let change: MinuteDelta = adjustments
        .iter()
        .filter(|adjustment| adjustment.day == day)
        .map(|adjustment| adjustment.minutes)
        .sum();
    change.apply(schedule.day(day.weekday()).quota())
}

/// Usage accumulated during one budget day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayUsage {
    /// The day the usage belongs to.
    pub day: BudgetDay,
    /// Total time used.
    #[serde(rename = "used_ms", with = "crate::usage::millis")]
    pub used: Duration,
}

/// Rules and limits of one group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPolicy {
    /// The group.
    pub id: GroupId,
    /// Name shown to the child.
    pub name: String,
    /// A process belongs to the group if any rule matches it.
    pub rules: Vec<Rule>,
    /// Quota and windows per day of week.
    pub schedule: WeekSchedule,
    /// Whether the parent has blocked the group until further notice.
    #[serde(default)]
    pub blocked: bool,
    /// The parent's changes to the quota, by day.
    #[serde(default)]
    pub adjustments: Vec<Adjustment>,
    /// Usage on other devices as known to the server at the last sync.
    ///
    /// This device's own usage always comes from its local journal, so the
    /// two never overlap.
    #[serde(default)]
    pub remote_usage: Option<DayUsage>,
}

impl GroupPolicy {
    /// Time allowed during `day` on all devices together; see [`allowance`].
    #[must_use]
    pub fn allowance(&self, day: BudgetDay) -> Minutes {
        allowance(&self.schedule, &self.adjustments, day)
    }

    /// Usage on other devices during `day`.
    #[must_use]
    pub fn remote_usage(&self, day: BudgetDay) -> Duration {
        self.remote_usage
            .filter(|usage| usage.day == day)
            .map_or(Duration::ZERO, |usage| usage.used)
    }
}

/// A validated policy snapshot for one device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPolicy", into = "RawPolicy")]
pub struct Policy {
    calendar: Calendar,
    accounts: BTreeSet<OsUser>,
    groups: Vec<GroupPolicy>,
}

#[derive(Serialize, Deserialize)]
struct RawPolicy {
    #[serde(with = "jiff::fmt::serde::tz::required")]
    time_zone: TimeZone,
    day_start: TimeOfDay,
    #[serde(default)]
    accounts: BTreeSet<OsUser>,
    #[serde(default)]
    groups: Vec<GroupPolicy>,
}

impl Policy {
    /// A policy limiting `accounts` on this device with `groups`.
    ///
    /// # Errors
    ///
    /// Fails when group ids repeat or a window crosses the day start.
    pub fn new(
        calendar: Calendar,
        accounts: BTreeSet<OsUser>,
        groups: Vec<GroupPolicy>,
    ) -> Result<Self, PolicyError> {
        let day_start = calendar.day_start();
        let mut seen = BTreeSet::new();
        for group in &groups {
            if !seen.insert(group.id) {
                return Err(PolicyError::DuplicateGroup(group.id));
            }
            let mut windows = group.schedule.days().flat_map(DaySchedule::windows);
            if let Some(&window) = windows.find(|window| window.span(day_start).is_none()) {
                return Err(PolicyError::WindowCrossesDayStart {
                    group: group.id,
                    window,
                    day_start,
                });
            }
        }
        Ok(Self {
            calendar,
            accounts,
            groups,
        })
    }

    /// How moments map to budget days.
    #[must_use]
    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// Whether processes of `user` are subject to the policy.
    ///
    /// With no bound accounts the device is not configured yet and nothing
    /// is limited.
    #[must_use]
    pub fn restricts(&self, user: &OsUser) -> bool {
        self.accounts.contains(user)
    }

    /// The accounts the policy limits.
    #[must_use]
    pub const fn accounts(&self) -> &BTreeSet<OsUser> {
        &self.accounts
    }

    /// The limited groups.
    #[must_use]
    pub fn groups(&self) -> &[GroupPolicy] {
        &self.groups
    }
}

impl TryFrom<RawPolicy> for Policy {
    type Error = PolicyError;

    fn try_from(raw: RawPolicy) -> Result<Self, Self::Error> {
        Self::new(
            Calendar::new(raw.time_zone, raw.day_start),
            raw.accounts,
            raw.groups,
        )
    }
}

impl From<Policy> for RawPolicy {
    fn from(policy: Policy) -> Self {
        Self {
            time_zone: policy.calendar.time_zone().clone(),
            day_start: policy.calendar.day_start(),
            accounts: policy.accounts,
            groups: policy.groups,
        }
    }
}

#[cfg(test)]
mod tests;
