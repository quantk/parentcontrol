//! Reading and setting the panel's cookies.
//!
//! Every cookie is `HttpOnly`, `Secure` and `SameSite=Strict`: scripts
//! cannot read it, it never travels in clear text, and other sites cannot
//! make the browser send it, so the panel's forms need no CSRF token.

use std::time::Duration;

use axum::http::header::COOKIE;
use axum::http::{HeaderMap, HeaderValue};

/// The value of cookie `name` sent by the browser, if any.
pub(crate) fn find(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|cookie| {
            let (key, value) = cookie.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

/// A `Set-Cookie` value storing `value` under `name` for `lifetime`, or
/// `None` if `value` is not valid in a cookie.
pub(crate) fn set(name: &str, value: &str, lifetime: Duration) -> Option<HeaderValue> {
    let valid = value
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b',' | b';' | b'\\'));
    let cookie = format!(
        "{name}={value}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Strict",
        lifetime.as_secs()
    );
    valid.then(|| HeaderValue::from_str(&cookie).ok()).flatten()
}

/// A `Set-Cookie` value deleting cookie `name`.
pub(crate) fn delete(name: &str) -> HeaderValue {
    // An empty value is always valid; browsers ignore an empty header.
    set(name, "", Duration::ZERO).unwrap_or_else(|| HeaderValue::from_static(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("theme=dark; parentcontrol_session=abc123; x=y"),
        );
        assert_eq!(
            find(&headers, "parentcontrol_session").as_deref(),
            Some("abc123")
        );
        assert_eq!(find(&headers, "missing"), None);
        assert_eq!(find(&HeaderMap::new(), "x"), None);
    }

    #[test]
    fn cookies_are_locked_down() {
        let cookie = set("name", "abc", Duration::from_hours(1)).unwrap();
        let text = cookie.to_str().unwrap();
        for attribute in [
            "name=abc",
            "HttpOnly",
            "Secure",
            "SameSite=Strict",
            "Max-Age=3600",
        ] {
            assert!(text.contains(attribute), "{text}");
        }
    }

    #[test]
    fn rejects_values_that_would_break_the_header() {
        assert_eq!(set("name", "a;b", Duration::ZERO), None);
        assert_eq!(set("name", "a b", Duration::ZERO), None);
        assert!(delete("name").to_str().unwrap().contains("Max-Age=0"));
    }
}
