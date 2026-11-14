//! What the core knows about a running process.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Identity of a process that survives PID reuse.
///
/// `started` is an OS-specific start-time token (boot ticks on Linux,
/// creation time on Windows); together with the PID it names one process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId {
    pid: u32,
    started: u64,
}

impl ProcessId {
    /// Identity of the process `pid` that started at `started`.
    #[must_use]
    pub const fn new(pid: u32, started: u64) -> Self {
        Self { pid, started }
    }

    /// The OS process id.
    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid
    }

    /// The OS-specific start-time token.
    #[must_use]
    pub const fn started(self) -> u64 {
        self.started
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pid.fmt(f)
    }
}

/// Name of a local OS account, as the platform reports it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OsUser(String);

impl OsUser {
    /// Wraps an account name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The account name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OsUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A snapshot of one running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Which process this is.
    pub id: ProcessId,
    /// The owning account, if it could be resolved.
    pub user: Option<OsUser>,
    /// Absolute path of the executable, if readable.
    pub executable: Option<PathBuf>,
    /// The command line, one argument per element; empty if unreadable.
    pub command_line: Vec<String>,
}

/// Expensive per-process details, fetched only when a rule needs them.
pub trait ProcessInspector {
    /// Files the process has open or mapped into memory.
    ///
    /// Returns an empty list when the process is gone or unreadable.
    fn loaded_files(&self, id: ProcessId) -> Vec<PathBuf>;
}
