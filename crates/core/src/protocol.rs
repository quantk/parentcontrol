//! Messages between agent and server.
//!
//! They travel as JSON text frames over one WebSocket per agent. The agent
//! opens with [`AgentMessage::Enroll`] once, when joining, and with
//! [`AgentMessage::Hello`] on every later connection; the server answers
//! `Hello` with the current [`ServerMessage::Policy`] and pushes a new one
//! whenever anything affecting the device changes.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, GroupId};
use crate::policy::Policy;
use crate::process::OsUser;
use crate::usage::{UsageRecord, UsageRecordId};

/// Version of this protocol; the server rejects agents speaking another.
pub const PROTOCOL_VERSION: u32 = 1;

/// The operating system of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlatform {
    /// Any Linux distribution.
    Linux,
    /// Microsoft Windows.
    Windows,
}

impl DevicePlatform {
    /// The stable identifier used in JSON and storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

impl fmt::Display for DevicePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Linux => "Linux",
            Self::Windows => "Windows",
        })
    }
}

/// A string that is not a [`DevicePlatform`] identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown platform {0:?}")]
pub struct UnknownPlatform(String);

impl FromStr for DevicePlatform {
    type Err = UnknownPlatform;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        [Self::Linux, Self::Windows]
            .into_iter()
            .find(|platform| platform.as_str() == s)
            .ok_or_else(|| UnknownPlatform(s.to_owned()))
    }
}

/// The secret an enrolled agent authenticates with.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentToken(String);

impl AgentToken {
    /// Wraps a token string.
    #[must_use]
    pub const fn new(token: String) -> Self {
        Self(token)
    }

    /// The token string.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AgentToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentToken(…)")
    }
}

/// A message from an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    /// Join the server with a one-time code from the parent's panel.
    Enroll {
        /// The code shown in the panel.
        code: String,
        /// This device's OS.
        platform: DevicePlatform,
    },
    /// Authenticate an enrolled agent; must be the first message.
    Hello {
        /// The token received on enrollment.
        token: AgentToken,
        /// [`PROTOCOL_VERSION`] of the agent.
        protocol: u32,
        /// This device's OS.
        platform: DevicePlatform,
        /// Local accounts the parent may bind to a child profile.
        local_users: Vec<OsUser>,
    },
    /// Usage not yet confirmed and what runs now; also serves as heartbeat.
    Report {
        /// Records in their current state; the server keeps the longest
        /// version of each, so resending is harmless.
        records: Vec<UsageRecord>,
        /// Groups with a running program right now.
        active_groups: Vec<GroupId>,
    },
}

/// Why the server refused an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    /// The enrollment code is unknown, used or expired.
    InvalidCode,
    /// The token is unknown or the device was deleted.
    InvalidToken,
    /// The agent speaks another protocol version.
    UnsupportedProtocol,
    /// The first message was neither `Enroll` nor `Hello`, or came twice.
    UnexpectedMessage,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCode => "the enrollment code is unknown, used or expired",
            Self::InvalidToken => "the agent token is not valid; enroll the device again",
            Self::UnsupportedProtocol => "the agent and server versions are incompatible",
            Self::UnexpectedMessage => "unexpected message",
        })
    }
}

/// The server's confirmation that it stored a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acknowledgement {
    /// The record.
    pub id: UsageRecordId,
    /// The duration stored for it.
    #[serde(rename = "duration_ms", with = "crate::usage::millis")]
    pub duration: Duration,
}

/// A message from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Enrollment succeeded.
    Enrolled {
        /// The new device.
        device: DeviceId,
        /// The token to authenticate with from now on.
        token: AgentToken,
    },
    /// The connection is refused; the server closes it next.
    Rejected {
        /// Why.
        reason: Rejection,
    },
    /// The policy the device must apply from now on.
    Policy {
        /// The policy snapshot.
        policy: Box<Policy>,
        /// The server's clock when sending, to correct the agent's clock.
        server_time: Timestamp,
    },
    /// Records the server has stored.
    Acknowledged {
        /// One entry per record of the last report.
        records: Vec<Acknowledgement>,
    },
}

#[cfg(test)]
mod tests;
