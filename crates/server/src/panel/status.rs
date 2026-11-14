//! Today's state of every group across all devices.

use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::budget::{self, GroupStatus};
use parentcontrol_core::calendar::{BudgetDay, Calendar};
use parentcontrol_core::policy::{Bonus, DayUsage, GroupPolicy};
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
    /// Quota plus bonuses.
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
    let bonuses = groups::bonuses_since(connection, position.day)?;
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
                bonuses: bonuses
                    .iter()
                    .filter(|(id, _)| *id == group.id)
                    .map(|(_, bonus)| *bonus)
                    .collect(),
                remote_usage: Some(DayUsage {
                    day: position.day,
                    used,
                }),
            };
            let status = budget::evaluate(&policy, calendar.day_start(), position, Duration::ZERO);
            let allowance = allowance(&group, &policy.bonuses, position.day);
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

/// Quota of `day` by the current schedule plus the bonuses granted for it.
pub(super) fn allowance(group: &Group, bonuses: &[Bonus], day: BudgetDay) -> Duration {
    let bonus: Duration = bonuses
        .iter()
        .filter(|bonus| bonus.day == day)
        .map(|bonus| bonus.minutes.to_duration())
        .sum();
    group.schedule.day(day.weekday()).quota().to_duration() + bonus
}

fn used_by(totals: &[DayTotal], group: &Group) -> Duration {
    totals
        .iter()
        .filter(|total| total.group == group.id)
        .map(|total| total.used)
        .sum()
}
