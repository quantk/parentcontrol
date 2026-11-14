//! Identifiers of domain entities.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A group of programs that share one quota and schedule.
///
/// The value is assigned by the server's storage and is opaque elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(i64);

impl GroupId {
    /// Wraps a raw storage identifier.
    #[must_use]
    pub const fn new(raw: i64) -> Self {
        Self(raw)
    }

    /// The raw storage identifier.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
