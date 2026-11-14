//! State shared by all request handlers.

use std::sync::Arc;

use crate::agents::Hub;
use crate::auth::Throttle;
use crate::db::Db;

/// Everything a handler may need; cheap to clone.
#[derive(Debug, Clone)]
pub(crate) struct AppState {
    pub(crate) db: Db,
    /// Connected agents and change notifications for them.
    pub(crate) hub: Arc<Hub>,
    /// Guards the parent's password and enrollment codes against guessing.
    pub(crate) throttle: Arc<Throttle>,
    /// SHA-256 of the served certificate, shown to compare with agents.
    pub(crate) fingerprint: Arc<str>,
}
