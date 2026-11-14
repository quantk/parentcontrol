//! Program groups with their rules, schedules, blocks and adjustments.

use jiff::Timestamp;
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::{GroupId, ProfileId};
use parentcontrol_core::policy::Adjustment;
use parentcontrol_core::rules::Rule;
use parentcontrol_core::schedule::{DaySchedule, WeekSchedule};
use parentcontrol_core::units::{MinuteDelta, Minutes};
use rusqlite::{Connection, OptionalExtension, params};

use super::DbError;
use super::convert::{day, json, optional_timestamp, text, to_json};

/// Quota of a newly created group on every day.
const DEFAULT_QUOTA: Minutes = Minutes::new(60);

/// A stored rule with its row id, so the panel can delete it.
#[derive(Debug, Clone)]
pub(crate) struct StoredRule {
    pub(crate) id: i64,
    pub(crate) rule: Rule,
}

/// A group as stored.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub(crate) id: GroupId,
    pub(crate) name: String,
    pub(crate) schedule: WeekSchedule,
    pub(crate) blocked_since: Option<Timestamp>,
    pub(crate) rules: Vec<StoredRule>,
}

/// All groups of `profile`, ordered by name.
pub(crate) fn list(connection: &Connection, profile: ProfileId) -> Result<Vec<Group>, DbError> {
    let mut statement = connection.prepare(
        "SELECT id, name, schedule, blocked_since FROM program_groups
         WHERE profile_id = ?1 ORDER BY name, id",
    )?;
    let rows = statement
        .query_map([profile.get()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut rules =
        connection.prepare("SELECT id, rule FROM rules WHERE group_id = ?1 ORDER BY id")?;
    rows.into_iter()
        .map(|(id, name, schedule, blocked_since)| {
            let stored = rules
                .query_map([id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .map(|row| {
                    let (id, rule) = row?;
                    Ok(StoredRule {
                        id,
                        rule: json(&rule, "rules.rule")?,
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            Ok(Group {
                id: GroupId::new(id),
                name,
                schedule: json(&schedule, "program_groups.schedule")?,
                blocked_since: optional_timestamp(
                    blocked_since.as_deref(),
                    "program_groups.blocked_since",
                )?,
                rules: stored,
            })
        })
        .collect()
}

/// The name of a group, if it exists.
pub(crate) fn name(connection: &Connection, group: GroupId) -> Result<Option<String>, DbError> {
    let name = connection
        .query_row(
            "SELECT name FROM program_groups WHERE id = ?1",
            [group.get()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(name)
}

/// Creates a group with the default schedule.
pub(crate) fn create(
    connection: &Connection,
    profile: ProfileId,
    name: &str,
) -> Result<GroupId, DbError> {
    // A quota within a day is always valid.
    let day = DaySchedule::new(DEFAULT_QUOTA, Vec::new())
        .map_err(|error| super::convert::corrupt("schedule", error))?;
    let schedule = to_json(&WeekSchedule::every_day(&day), "program_groups.schedule")?;
    connection.execute(
        "INSERT INTO program_groups (profile_id, name, schedule) VALUES (?1, ?2, ?3)",
        params![profile.get(), name, schedule],
    )?;
    Ok(GroupId::new(connection.last_insert_rowid()))
}

/// Renames a group; `false` if it does not exist.
pub(crate) fn rename(connection: &Connection, group: GroupId, name: &str) -> Result<bool, DbError> {
    Ok(connection.execute(
        "UPDATE program_groups SET name = ?2 WHERE id = ?1",
        params![group.get(), name],
    )? > 0)
}

/// Deletes a group with its rules and adjustments; usage history stays.
pub(crate) fn delete(connection: &Connection, group: GroupId) -> Result<bool, DbError> {
    Ok(connection.execute("DELETE FROM program_groups WHERE id = ?1", [group.get()])? > 0)
}

/// Replaces the weekly schedule.
pub(crate) fn set_schedule(
    connection: &Connection,
    group: GroupId,
    schedule: &WeekSchedule,
) -> Result<bool, DbError> {
    let schedule = to_json(schedule, "program_groups.schedule")?;
    Ok(connection.execute(
        "UPDATE program_groups SET schedule = ?2 WHERE id = ?1",
        params![group.get(), schedule],
    )? > 0)
}

/// Blocks the group since `since`, or lifts the block with `None`.
pub(crate) fn set_blocked(
    connection: &Connection,
    group: GroupId,
    since: Option<Timestamp>,
) -> Result<bool, DbError> {
    let since = since.map(text);
    Ok(connection.execute(
        "UPDATE program_groups SET blocked_since = ?2 WHERE id = ?1",
        params![group.get(), since],
    )? > 0)
}

/// Adds a rule to a group.
pub(crate) fn add_rule(
    connection: &Connection,
    group: GroupId,
    rule: &Rule,
) -> Result<(), DbError> {
    let rule = to_json(rule, "rules.rule")?;
    connection.execute(
        "INSERT INTO rules (group_id, rule) VALUES (?1, ?2)",
        params![group.get(), rule],
    )?;
    Ok(())
}

/// Deletes a rule; `false` if it does not exist.
pub(crate) fn delete_rule(connection: &Connection, rule: i64) -> Result<bool, DbError> {
    Ok(connection.execute("DELETE FROM rules WHERE id = ?1", [rule])? > 0)
}

/// Records a change of a day's allowance.
pub(crate) fn add_adjustment(
    connection: &Connection,
    group: GroupId,
    adjustment: Adjustment,
) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO adjustments (group_id, day, minutes) VALUES (?1, ?2, ?3)",
        params![
            group.get(),
            adjustment.day.to_string(),
            adjustment.minutes.get()
        ],
    )?;
    Ok(())
}

/// Adjustments of every group for `since` or later, oldest first.
pub(crate) fn adjustments_since(
    connection: &Connection,
    since: BudgetDay,
) -> Result<Vec<(GroupId, Adjustment)>, DbError> {
    let mut statement = connection
        .prepare("SELECT group_id, day, minutes FROM adjustments WHERE day >= ?1 ORDER BY id")?;
    statement
        .query_map([since.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
            ))
        })?
        .map(|row| {
            let (group, text, minutes) = row?;
            let adjustment = Adjustment {
                day: day(&text, "adjustments.day")?,
                minutes: MinuteDelta::new(minutes),
            };
            Ok((GroupId::new(group), adjustment))
        })
        .collect()
}

#[cfg(test)]
mod tests;
