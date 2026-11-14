//! The agent protocol and the panel against a real server.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failing assumption should fail the test"
)]

mod common;

use std::time::Duration;

use common::{Agent, TestServer};
use jiff::Timestamp;
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::process::OsUser;
use parentcontrol_core::protocol::{
    AgentMessage, AgentToken, DevicePlatform, PROTOCOL_VERSION, Rejection, ServerMessage,
};
use parentcontrol_core::usage::{UsageRecord, UsageRecordId};
use uuid::Uuid;

const GAMES: GroupId = GroupId::new(1);

/// Creates a device in the panel and returns its enrollment code.
async fn new_device(server: &TestServer, cookie: &str, name: &str) -> String {
    let page = server
        .http("POST", "/devices", Some(cookie), &[("name", name)])
        .await;
    assert_eq!(page.status, 200);
    let start = page
        .body
        .find("code-big code\">")
        .expect("no code on the page")
        + "code-big code\">".len();
    page.body[start..start + 9].to_owned()
}

async fn enroll(server: &TestServer, code: &str) -> AgentToken {
    let mut agent = server.agent().await;
    agent
        .send(&AgentMessage::Enroll {
            code: code.to_owned(),
            platform: DevicePlatform::Linux,
        })
        .await;
    match agent.receive().await {
        ServerMessage::Enrolled { token, .. } => token,
        other => panic!("not enrolled: {other:?}"),
    }
}

async fn connect(server: &TestServer, token: &AgentToken) -> Agent {
    let mut agent = server.agent().await;
    agent
        .send(&AgentMessage::Hello {
            token: token.clone(),
            protocol: PROTOCOL_VERSION,
            platform: DevicePlatform::Linux,
            local_users: vec![OsUser::new("kid"), OsUser::new("parent")],
        })
        .await;
    agent
}

fn record(id: u128, start: Timestamp, minutes: u64) -> UsageRecord {
    UsageRecord {
        id: UsageRecordId::new(Uuid::from_u128(id)),
        group: GAMES,
        day: parentcontrol_core::calendar::BudgetDay::new(
            start
                .to_zoned(jiff::tz::TimeZone::get("Europe/Moscow").unwrap())
                .date(),
        ),
        start,
        duration: Duration::from_mins(minutes),
        offline: false,
    }
}

fn policy_of(message: &ServerMessage) -> Option<&parentcontrol_core::policy::Policy> {
    match message {
        ServerMessage::Policy { policy, .. } => Some(policy),
        _ => None,
    }
}

#[tokio::test]
async fn panel_changes_reach_agents_and_usage_reaches_the_panel() {
    let server = TestServer::start().await;
    let cookie = server.login().await;
    assert_eq!(
        server
            .http("POST", "/groups", Some(&cookie), &[("name", "Игры")])
            .await
            .status,
        303
    );
    let added = server
        .http(
            "POST",
            "/groups/1/rules",
            Some(&cookie),
            &[("kind", "minecraft_java")],
        )
        .await;
    assert_eq!(added.status, 303);

    let token = enroll(&server, &new_device(&server, &cookie, "ПК").await).await;
    let mut pc = connect(&server, &token).await;
    let first = pc.receive().await;
    let policy = policy_of(&first).expect("policy comes first");
    assert!(
        !policy.restricts(&OsUser::new("kid")),
        "nothing is bound yet"
    );
    assert_eq!(policy.groups()[0].name, "Игры");

    // Binding the child's account is pushed at once.
    let devices = server.http("GET", "/devices", Some(&cookie), &[]).await;
    assert!(
        devices.body.contains("kid"),
        "reported accounts are offered"
    );
    server
        .http("POST", "/devices/1/bind", Some(&cookie), &[("user", "kid")])
        .await;
    pc.receive_until(|m| policy_of(m).is_some_and(|p| p.restricts(&OsUser::new("kid"))))
        .await;

    // Usage is acknowledged idempotently.
    let now = Timestamp::now();
    let played = record(1, now.checked_sub(Duration::from_mins(10)).unwrap(), 10);
    for _ in 0..2 {
        pc.send(&AgentMessage::Report {
            records: vec![played.clone()],
            active_groups: vec![GAMES],
        })
        .await;
        let ack = pc
            .receive_until(|m| matches!(m, ServerMessage::Acknowledged { .. }))
            .await;
        let ServerMessage::Acknowledged { records } = ack else {
            unreachable!()
        };
        assert_eq!(records[0].duration, Duration::from_mins(10));
    }
    let home = server.http("GET", "/", Some(&cookie), &[]).await;
    assert!(
        home.body.contains("использовано 10 мин"),
        "usage shows on the main page"
    );
    assert!(home.body.contains("Играют сейчас"));

    // A bonus is pushed at once.
    server
        .http(
            "POST",
            "/groups/1/bonus",
            Some(&cookie),
            &[("minutes", "15")],
        )
        .await;
    pc.receive_until(|m| policy_of(m).is_some_and(|p| !p.groups()[0].bonuses.is_empty()))
        .await;

    // A second device's usage counts against the first device's budget.
    let laptop_token = enroll(&server, &new_device(&server, &cookie, "Ноутбук").await).await;
    let mut laptop = connect(&server, &laptop_token).await;
    laptop.receive().await;
    let offline = UsageRecord {
        offline: true,
        ..record(2, now.checked_sub(Duration::from_mins(90)).unwrap(), 20)
    };
    laptop
        .send(&AgentMessage::Report {
            records: vec![offline],
            active_groups: vec![],
        })
        .await;
    let pushed = pc
        .receive_until(|m| {
            policy_of(m).is_some_and(|p| {
                let day = p.calendar().locate(Timestamp::now()).day;
                p.groups()[0].remote_usage(day) == Duration::from_mins(20)
            })
        })
        .await;
    assert!(policy_of(&pushed).is_some());

    let history = server.http("GET", "/history", Some(&cookie), &[]).await;
    assert!(
        history.body.contains("офлайн"),
        "late usage is marked offline"
    );
    assert!(
        history.body.contains("Бонус +15 мин: Игры"),
        "the parent's actions are logged"
    );
}

#[tokio::test]
async fn agents_with_bad_credentials_are_rejected() {
    let server = TestServer::start().await;

    let mut stranger = server.agent().await;
    stranger
        .send(&AgentMessage::Enroll {
            code: "AAAA-BBBB".to_owned(),
            platform: DevicePlatform::Windows,
        })
        .await;
    assert_eq!(
        stranger.receive().await,
        ServerMessage::Rejected {
            reason: Rejection::InvalidCode
        }
    );

    let mut forger = connect(&server, &AgentToken::new("forged".to_owned())).await;
    assert_eq!(
        forger.receive().await,
        ServerMessage::Rejected {
            reason: Rejection::InvalidToken
        }
    );
}

#[tokio::test]
async fn deleting_a_device_disconnects_its_agent() {
    let server = TestServer::start().await;
    let cookie = server.login().await;
    let token = enroll(&server, &new_device(&server, &cookie, "ПК").await).await;
    let mut pc = connect(&server, &token).await;
    pc.receive().await;

    server
        .http("POST", "/devices/1/delete", Some(&cookie), &[])
        .await;
    let rejected = pc
        .receive_until(|m| matches!(m, ServerMessage::Rejected { .. }))
        .await;
    assert_eq!(
        rejected,
        ServerMessage::Rejected {
            reason: Rejection::InvalidToken
        }
    );
}

#[tokio::test]
async fn panel_requires_login() {
    let server = TestServer::start().await;
    let page = server.http("GET", "/", None, &[]).await;
    assert_eq!(page.status, 303);
    assert_eq!(page.header("location"), Some("/login"));

    let wrong = server
        .http("POST", "/login", None, &[("password", "wrong password")])
        .await;
    assert_eq!(wrong.header("location"), Some("/login?failed=true"));
    assert_eq!(wrong.header("set-cookie"), None);

    let forged = server
        .http("GET", "/", Some("parentcontrol_session=00ff"), &[])
        .await;
    assert_eq!(forged.status, 303);
}
