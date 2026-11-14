//! The panel's pages, messages, headers and assets against a real server.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failing assumption should fail the test"
)]

mod common;

use common::TestServer;

/// Every section renders for the logged-in parent.
#[tokio::test]
async fn every_page_renders() {
    let server = TestServer::start().await;
    let cookie = server.login().await;
    server
        .http("POST", "/groups", Some(&cookie), &[("name", "Игры")])
        .await;
    for path in [
        "/",
        "/groups",
        "/schedule",
        "/devices",
        "/history",
        "/settings",
    ] {
        let page = server.http("GET", path, Some(&cookie), &[]).await;
        assert_eq!(page.status, 200, "{path}");
        assert!(
            page.body.contains("Игры") || path == "/devices" || path == "/settings",
            "{path}"
        );
    }
}

/// A rejected input is explained once, on the page the form came from.
#[tokio::test]
async fn messages_are_shown_once() {
    let server = TestServer::start().await;
    let session = server.login().await;
    let rejected = server
        .http("POST", "/groups", Some(&session), &[("name", "  ")])
        .await;
    assert_eq!(rejected.status, 303);
    assert_eq!(rejected.header("location"), Some("/groups"));
    let flash = rejected
        .header("set-cookie")
        .and_then(|cookie| cookie.split(';').next())
        .expect("the message travels in a cookie")
        .to_owned();

    let cookies = format!("{session}; {flash}");
    let page = server.http("GET", "/groups", Some(&cookies), &[]).await;
    assert!(page.body.contains("Название не может быть пустым."));
    let cleared = page.header("set-cookie").expect("the message is deleted");
    assert!(cleared.starts_with("parentcontrol_flash=;"), "{cleared}");

    let again = server.http("GET", "/groups", Some(&session), &[]).await;
    assert!(!again.body.contains("Название не может быть пустым."));
}

/// Changes are confirmed with what was done.
#[tokio::test]
async fn changes_are_confirmed() {
    let server = TestServer::start().await;
    let session = server.login().await;
    let created = server
        .http("POST", "/groups", Some(&session), &[("name", "Игры")])
        .await;
    let flash = created
        .header("set-cookie")
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let page = server
        .http("GET", "/groups", Some(&format!("{session}; {flash}")), &[])
        .await;
    assert!(page.body.contains("flash-done"));
    assert!(page.body.contains("Создана группа: Игры"));
}

#[tokio::test]
async fn pages_forbid_foreign_scripts_and_framing() {
    let server = TestServer::start().await;
    let page = server.http("GET", "/login", None, &[]).await;
    let policy = page.header("content-security-policy").unwrap();
    assert!(policy.contains("script-src 'self'"), "{policy}");
    assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
    assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
}

#[tokio::test]
async fn assets_are_revalidated_by_etag() {
    let server = TestServer::start().await;
    for path in [
        "/static/style.css",
        "/static/panel.js",
        "/static/htmx.min.js",
        "/static/icons.svg",
        "/static/favicon.svg",
    ] {
        let asset = server.http("GET", path, None, &[]).await;
        assert_eq!(asset.status, 200, "{path}");
        assert_eq!(asset.header("cache-control"), Some("no-cache"));
        let etag = asset.header("etag").expect("assets carry an etag");
        let revalidated = server
            .http_with_headers("GET", path, &[("If-None-Match", etag)])
            .await;
        assert_eq!(revalidated.status, 304, "{path}");
    }
}
