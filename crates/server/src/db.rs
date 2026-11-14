//! SQLite storage.
//!
//! Queries are plain synchronous functions over a [`Connection`], grouped
//! by entity in submodules; [`Db::call`] runs them on the blocking pool so
//! async handlers never stall the runtime.

pub(crate) mod actions;
mod convert;
pub(crate) mod devices;
pub(crate) mod groups;
pub(crate) mod sessions;
pub(crate) mod settings;
pub(crate) mod usage;

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use parentcontrol_core::ids::ProfileId;
use rusqlite::Connection;
use thiserror::Error;

/// The only profile the first version's interface manages.
pub(crate) const DEFAULT_PROFILE: ProfileId = ProfileId::new(1);

/// Migrations by resulting schema version; never edit a released one.
const MIGRATIONS: &[&str] = &[
    include_str!("db/migrations/0001_initial.sql"),
    include_str!("db/migrations/0002_adjustments.sql"),
];

/// A storage failure.
#[derive(Debug, Error)]
pub(crate) enum DbError {
    /// SQLite reported an error.
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored value does not parse; the database was edited by hand or
    /// written by an incompatible version.
    #[error("corrupt stored value in {column}: {reason}")]
    Corrupt {
        /// Where the value was found.
        column: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// The blocking task running the query panicked or was cancelled.
    #[error("database task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Shared handle to the database.
#[derive(Debug, Clone)]
pub(crate) struct Db {
    connection: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens or creates the database at `path` and migrates it.
    pub(crate) fn open(path: &Path) -> Result<Self, DbError> {
        Self::prepare(Connection::open(path)?)
    }

    /// An empty in-memory database, for tests.
    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, DbError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(mut connection: Connection) -> Result<Self, DbError> {
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "wal")?;
        migrate(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Runs `query` with exclusive access to the connection.
    pub(crate) async fn call<T, F>(&self, query: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || {
            // A panic inside a query cannot leave SQLite inconsistent: an
            // open transaction is rolled back when dropped.
            let mut connection = connection.lock().unwrap_or_else(PoisonError::into_inner);
            query(&mut connection)
        })
        .await?
    }
}

fn migrate(connection: &mut Connection) -> Result<(), DbError> {
    let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (version, migration) in (1_u32..).zip(MIGRATIONS).skip(current as usize) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        drop(Db::open(&path).unwrap());
        let mut connection = Connection::open(&path).unwrap();
        migrate(&mut connection).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }

    #[test]
    fn bonuses_become_adjustments() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(MIGRATIONS[0]).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection
            .execute_batch(
                "INSERT INTO program_groups (id, profile_id, name, schedule) VALUES (1, 1, 'Игры', '{}');
                 INSERT INTO bonuses (group_id, day, minutes) VALUES (1, '2026-11-14', 15);",
            )
            .unwrap();
        migrate(&mut connection).unwrap();
        let minutes: i32 = connection
            .query_row("SELECT minutes FROM adjustments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(minutes, 15);
    }
}
