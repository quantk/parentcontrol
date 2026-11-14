//! Usage records merged from all devices.

use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::{DeviceId, GroupId};
use parentcontrol_core::protocol::Acknowledgement;
use parentcontrol_core::usage::UsageRecord;
use rusqlite::{Connection, OptionalExtension, params};

use super::DbError;
use super::convert::{day, duration, millis, text, timestamp};

/// Usage of one group on one device during one day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DayTotal {
    pub(crate) day: BudgetDay,
    pub(crate) group: GroupId,
    pub(crate) device: DeviceId,
    pub(crate) used: Duration,
    /// Whether part of it was recorded offline.
    pub(crate) offline: bool,
}

/// A stored record with the device it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Session {
    pub(crate) day: BudgetDay,
    pub(crate) device: DeviceId,
    pub(crate) group: GroupId,
    pub(crate) start: Timestamp,
    pub(crate) duration: Duration,
    pub(crate) offline: bool,
}

/// Merges `records` from `device`, keeping the longest version of each.
///
/// Merging is idempotent, so an agent may resend freely after a lost
/// acknowledgement. A record id already stored for another device is left
/// untouched; one device cannot rewrite another's history.
pub(crate) fn merge(
    connection: &mut Connection,
    device: DeviceId,
    records: &[UsageRecord],
) -> Result<Vec<Acknowledgement>, DbError> {
    let transaction = connection.transaction()?;
    let mut acknowledgements = Vec::with_capacity(records.len());
    {
        let mut upsert = transaction.prepare(
            "INSERT INTO usage (id, device_id, group_id, day, start, duration_ms, offline)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET
                 duration_ms = max(duration_ms, excluded.duration_ms),
                 offline = max(offline, excluded.offline)
             WHERE device_id = excluded.device_id
             RETURNING duration_ms",
        )?;
        for record in records {
            let stored: Option<i64> = upsert
                .query_row(
                    params![
                        record.id.to_string(),
                        device.get(),
                        record.group.get(),
                        record.day.to_string(),
                        text(record.start),
                        millis(record.duration),
                        record.offline,
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(stored) = stored {
                acknowledgements.push(Acknowledgement {
                    id: record.id,
                    duration: duration(stored),
                });
            }
        }
    }
    transaction.commit()?;
    Ok(acknowledgements)
}

/// Per group, device and day totals for days from `first` to `last`.
pub(crate) fn totals(
    connection: &Connection,
    first: BudgetDay,
    last: BudgetDay,
) -> Result<Vec<DayTotal>, DbError> {
    let mut statement = connection.prepare(
        "SELECT day, group_id, device_id, sum(duration_ms), max(offline) FROM usage
         WHERE day BETWEEN ?1 AND ?2 GROUP BY day, group_id, device_id ORDER BY day DESC, group_id, device_id",
    )?;
    statement
        .query_map([first.to_string(), last.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?
        .map(|row| {
            let (text, group, device, used, offline) = row?;
            Ok(DayTotal {
                day: day(&text, "usage.day")?,
                group: GroupId::new(group),
                device: DeviceId::new(device),
                used: duration(used),
                offline,
            })
        })
        .collect()
}

/// Every record of the days from `first` to `last`, in order of start.
pub(crate) fn sessions(
    connection: &Connection,
    first: BudgetDay,
    last: BudgetDay,
) -> Result<Vec<Session>, DbError> {
    let mut statement = connection.prepare(
        "SELECT day, device_id, group_id, start, duration_ms, offline FROM usage
         WHERE day BETWEEN ?1 AND ?2 ORDER BY start",
    )?;
    statement
        .query_map([first.to_string(), last.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, bool>(5)?,
            ))
        })?
        .map(|row| {
            let (text, device, group, start, used, offline) = row?;
            Ok(Session {
                day: day(&text, "usage.day")?,
                device: DeviceId::new(device),
                group: GroupId::new(group),
                start: timestamp(&start, "usage.start")?,
                duration: duration(used),
                offline,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
