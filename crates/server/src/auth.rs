//! The parent's authentication: password, sessions, throttling.

mod password;
mod session;
mod throttle;

pub(crate) use password::{PasswordError, hash_password, verify_password};
pub(crate) use session::{Parent, session_cookie};
pub(crate) use throttle::Throttle;
