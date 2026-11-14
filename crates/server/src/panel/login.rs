//! Logging in and out.

use std::time::Duration;

use askama::Template;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use jiff::Timestamp;
use serde::Deserialize;

use super::render;
use crate::auth::{Parent, session_cookie, verify_password};
use crate::db::{DbError, sessions, settings};
use crate::error::AppError;
use crate::secret;
use crate::state::AppState;

/// How long a login lasts.
const SESSION_LIFETIME: Duration = Duration::from_hours(24 * 30);

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage {
    no_password: bool,
    failed: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct LoginQuery {
    #[serde(default)]
    failed: bool,
}

pub(super) async fn page(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Result<Response, AppError> {
    let no_password = state
        .db
        .call(|c| settings::password_hash(c))
        .await?
        .is_none();
    Ok(render(&LoginPage {
        no_password,
        failed: query.failed,
    })?
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(super) struct LoginForm {
    password: String,
}

pub(super) async fn submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    let db = state.db.clone();
    let passed = state
        .throttle
        .check(async move {
            let hash = db.call(|c| settings::password_hash(c)).await?;
            // Argon2 is slow on purpose; keep it off the async workers.
            let verified = tokio::task::spawn_blocking(move || {
                hash.is_some_and(|hash| verify_password(&form.password, &hash))
            })
            .await
            .map_err(DbError::from)?;
            Ok::<_, DbError>(verified)
        })
        .await?;
    if !passed {
        return Ok(Redirect::to("/login?failed=true").into_response());
    }

    let token = secret::token()?;
    let hash = secret::hash(&token);
    let expires = Timestamp::now()
        .checked_add(SESSION_LIFETIME)
        .unwrap_or(Timestamp::MAX);
    state
        .db
        .call(move |c| sessions::create(c, &hash, expires))
        .await?;
    Ok((
        [(SET_COOKIE, session_cookie(&token, SESSION_LIFETIME))],
        Redirect::to("/"),
    )
        .into_response())
}

pub(super) async fn logout(
    State(state): State<AppState>,
    parent: Parent,
) -> Result<Response, AppError> {
    state
        .db
        .call(move |c| sessions::delete(c, &parent.session_hash))
        .await?;
    Ok((
        [(SET_COOKIE, session_cookie("", Duration::ZERO))],
        Redirect::to("/login"),
    )
        .into_response())
}
