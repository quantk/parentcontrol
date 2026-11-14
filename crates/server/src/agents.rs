//! The agent side of the server: WebSocket sessions, enrollment and the
//! policy each device receives.

mod hub;
mod policy;
mod socket;

pub(crate) use hub::{Hub, Live};
pub(crate) use socket::upgrade;
