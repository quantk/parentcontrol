//! Errors of request handlers.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;
use tracing::error;

use crate::db::DbError;

/// A failure while handling a request that is not the user's fault.
#[derive(Debug, Error)]
pub(crate) enum AppError {
    /// Storage failed.
    #[error(transparent)]
    Db(#[from] DbError),
    /// A page did not render.
    #[error("rendering a page: {0}")]
    Template(#[from] askama::Error),
    /// The OS random generator failed.
    #[error("generating a secret: {0}")]
    Random(#[from] getrandom::Error),
    /// Hashing a password failed.
    #[error(transparent)]
    Password(#[from] crate::auth::PasswordError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(error = %self, "request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Внутренняя ошибка сервера. Подробности в журнале сервера.",
        )
            .into_response()
    }
}
