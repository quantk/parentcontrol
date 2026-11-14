//! The agent's notion of the current time.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use jiff::{SignedDuration, Timestamp};
use parentcontrol_core::clock::Clock;

/// The system clock corrected by the server's.
///
/// Budget days and windows follow the server's clock, so the child gains
/// nothing by changing the time on the PC; online, every policy push
/// refreshes the correction, offline the last one stays. Usage itself is
/// measured with the monotonic clock and does not depend on this at all.
#[derive(Debug, Clone, Default)]
pub(crate) struct ServerClock {
    /// Server time minus system time, in milliseconds.
    offset: Arc<AtomicI64>,
}

impl ServerClock {
    /// A clock corrected by `offset`.
    pub(crate) fn new(offset: SignedDuration) -> Self {
        let clock = Self::default();
        clock.set_offset(offset);
        clock
    }

    /// Learns the server's time and returns the new correction.
    pub(crate) fn synchronize(&self, server_time: Timestamp) -> SignedDuration {
        let offset = server_time.duration_since(Timestamp::now());
        self.set_offset(offset);
        offset
    }

    fn set_offset(&self, offset: SignedDuration) {
        let millis = i64::try_from(offset.as_millis()).unwrap_or(0);
        self.offset.store(millis, Ordering::Relaxed);
    }
}

impl Clock for ServerClock {
    fn now(&self) -> Timestamp {
        let offset = SignedDuration::from_millis(self.offset.load(Ordering::Relaxed));
        Timestamp::now()
            .checked_add(offset)
            .unwrap_or_else(|_| Timestamp::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrects_by_the_server_offset() {
        let clock = ServerClock::new(SignedDuration::from_hours(3));
        let ahead = clock.now().duration_since(Timestamp::now());
        assert!((ahead - SignedDuration::from_hours(3)).abs() < SignedDuration::from_secs(1));
    }

    #[test]
    fn synchronizing_measures_the_difference() {
        let clock = ServerClock::default();
        let server_time = Timestamp::now()
            .checked_sub(SignedDuration::from_mins(10))
            .unwrap();
        let offset = clock.synchronize(server_time);
        assert!((offset + SignedDuration::from_mins(10)).abs() < SignedDuration::from_secs(1));
        assert!(clock.now() < Timestamp::now());
    }
}
