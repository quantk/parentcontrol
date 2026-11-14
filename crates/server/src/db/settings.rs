//! Server-wide settings: calendar and the parent's password.

use jiff::tz::TimeZone;
use parentcontrol_core::calendar::{Calendar, TimeOfDay};
use rusqlite::{Connection, OptionalExtension, params};

use super::DbError;
use super::convert::corrupt;

const TIME_ZONE: &str = "time_zone";
const DAY_START: &str = "day_start";
const PASSWORD_HASH: &str = "password_hash";

fn get(connection: &Connection, key: &str) -> Result<Option<String>, DbError> {
    let value = connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(value)
}

fn set(connection: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// The stored calendar, if one was ever saved.
pub(crate) fn calendar(connection: &Connection) -> Result<Option<Calendar>, DbError> {
    let (Some(zone), Some(start)) = (get(connection, TIME_ZONE)?, get(connection, DAY_START)?)
    else {
        return Ok(None);
    };
    let zone = TimeZone::get(&zone).map_err(|error| corrupt(TIME_ZONE, error))?;
    let start: TimeOfDay = start.parse().map_err(|error| corrupt(DAY_START, error))?;
    Ok(Some(Calendar::new(zone, start)))
}

/// The stored calendar; the server stores one on its first start, so its
/// absence means a damaged database.
pub(crate) fn required_calendar(connection: &Connection) -> Result<Calendar, DbError> {
    calendar(connection)?.ok_or_else(|| corrupt("settings", "no calendar stored"))
}

/// Saves the calendar.
pub(crate) fn set_calendar(connection: &Connection, calendar: &Calendar) -> Result<(), DbError> {
    // Zones built from a name always carry it; `UTC` covers fixed offsets.
    let zone = calendar.time_zone().iana_name().unwrap_or("UTC");
    set(connection, TIME_ZONE, zone)?;
    set(connection, DAY_START, &calendar.day_start().to_string())
}

/// The parent's password hash in PHC format, if a password was set.
pub(crate) fn password_hash(connection: &Connection) -> Result<Option<String>, DbError> {
    get(connection, PASSWORD_HASH)
}

/// Replaces the parent's password hash.
pub(crate) fn set_password_hash(connection: &Connection, hash: &str) -> Result<(), DbError> {
    set(connection, PASSWORD_HASH, hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[tokio::test]
    async fn calendar_round_trips() {
        let db = Db::in_memory().unwrap();
        let saved = db
            .call(|connection| {
                assert!(calendar(connection)?.is_none());
                let calendar = Calendar::new(
                    TimeZone::get("Europe/Moscow").unwrap(),
                    "05:30".parse().unwrap(),
                );
                set_calendar(connection, &calendar)?;
                Ok(calendar)
            })
            .await
            .unwrap();
        let loaded = db.call(|connection| calendar(connection)).await.unwrap();
        assert_eq!(loaded, Some(saved));
    }
}
