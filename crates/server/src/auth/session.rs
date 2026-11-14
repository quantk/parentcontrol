//! Session cookies and the [`Parent`] extractor guarding panel pages.

use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::HeaderValue;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use jiff::Timestamp;

use crate::cookie;
use crate::db::sessions;
use crate::error::AppError;
use crate::secret;
use crate::state::AppState;

/// Name of the session cookie.
const SESSION_COOKIE: &str = "parentcontrol_session";

/// A request made by the logged-in parent.
#[derive(Debug)]
pub(crate) struct Parent {
    /// Hash of the session token, to end the session on logout.
    pub(crate) session_hash: String,
}

impl FromRequestParts<AppState> for Parent {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = cookie::find(&parts.headers, SESSION_COOKIE) else {
            return Err(Redirect::to("/login").into_response());
        };
        let session_hash = secret::hash(&token);
        let lookup = session_hash.clone();
        let valid = state
            .db
            .call(move |connection| sessions::is_valid(connection, &lookup, Timestamp::now()))
            .await
            .map_err(|error| AppError::from(error).into_response())?;
        if valid {
            Ok(Self { session_hash })
        } else {
            Err(Redirect::to("/login").into_response())
        }
    }
}

/// A `Set-Cookie` value carrying session `token` for `lifetime`.
pub(crate) fn session_cookie(token: &str, lifetime: Duration) -> HeaderValue {
    // Tokens are hex, so they are always valid cookie values.
    cookie::set(SESSION_COOKIE, token, lifetime).unwrap_or_else(|| cookie::delete(SESSION_COOKIE))
}

/// A `Set-Cookie` value ending the session in the browser.
pub(crate) fn end_session_cookie() -> HeaderValue {
    cookie::delete(SESSION_COOKIE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_carries_the_token_and_ending_it_clears_it() {
        let cookie = session_cookie("abc123", Duration::from_hours(1));
        assert!(
            cookie
                .to_str()
                .unwrap()
                .starts_with("parentcontrol_session=abc123;")
        );
        let ended = end_session_cookie();
        let ended = ended.to_str().unwrap();
        assert!(ended.starts_with("parentcontrol_session=;") && ended.contains("Max-Age=0"));
    }
}
