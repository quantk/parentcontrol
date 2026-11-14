//! Units of time used by quotas and bonuses.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A whole number of minutes, as the parent configures quotas and bonuses.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Minutes(u32);

impl Minutes {
    /// Zero minutes.
    pub const ZERO: Self = Self(0);

    /// Wraps a number of minutes.
    #[must_use]
    pub const fn new(minutes: u32) -> Self {
        Self(minutes)
    }

    /// The number of minutes.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The same span as a [`Duration`].
    #[must_use]
    pub const fn to_duration(self) -> Duration {
        Duration::from_mins(self.0 as u64)
    }

    /// Whole minutes needed to cover `duration`, rounded up.
    ///
    /// Used for messages such as "1 minute left" when 20 seconds remain.
    #[must_use]
    pub fn ceil_from(duration: Duration) -> Self {
        const NANOS_PER_MINUTE: u128 = 60 * 1_000_000_000;
        let minutes = duration.as_nanos().div_ceil(NANOS_PER_MINUTE);
        Self(u32::try_from(minutes).unwrap_or(u32::MAX))
    }
}

impl fmt::Display for Minutes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} min", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceil_rounds_partial_minutes_up() {
        assert_eq!(Minutes::ceil_from(Duration::ZERO), Minutes::new(0));
        assert_eq!(Minutes::ceil_from(Duration::from_secs(1)), Minutes::new(1));
        assert_eq!(Minutes::ceil_from(Duration::from_secs(60)), Minutes::new(1));
        assert_eq!(
            Minutes::ceil_from(Duration::from_millis(60_001)),
            Minutes::new(2)
        );
        assert_eq!(
            Minutes::ceil_from(Duration::from_millis(500)),
            Minutes::new(1)
        );
    }

    #[test]
    fn converts_to_duration() {
        assert_eq!(Minutes::new(15).to_duration(), Duration::from_mins(15));
    }
}
