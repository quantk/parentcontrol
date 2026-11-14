//! Platform-independent heart of parentcontrol.
//!
//! This crate holds the domain types, the budget and permission calculation,
//! matching of rules against processes and the agent–server protocol types.
//! It knows nothing about the network, the disk or the operating system:
//! time and the process list reach it through traits, so everything here is
//! testable without a real system.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
