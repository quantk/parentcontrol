//! Login sessions of the parent, stored by token hash.

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};

use super::DbError;
use super::convert::text;

/// Starts a session valid until `expires`.
pub(crate) fn create(
    connection: &Connection,
    token_hash: &str,
    expires: Timestamp,
) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO sessions (token_hash, expires) VALUES (?1, ?2)",
        params![token_hash, text(expires)],
    )?;
    Ok(())
}

/// Whether the session exists and has not expired; also purges expired ones.
pub(crate) fn is_valid(
    connection: &Connection,
    token_hash: &str,
    now: Timestamp,
) -> Result<bool, DbError> {
    let now = text(now);
    connection.execute("DELETE FROM sessions WHERE expires <= ?1", [&now])?;
    let found = connection
        .query_row(
            "SELECT 1 FROM sessions WHERE token_hash = ?1",
            [token_hash],
            |_| Ok(()),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Ends one session.
pub(crate) fn delete(connection: &Connection, token_hash: &str) -> Result<(), DbError> {
    connection.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])?;
    Ok(())
}

/// Ends every session, as after a password change.
pub(crate) fn delete_all(connection: &Connection) -> Result<(), DbError> {
    connection.execute("DELETE FROM sessions", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[tokio::test]
    async fn sessions_expire() {
        let db = Db::in_memory().unwrap();
        db.call(|c| {
            create(c, "a", "2026-11-14T12:00:00Z".parse().unwrap())?;
            assert!(is_valid(c, "a", "2026-11-14T11:00:00Z".parse().unwrap())?);
            assert!(!is_valid(c, "a", "2026-11-14T12:00:00Z".parse().unwrap())?);
            assert!(!is_valid(c, "b", "2026-11-14T11:00:00Z".parse().unwrap())?);
            Ok(())
        })
        .await
        .unwrap();
    }
}
