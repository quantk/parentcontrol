//! Response headers that harden every response.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::middleware::Next;
use axum::response::Response;

/// Scripts, styles and requests only from the panel itself, so injected
/// markup can neither run code nor send data elsewhere. Inline `style`
/// attributes stay allowed: charts size their bars with them, and they
/// cannot run code.
const POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
     style-src-attr 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; \
     form-action 'self'; frame-ancestors 'none'; base-uri 'none'; object-src 'none'";

/// Middleware adding the headers.
pub(crate) async fn harden(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(POLICY));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}
