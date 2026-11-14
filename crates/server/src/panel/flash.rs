//! One-shot messages shown on the page a change redirects back to: what
//! was done, or why the input was rejected.
//!
//! A message travels in a short-lived cookie rather than in the query
//! string, so the address stays clean and reloading the page does not
//! repeat it. [`consume`] hands the message to the next page rendered and
//! deletes the cookie.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{OptionalFromRequestParts, Request};
use axum::http::header::SET_COOKIE;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};

use crate::cookie;

const COOKIE: &str = "parentcontrol_flash";

/// Long enough to survive the redirect, short enough not to linger.
const LIFETIME: Duration = Duration::from_mins(1);

/// What kind of message it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Tone {
    /// A change was made.
    Done,
    /// The input was rejected.
    Error,
}

/// A message for the parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Flash {
    pub(crate) tone: Tone,
    pub(crate) text: String,
}

impl Flash {
    /// Reports a change that was made.
    pub(crate) fn done(text: impl Into<String>) -> Self {
        Self {
            tone: Tone::Done,
            text: text.into(),
        }
    }

    /// Reports why the input was rejected.
    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self {
            tone: Tone::Error,
            text: text.into(),
        }
    }

    /// Whether the message reports a rejected input.
    pub(crate) fn is_error(&self) -> bool {
        self.tone == Tone::Error
    }

    fn decode(value: &str) -> Option<Self> {
        serde_urlencoded::from_str(value).ok()
    }

    fn encode(&self) -> Option<String> {
        serde_urlencoded::to_string(self).ok()
    }
}

/// Redirects to `path`, showing `flash` there.
pub(crate) fn back(path: &str, flash: &Flash) -> Response {
    let redirect = Redirect::to(path);
    match flash
        .encode()
        .and_then(|value| cookie::set(COOKIE, &value, LIFETIME))
    {
        Some(cookie) => ([(SET_COOKIE, cookie)], redirect).into_response(),
        None => redirect.into_response(),
    }
}

/// Middleware: makes a pending message available to the handler and
/// deletes it once a page has been rendered with it.
pub(crate) async fn consume(mut request: Request, next: Next) -> Response {
    let flash = cookie::find(request.headers(), COOKIE);
    if let Some(flash) = flash.as_deref().and_then(Flash::decode) {
        request.extensions_mut().insert(flash);
    }
    let mut response = next.run(request).await;
    // A redirect passes the message on to the page it leads to.
    if flash.is_some() && response.status().is_success() {
        response
            .headers_mut()
            .append(SET_COOKIE, cookie::delete(COOKIE));
    }
    response
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Flash {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        std::future::ready(Ok(parts.extensions.get::<Self>().cloned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_survive_the_cookie() {
        let flash = Flash::error("Нет & да; «кавычки»");
        let encoded = flash.encode().unwrap();
        assert!(
            cookie::set(COOKIE, &encoded, LIFETIME).is_some(),
            "{encoded}"
        );
        assert_eq!(Flash::decode(&encoded), Some(flash));
        assert_eq!(Flash::decode("garbage"), None);
    }

    #[test]
    fn back_redirects_and_sets_the_message() {
        let response = back("/groups", &Flash::done("Сохранено"));
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers()["location"], "/groups");
        let cookie = response.headers()[SET_COOKIE].to_str().unwrap();
        assert!(
            cookie.starts_with("parentcontrol_flash=tone=done&text="),
            "{cookie}"
        );
    }
}
