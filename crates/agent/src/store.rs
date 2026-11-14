//! The agent's files: policy snapshot, usage journal, server link, clock
//! correction.
//!
//! They live in a directory only administrators can read or write, so the
//! child can neither forge the policy nor erase recorded usage.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::{SignedDuration, Timestamp};
use parentcontrol_core::usage::Journal;
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tracing::error;

use crate::link::Link;
use crate::platform::{self, PlatformError};

const POLICY_FILE: &str = "policy.json";
const JOURNAL_FILE: &str = "journal.json";
const LINK_FILE: &str = "server.json";
const CLOCK_FILE: &str = "clock.json";

/// A failure to read or write the agent's files.
#[derive(Debug, Error)]
pub(crate) enum StoreError {
    /// The data directory could not be prepared.
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// A file could not be read or written.
    #[error("{}: {source}", .path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// A file does not hold what it should.
    #[error("{}: {source}", .path.display())]
    Json {
        /// The file.
        path: PathBuf,
        /// What is wrong with it.
        source: serde_json::Error,
    },
}

/// The agent's data directory.
#[derive(Debug, Clone)]
pub(crate) struct Store {
    dir: PathBuf,
}

impl Store {
    /// Opens `dir`, creating it with restricted access if needed.
    pub(crate) fn open(dir: PathBuf) -> Result<Self, StoreError> {
        platform::create_private_dir(&dir)?;
        Ok(Self { dir })
    }

    /// Path of another file in the data directory.
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Path of the policy snapshot.
    pub(crate) fn policy_path(&self) -> PathBuf {
        self.dir.join(POLICY_FILE)
    }

    /// Path of the server link written by enrollment.
    pub(crate) fn link_path(&self) -> PathBuf {
        self.dir.join(LINK_FILE)
    }

    /// Loads the journal; a missing file is an empty journal.
    ///
    /// A corrupt journal is moved aside rather than blocking the agent: an
    /// agent that does not start limits nothing at all.
    pub(crate) fn load_journal(&self) -> Result<Journal, StoreError> {
        let path = self.dir.join(JOURNAL_FILE);
        match read_json(&path) {
            Ok(journal) => Ok(journal.unwrap_or_default()),
            Err(StoreError::Json { source, .. }) => {
                let aside =
                    path.with_extension(format!("corrupt-{}", Timestamp::now().as_second()));
                error!(error = %source, aside = %aside.display(), "usage journal is corrupt; starting a new one");
                fs::rename(&path, &aside).map_err(|source| StoreError::Io { path, source })?;
                Ok(Journal::default())
            }
            Err(error) => Err(error),
        }
    }

    /// Saves the journal atomically.
    pub(crate) fn save_journal(&self, journal: &Journal) -> Result<(), StoreError> {
        write_json(&self.dir.join(JOURNAL_FILE), journal)
    }

    /// Saves the link to the server.
    pub(crate) fn save_link(&self, link: &Link) -> Result<(), StoreError> {
        write_json(&self.link_path(), link)
    }

    /// The last known difference between the server clock and this one.
    pub(crate) fn load_clock_offset(&self) -> Result<SignedDuration, StoreError> {
        Ok(read_json(&self.dir.join(CLOCK_FILE))?.unwrap_or_default())
    }

    /// Saves the difference between the server clock and this one.
    pub(crate) fn save_clock_offset(&self, offset: SignedDuration) -> Result<(), StoreError> {
        write_json(&self.dir.join(CLOCK_FILE), &offset)
    }
}

/// Reads JSON from `path`; `None` if the file does not exist.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StoreError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|source| StoreError::Json {
            path: path.to_owned(),
            source,
        })
}

/// Writes `value` as JSON atomically.
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), StoreError> {
    let json = serde_json::to_vec_pretty(value).map_err(|source| StoreError::Json {
        path: path.to_owned(),
        source,
    })?;
    write_atomically(path, &json)
}

/// Writes through a temporary file and a rename, so a crash or power loss
/// leaves either the old or the new content, never a torn file.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), StoreError> {
    let temporary = path.with_extension("tmp");
    let io_error = |source| StoreError::Io {
        path: temporary.clone(),
        source,
    };
    let mut file = File::create(&temporary).map_err(io_error)?;
    file.write_all(contents).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(|source| StoreError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests;
