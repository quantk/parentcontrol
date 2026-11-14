//! The parent's web panel: server-rendered pages with plain HTML forms,
//! enhanced by htmx where it is available.
//!
//! Every change goes through a `POST` form and ends in a redirect back to
//! the page, with a one-shot message saying what was done or why the input
//! was rejected. Changes that affect devices notify the agents at once.
//!
//! Without JavaScript every page still works with full reloads. With it,
//! htmx turns links and forms into requests that swap the page body, and
//! parts of a page refresh on their own; the server renders the same full
//! pages either way, and the browser picks the parts it needs.

mod assets;
mod controls;
mod devices;
mod flash;
mod format;
mod groups;
mod history;
mod home;
mod icon;
mod login;
mod presence;
mod schedule;
mod settings;
mod status;

use askama::Template;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Router, middleware};
use jiff::Timestamp;

pub(crate) use self::flash::Flash;
pub(crate) use self::icon::Icon;
use crate::db::actions;
use crate::error::AppError;
use crate::state::AppState;

/// All panel routes.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(home::page))
        .route("/login", get(login::page).post(login::submit))
        .route("/logout", post(login::logout))
        .route("/groups", get(groups::page).post(groups::create))
        .route("/groups/{id}/rename", post(groups::rename))
        .route("/groups/{id}/delete", post(groups::delete))
        .route("/groups/{id}/rules", post(groups::add_rule))
        .route("/groups/{id}/adjust", post(controls::adjust))
        .route("/groups/{id}/remaining", post(controls::set_remaining))
        .route("/groups/{id}/block", post(controls::block))
        .route("/groups/{id}/unblock", post(controls::unblock))
        .route("/groups/{id}/schedule", post(schedule::save))
        .route("/rules/{id}/delete", post(groups::delete_rule))
        .route("/schedule", get(schedule::page))
        .route("/devices", get(devices::page).post(devices::create))
        .route("/devices/{id}/code", post(devices::new_code))
        .route("/devices/{id}/rename", post(devices::rename))
        .route("/devices/{id}/delete", post(devices::delete))
        .route("/devices/{id}/bind", post(devices::bind))
        .route("/devices/{id}/unbind", post(devices::unbind))
        .route("/history", get(history::page))
        .route("/settings", get(settings::page))
        .route("/settings/password", post(settings::password))
        .route("/settings/calendar", post(settings::calendar))
        .merge(assets::routes())
        .layer(middleware::from_fn(flash::consume))
}

/// What the shared layout needs from every page.
#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) title: &'static str,
    /// Path of the navigation entry to highlight.
    pub(crate) active: &'static str,
    /// The outcome of the change that led here, if any.
    pub(crate) flash: Option<Flash>,
}

/// Renders a page.
pub(crate) fn render(page: &impl Template) -> Result<Html<String>, AppError> {
    Ok(Html(page.render()?))
}

/// Redirects to `path`.
pub(crate) fn back(path: &str) -> Response {
    Redirect::to(path).into_response()
}

/// Redirects to `path`, explaining why the input was rejected.
pub(crate) fn rejected(path: &str, reason: &str) -> Response {
    flash::back(path, &Flash::error(reason))
}

/// Logs a change, tells agents about it and redirects to `path`
/// confirming it.
pub(crate) async fn done(
    state: &AppState,
    path: &str,
    summary: String,
) -> Result<Response, AppError> {
    changed(state, summary.clone()).await?;
    Ok(flash::back(path, &Flash::done(summary)))
}

/// Longest name of a group or device accepted.
const MAX_NAME: usize = 60;

/// A trimmed, non-empty, bounded name, or the reason it is not one.
pub(crate) fn valid_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    match name.chars().count() {
        0 => Err("Название не может быть пустым."),
        length if length > MAX_NAME => Err("Название слишком длинное."),
        _ => Ok(name.to_owned()),
    }
}

/// Logs a parent action and tells agents to fetch their new policy.
pub(crate) async fn changed(state: &AppState, summary: String) -> Result<(), AppError> {
    state
        .db
        .call(move |c| actions::log(c, Timestamp::now(), &summary))
        .await?;
    state.hub.policy_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(valid_name("  Игры "), Ok("Игры".to_owned()));
        assert!(valid_name("   ").is_err());
        assert!(valid_name(&"я".repeat(MAX_NAME + 1)).is_err());
    }
}
