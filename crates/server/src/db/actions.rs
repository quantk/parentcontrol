//! The log of the parent's actions.

use jiff::Timestamp;
use rusqlite::{Connection, params};

use super::DbError;
use super::convert::{text, timestamp};

/// One logged action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParentAction {
    pub(crate) at: Timestamp,
    pub(crate) summary: String,
}

/// Appends an action described for the parent, in Russian.
pub(crate) fn log(connection: &Connection, at: Timestamp, summary: &str) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO parent_actions (at, summary) VALUES (?1, ?2)",
        params![text(at), summary],
    )?;
    Ok(())
}

/// The latest `limit` actions, newest first.
pub(crate) fn recent(connection: &Connection, limit: u32) -> Result<Vec<ParentAction>, DbError> {
    let mut statement =
        connection.prepare("SELECT at, summary FROM parent_actions ORDER BY id DESC LIMIT ?1")?;
    statement
        .query_map([limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .map(|row| {
            let (at, summary) = row?;
            Ok(ParentAction {
                at: timestamp(&at, "parent_actions.at")?,
                summary,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[tokio::test]
    async fn newest_actions_come_first() {
        let db = Db::in_memory().unwrap();
        let actions = db
            .call(|c| {
                log(c, "2026-11-14T10:00:00Z".parse().unwrap(), "первое")?;
                log(c, "2026-11-14T11:00:00Z".parse().unwrap(), "второе")?;
                recent(c, 10)
            })
            .await
            .unwrap();
        let summaries: Vec<&str> = actions.iter().map(|a| a.summary.as_str()).collect();
        assert_eq!(summaries, ["второе", "первое"]);
    }
}
