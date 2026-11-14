//! Source of the current time.

use jiff::Timestamp;

/// Tells the current moment.
///
/// Implementations decide how much to trust the local clock; the agent, for
/// instance, corrects it with the offset reported by the server so that a
/// changed system clock does not shift budget days or windows.
pub trait Clock {
    /// The current moment.
    fn now(&self) -> Timestamp;
}
