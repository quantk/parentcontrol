//! The parent's web panel: server-rendered pages with plain HTML forms.
//!
//! Every change goes through a `POST` form and ends in a redirect back to
//! the page, with an error message in the query string when the input was
//! rejected. Changes that affect devices notify the agents at once.

mod devices;
mod format;
mod groups;
mod history;
mod home;
mod login;
mod schedule;
mod settings;
mod status;

use askama::Template;
use axum::Router;
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, Redirect};
use axum::routing::{get, post};
use jiff::Timestamp;
use serde::Deserialize;

use crate::db::actions;
use crate::error::AppError;
use crate::state::AppState;

const STYLE: &str = include_str!("../static/style.css");

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
        .route("/groups/{id}/bonus", post(home::bonus))
        .route("/groups/{id}/block", post(home::block))
        .route("/groups/{id}/unblock", post(home::unblock))
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
        .route(
            "/static/style.css",
            get(|| async { ([(CONTENT_TYPE, "text/css; charset=utf-8")], STYLE) }),
        )
}

/// What the shared layout needs from every page.
#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) title: &'static str,
    /// Path of the navigation entry to highlight.
    pub(crate) active: &'static str,
    /// A message about a rejected input, shown above the content.
    pub(crate) error: Option<String>,
}

/// The `?error=` a redirect back to a page may carry.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Feedback {
    pub(crate) error: Option<String>,
}

/// Renders a page.
pub(crate) fn render(page: &impl Template) -> Result<Html<String>, AppError> {
    Ok(Html(page.render()?))
}

/// Redirects back to `path`, optionally reporting `error`.
pub(crate) fn back(path: &str, error: Option<&str>) -> Redirect {
    match error.and_then(|error| serde_urlencoded::to_string([("error", error)]).ok()) {
        Some(query) => Redirect::to(&format!("{path}?{query}")),
        None => Redirect::to(path),
    }
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
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(valid_name("  Игры "), Ok("Игры".to_owned()));
        assert!(valid_name("   ").is_err());
        assert!(valid_name(&"я".repeat(MAX_NAME + 1)).is_err());
    }

    #[test]
    fn errors_travel_in_the_query_string() {
        let redirect = back("/groups", Some("Нет & да")).into_response();
        let location = redirect.headers()["location"].to_str().unwrap();
        assert_eq!(
            location,
            "/groups?error=%D0%9D%D0%B5%D1%82+%26+%D0%B4%D0%B0"
        );
    }
}
