//! Platform-independent heart of parentcontrol.
//!
//! This crate holds the domain types, the budget and permission calculation,
//! matching of rules against processes and the agent–server protocol types.
//! It knows nothing about the network, the disk or the operating system:
//! time and the process list reach it through traits, so everything here is
//! testable without a real system.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod budget;
pub mod calendar;
pub mod clock;
pub mod enforcement;
pub mod ids;
pub mod policy;
pub mod process;
pub mod protocol;
pub mod rules;
pub mod schedule;
pub mod tray;
pub mod units;
pub mod usage;

#[cfg(test)]
mod test_support;
