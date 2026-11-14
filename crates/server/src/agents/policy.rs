//! Building the policy snapshot a device receives.

use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::DeviceId;
use parentcontrol_core::policy::{DayUsage, GroupPolicy, Policy};
use rusqlite::Connection;

use crate::db::{DEFAULT_PROFILE, DbError, devices, groups, settings, usage};

/// The policy of `device` at `now`, or `None` if the device was deleted.
///
/// Every account bound on the device is limited by the groups of the one
/// profile the interface manages. Adjustments of yesterday are included
/// too, so a device whose clock is slightly behind still applies them.
pub(crate) fn device_policy(
    connection: &Connection,
    device: DeviceId,
    now: Timestamp,
) -> Result<Option<Policy>, DbError> {
    if !devices::exists(connection, device)? {
        return Ok(None);
    }
    let calendar = settings::required_calendar(connection)?;
    let today = calendar.locate(now).day;
    let yesterday = BudgetDay::new(today.date().yesterday().unwrap_or(today.date()));
    let adjustments = groups::adjustments_since(connection, yesterday)?;
    let totals = usage::totals(connection, today, today)?;

    let groups = groups::list(connection, DEFAULT_PROFILE)?
        .into_iter()
        .map(|group| {
            let remote: Duration = totals
                .iter()
                .filter(|total| total.group == group.id && total.device != device)
                .map(|total| total.used)
                .sum();
            GroupPolicy {
                id: group.id,
                adjustments: adjustments
                    .iter()
                    .filter(|(id, _)| *id == group.id)
                    .map(|(_, adjustment)| *adjustment)
                    .collect(),
                name: group.name,
                rules: group.rules.into_iter().map(|stored| stored.rule).collect(),
                schedule: group.schedule,
                blocked: group.blocked_since.is_some(),
                remote_usage: Some(DayUsage {
                    day: today,
                    used: remote,
                }),
            }
        })
        .collect();

    let accounts = devices::bindings(connection, device)?;
    Policy::new(calendar, accounts, groups)
        .map(Some)
        .map_err(|error| DbError::Corrupt {
            column: "program_groups.schedule",
            reason: error.to_string(),
        })
}

#[cfg(test)]
mod tests;
