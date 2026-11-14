//! Identifiers of domain entities.
//!
//! Values are assigned by the server's storage and are opaque elsewhere.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! storage_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(i64);

        impl $name {
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

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

storage_id!(
    /// A child profile.
    ProfileId
);

storage_id!(
    /// A group of programs that share one quota and schedule.
    GroupId
);

storage_id!(
    /// A computer running the agent.
    DeviceId
);
