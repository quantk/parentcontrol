//! Units of time used by quotas and the parent's adjustments.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A whole number of minutes, as the parent configures quotas.
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

/// A signed whole number of minutes: time the parent adds or takes away.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MinuteDelta(i32);

impl MinuteDelta {
    /// No change.
    pub const ZERO: Self = Self(0);

    /// Wraps a signed number of minutes.
    #[must_use]
    pub const fn new(minutes: i32) -> Self {
        Self(minutes)
    }

    /// The signed number of minutes.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The change from `from` to `to`.
    #[must_use]
    pub fn between(from: Minutes, to: Minutes) -> Self {
        let delta = i64::from(to.get()) - i64::from(from.get());
        Self(i32::try_from(delta).unwrap_or(if delta < 0 { i32::MIN } else { i32::MAX }))
    }

    /// `minutes` changed by this delta, never below zero.
    #[must_use]
    pub fn apply(self, minutes: Minutes) -> Minutes {
        let changed = i64::from(minutes.get()) + i64::from(self.0);
        Minutes::new(u32::try_from(changed.max(0)).unwrap_or(u32::MAX))
    }
}

impl std::iter::Sum for MinuteDelta {
    fn sum<I: Iterator<Item = Self>>(deltas: I) -> Self {
        Self(deltas.fold(0, |total: i32, delta| total.saturating_add(delta.0)))
    }
}

impl fmt::Display for MinuteDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:+} min", self.0)
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
    fn deltas_never_take_below_zero() {
        let hour = Minutes::new(60);
        assert_eq!(MinuteDelta::new(15).apply(hour), Minutes::new(75));
        assert_eq!(MinuteDelta::new(-15).apply(hour), Minutes::new(45));
        assert_eq!(MinuteDelta::new(-90).apply(hour), Minutes::ZERO);
    }

    #[test]
    fn delta_between_two_amounts() {
        let delta = MinuteDelta::between(Minutes::new(60), Minutes::new(20));
        assert_eq!(delta, MinuteDelta::new(-40));
        assert_eq!(delta.apply(Minutes::new(60)), Minutes::new(20));
    }

    #[test]
    fn deltas_add_up_without_overflow() {
        let total: MinuteDelta = [MinuteDelta::new(i32::MAX), MinuteDelta::new(1)]
            .into_iter()
            .sum();
        assert_eq!(total, MinuteDelta::new(i32::MAX));
        assert_eq!(MinuteDelta::new(-5).to_string(), "-5 min");
    }

    #[test]
    fn converts_to_duration() {
        assert_eq!(Minutes::new(15).to_duration(), Duration::from_mins(15));
    }
}
