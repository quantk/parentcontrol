//! Stylesheet, scripts and icons, compiled into the binary.
//!
//! Browsers revalidate every asset with its `ETag`, so a new server
//! version takes effect on the next page load, and an unchanged asset costs
//! only a `304 Not Modified`.

use std::fmt::Write as _;

use axum::Router;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// A file served under `/static/`.
struct Asset {
    path: &'static str,
    content_type: &'static str,
    body: &'static [u8],
}

const ASSETS: [Asset; 5] = [
    Asset {
        path: "/static/style.css",
        content_type: "text/css; charset=utf-8",
        body: include_bytes!("../../static/style.css"),
    },
    Asset {
        path: "/static/panel.js",
        content_type: "text/javascript; charset=utf-8",
        body: include_bytes!("../../static/panel.js"),
    },
    Asset {
        path: "/static/icons.svg",
        content_type: "image/svg+xml",
        body: include_bytes!("../../static/icons.svg"),
    },
    Asset {
        path: "/static/favicon.svg",
        content_type: "image/svg+xml",
        body: include_bytes!("../../static/favicon.svg"),
    },
    Asset {
        path: "/static/htmx.min.js",
        content_type: "text/javascript; charset=utf-8",
        body: include_bytes!("../../static/vendor/htmx.min.js"),
    },
];

/// Routes of every asset.
pub(super) fn routes() -> Router<AppState> {
    ASSETS.iter().fold(Router::new(), |router, asset| {
        let etag = etag(asset.body);
        let (content_type, body) = (asset.content_type, asset.body);
        router.route(
            asset.path,
            get(
                move |headers: HeaderMap| async move { serve(&headers, &etag, content_type, body) },
            ),
        )
    })
}

fn serve(
    headers: &HeaderMap,
    etag: &HeaderValue,
    content_type: &'static str,
    body: &'static [u8],
) -> Response {
    let cache = [
        (CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        (ETAG, etag.clone()),
    ];
    if headers.get(IF_NONE_MATCH) == Some(etag) {
        return (StatusCode::NOT_MODIFIED, cache).into_response();
    }
    (cache, [(CONTENT_TYPE, content_type)], body).into_response()
}

/// A strong `ETag`: the start of the content's SHA-256.
fn etag(body: &[u8]) -> HeaderValue {
    let digest = Sha256::digest(body);
    let hex = digest[..8].iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    // Quoted hex is always a valid header value.
    HeaderValue::from_str(&format!("\"{hex}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_assets_are_not_resent() {
        let tag = etag(b"body");
        let mut headers = HeaderMap::new();
        assert_eq!(
            serve(&headers, &tag, "text/css", b"body").status(),
            StatusCode::OK
        );
        headers.insert(IF_NONE_MATCH, tag.clone());
        assert_eq!(
            serve(&headers, &tag, "text/css", b"body").status(),
            StatusCode::NOT_MODIFIED
        );
    }

    #[test]
    fn tags_follow_the_content() {
        assert_eq!(etag(b"a"), etag(b"a"));
        assert_ne!(etag(b"a"), etag(b"b"));
    }
}
