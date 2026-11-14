//! The agent's files: the policy snapshot and the usage journal.
//!
//! They live in a directory only administrators can read or write, so the
//! child can neither forge the policy nor erase recorded usage.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use parentcontrol_core::usage::Journal;
use thiserror::Error;
use tracing::error;

use crate::platform::{self, PlatformError};

const POLICY_FILE: &str = "policy.json";
const JOURNAL_FILE: &str = "journal.json";

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
    /// The journal could not be encoded.
    #[error("encoding the usage journal: {0}")]
    Encode(serde_json::Error),
}

/// The agent's data directory.
#[derive(Debug)]
pub(crate) struct Store {
    dir: PathBuf,
}

impl Store {
    /// Opens `dir`, creating it with restricted access if needed.
    pub(crate) fn open(dir: PathBuf) -> Result<Self, StoreError> {
        platform::create_private_dir(&dir)?;
        Ok(Self { dir })
    }

    /// Path of the policy snapshot.
    pub(crate) fn policy_path(&self) -> PathBuf {
        self.dir.join(POLICY_FILE)
    }

    /// Loads the journal; a missing file is an empty journal.
    ///
    /// A corrupt journal is moved aside rather than blocking the agent: an
    /// agent that does not start limits nothing at all.
    pub(crate) fn load_journal(&self) -> Result<Journal, StoreError> {
        let path = self.dir.join(JOURNAL_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Journal::default()),
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        match serde_json::from_str(&text) {
            Ok(journal) => Ok(journal),
            Err(parse_error) => {
                let aside =
                    path.with_extension(format!("corrupt-{}", Timestamp::now().as_second()));
                error!(%parse_error, aside = %aside.display(), "usage journal is corrupt; starting a new one");
                fs::rename(&path, &aside).map_err(|source| StoreError::Io { path, source })?;
                Ok(Journal::default())
            }
        }
    }

    /// Saves the journal atomically.
    pub(crate) fn save_journal(&self, journal: &Journal) -> Result<(), StoreError> {
        let json = serde_json::to_vec_pretty(journal).map_err(StoreError::Encode)?;
        write_atomically(&self.dir.join(JOURNAL_FILE), &json)
    }
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
