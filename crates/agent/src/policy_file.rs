//! The policy snapshot on disk, reloaded whenever the file changes.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use parentcontrol_core::policy::Policy;
use tracing::{error, info, warn};

use crate::store::{StoreError, write_json};

/// The last valid policy read from a file.
#[derive(Debug)]
pub(crate) struct PolicyFile {
    path: PathBuf,
    /// Modification time of the content last looked at; `None` if the file
    /// was missing.
    seen: Option<SystemTime>,
    policy: Option<Policy>,
}

impl PolicyFile {
    /// Watches `path`; nothing is read until [`Self::refresh`].
    pub(crate) const fn new(path: PathBuf) -> Self {
        Self {
            path,
            seen: None,
            policy: None,
        }
    }

    /// Rereads the file if it changed and returns the current policy.
    ///
    /// An invalid file keeps the previous policy in force, so a botched
    /// edit does not lift the limits. A deleted file lifts them, as on a
    /// device that was never configured.
    pub(crate) fn refresh(&mut self) -> Option<&Policy> {
        let modified = match fs::metadata(&self.path).and_then(|meta| meta.modified()) {
            Ok(modified) => Some(modified),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                warn!(%error, path = %self.path.display(), "cannot inspect policy file");
                return self.policy.as_ref();
            }
        };
        if modified != self.seen {
            self.seen = modified;
            self.reload(modified.is_some());
        }
        self.policy.as_ref()
    }

    /// Stores `policy` as the new snapshot and puts it in force.
    pub(crate) fn replace(&mut self, policy: Policy) -> Result<(), StoreError> {
        write_json(&self.path, &policy)?;
        self.seen = fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok();
        self.policy = Some(policy);
        Ok(())
    }

    fn reload(&mut self, exists: bool) {
        if !exists {
            info!(path = %self.path.display(), "no policy file; nothing is limited");
            self.policy = None;
            return;
        }
        let parsed = fs::read_to_string(&self.path)
            .map_err(|error| error.to_string())
            .and_then(|text| {
                serde_json::from_str::<Policy>(&text).map_err(|error| error.to_string())
            });
        match parsed {
            Ok(policy) => {
                info!(path = %self.path.display(), groups = policy.groups().len(), "policy loaded");
                self.policy = Some(policy);
            }
            Err(error) => {
                error!(%error, path = %self.path.display(), "invalid policy file; keeping the previous policy");
            }
        }
    }
}

#[cfg(test)]
mod tests;
