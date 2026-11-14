//! The agent's notion of the current time.

use jiff::Timestamp;
use parentcontrol_core::clock::Clock;

/// The system clock.
///
/// Only budget days and windows depend on it; usage is measured with the
/// monotonic clock, so moving the system clock never adds play time.
#[derive(Debug, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}
