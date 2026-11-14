//! Session cookies and the [`Parent`] extractor guarding panel pages.

use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use jiff::Timestamp;

use crate::db::sessions;
use crate::error::AppError;
use crate::secret;
use crate::state::AppState;

/// Name of the session cookie.
pub(crate) const SESSION_COOKIE: &str = "parentcontrol_session";

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
        let Some(token) = session_token(&parts.headers) else {
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

/// The session token sent by the browser, if any.
pub(crate) fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|cookie| {
            let (name, value) = cookie.trim().split_once('=')?;
            (name == SESSION_COOKIE).then(|| value.to_owned())
        })
}

/// A `Set-Cookie` value carrying `token` for `lifetime`; an empty token
/// with zero lifetime deletes the cookie.
///
/// `SameSite=Strict` keeps other sites from submitting the panel's forms
/// with the parent's session, so the forms need no separate CSRF token.
pub(crate) fn session_cookie(token: &str, lifetime: Duration) -> HeaderValue {
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Strict",
        lifetime.as_secs()
    );
    // Tokens are hex, so the value is always a valid header.
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_session_among_other_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("theme=dark; parentcontrol_session=abc123; x=y"),
        );
        assert_eq!(session_token(&headers).as_deref(), Some("abc123"));
    }

    #[test]
    fn no_cookie_means_no_session() {
        assert_eq!(session_token(&HeaderMap::new()), None);
    }

    #[test]
    fn cookie_is_locked_down() {
        let cookie = session_cookie("abc", Duration::from_hours(1));
        let text = cookie.to_str().unwrap();
        for attribute in ["HttpOnly", "Secure", "SameSite=Strict", "Max-Age=3600"] {
            assert!(text.contains(attribute), "{text}");
        }
    }
}
