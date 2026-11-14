//! Parent control server: the agents' WebSocket API, the parent's web
//! panel and the SQLite storage behind both.

mod agents;
mod auth;
mod config;
mod cookie;
mod db;
mod error;
mod headers;
mod health;
mod panel;
mod secret;
mod state;
mod tls;

use std::error::Error as StdError;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use axum_server::Handle;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::info;

pub use crate::config::Config;
use crate::db::{Db, sessions, settings};
pub use crate::health::check as check_health;
use crate::state::AppState;
pub use crate::tls::TlsError;

/// How long open connections may finish after a stop is requested.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// A failure to start or administer the server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// A file system or network operation failed.
    #[error("{action}: {source}")]
    Io {
        /// What was being done.
        action: String,
        /// The underlying error.
        source: io::Error,
    },
    /// The database failed.
    #[error("storage: {0}")]
    Storage(#[source] Box<dyn StdError + Send + Sync>),
    /// The certificate could not be loaded.
    #[error(transparent)]
    Tls(#[from] TlsError),
    /// The password was rejected or could not be hashed.
    #[error("{0}")]
    Password(String),
    /// The health check failed.
    #[error("unhealthy: {0}")]
    Unhealthy(String),
}

impl From<db::DbError> for ServerError {
    fn from(error: db::DbError) -> Self {
        Self::Storage(Box::new(error))
    }
}

fn io_error(action: &str) -> impl FnOnce(io::Error) -> ServerError + '_ {
    move |source| ServerError::Io {
        action: action.to_owned(),
        source,
    }
}

/// A running server.
#[derive(Debug)]
pub struct Server {
    address: SocketAddr,
    fingerprint: String,
    handle: Handle<SocketAddr>,
    task: JoinHandle<io::Result<()>>,
}

impl Server {
    /// Opens storage, loads or creates the certificate and starts serving.
    ///
    /// # Errors
    ///
    /// Fails when the data directory, database, certificate or listening
    /// address cannot be used.
    pub async fn start(config: Config) -> Result<Self, ServerError> {
        // Fails only if a provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();

        std::fs::create_dir_all(&config.data_dir)
            .map_err(io_error("creating the data directory"))?;
        let db = Db::open(&config.database_path())?;
        let initial = config.initial_calendar();
        db.call(move |c| {
            if settings::calendar(c)?.is_none() {
                settings::set_calendar(c, &initial)?;
            }
            Ok(())
        })
        .await?;

        let tls = tls::load(&config).await?;
        let state = AppState {
            db,
            hub: Arc::default(),
            throttle: Arc::default(),
            fingerprint: tls.fingerprint.clone().into(),
        };
        let app = Router::new()
            .merge(panel::routes())
            .route("/api/agent", get(agents::upgrade))
            .route("/healthz", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(headers::harden))
            .with_state(state);

        let handle = Handle::new();
        let server = axum_server::bind_rustls(config.listen, tls.config).handle(handle.clone());
        let task = tokio::spawn(server.serve(app.into_make_service()));
        let address = handle.listening().await.ok_or_else(|| ServerError::Io {
            action: format!("listening on {}", config.listen),
            source: io::Error::other("the listener did not start"),
        })?;
        info!(%address, fingerprint = %tls.fingerprint, "server started");
        Ok(Self {
            address,
            fingerprint: tls.fingerprint,
            handle,
            task,
        })
    }

    /// The address the server listens on.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// SHA-256 of the served certificate in lowercase hex.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Serves until `stop` resolves, then lets connections finish briefly.
    ///
    /// # Errors
    ///
    /// Fails when serving stops with an I/O error.
    pub async fn run_until(self, stop: impl Future<Output = ()>) -> Result<(), ServerError> {
        let Self {
            handle, mut task, ..
        } = self;
        let finished = tokio::select! {
            () = stop => None,
            result = &mut task => Some(result),
        };
        let joined = if let Some(result) = finished {
            result
        } else {
            handle.graceful_shutdown(Some(SHUTDOWN_GRACE));
            task.await
        };
        let result = joined.map_err(|error| ServerError::Io {
            action: "serving".to_owned(),
            source: io::Error::other(error),
        })?;
        result.map_err(io_error("serving"))
    }
}

/// Sets the parent's password and ends all sessions.
///
/// # Errors
///
/// Fails when the password is too short or the database is unusable.
pub async fn set_password(data_dir: &Path, password: &str) -> Result<(), ServerError> {
    std::fs::create_dir_all(data_dir).map_err(io_error("creating the data directory"))?;
    let hash =
        auth::hash_password(password).map_err(|error| ServerError::Password(error.to_string()))?;
    let db = Db::open(&config::database_path(data_dir))?;
    db.call(move |c| {
        settings::set_password_hash(c, &hash)?;
        sessions::delete_all(c)
    })
    .await?;
    Ok(())
}
