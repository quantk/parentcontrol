//! Today's state of every group across all devices.

use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::budget::{self, GroupStatus};
use parentcontrol_core::calendar::{BudgetDay, Calendar};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::{Adjustment, DayUsage, GroupPolicy};
use rusqlite::Connection;

use crate::db::groups::Group;
use crate::db::usage::DayTotal;
use crate::db::{DEFAULT_PROFILE, DbError, groups, settings, usage};

/// One group today.
#[derive(Debug)]
pub(super) struct GroupToday {
    pub(super) group: Group,
    pub(super) status: GroupStatus,
    /// Used on all devices.
    pub(super) used: Duration,
    /// Quota changed by the parent's adjustments.
    pub(super) allowance: Duration,
}

/// Every group today.
#[derive(Debug)]
pub(super) struct Today {
    pub(super) calendar: Calendar,
    pub(super) day: BudgetDay,
    pub(super) groups: Vec<GroupToday>,
}

/// Evaluates every group at `now`, counting usage of all devices.
pub(super) fn today(connection: &Connection, now: Timestamp) -> Result<Today, DbError> {
    let calendar = settings::required_calendar(connection)?;
    let position = calendar.locate(now);
    let adjustments = groups::adjustments_since(connection, position.day)?;
    let totals = usage::totals(connection, position.day, position.day)?;

    let groups = groups::list(connection, DEFAULT_PROFILE)?
        .into_iter()
        .map(|group| {
            let used = used_by(&totals, &group);
            let policy = GroupPolicy {
                id: group.id,
                name: group.name.clone(),
                rules: Vec::new(),
                schedule: group.schedule.clone(),
                blocked: group.blocked_since.is_some(),
                adjustments: of_group(&adjustments, group.id),
                remote_usage: Some(DayUsage {
                    day: position.day,
                    used,
                }),
            };
            let status = budget::evaluate(&policy, calendar.day_start(), position, Duration::ZERO);
            let allowance = policy.allowance(position.day).to_duration();
            GroupToday {
                group,
                status,
                used,
                allowance,
            }
        })
        .collect();
    Ok(Today {
        day: position.day,
        calendar,
        groups,
    })
}

/// The adjustments of `group` among those of every group.
pub(super) fn of_group(all: &[(GroupId, Adjustment)], group: GroupId) -> Vec<Adjustment> {
    all.iter()
        .filter(|(id, _)| *id == group)
        .map(|(_, adjustment)| *adjustment)
        .collect()
}

fn used_by(totals: &[DayTotal], group: &Group) -> Duration {
    totals
        .iter()
        .filter(|total| total.group == group.id)
        .map(|total| total.used)
        .sum()
}
