use super::*;

#[test]
fn messages_are_tagged_json() {
    let message = AgentMessage::Report {
        records: Vec::new(),
        active_groups: vec![GroupId::new(3)],
    };
    assert_eq!(
        serde_json::to_string(&message).unwrap(),
        r#"{"type":"report","records":[],"active_groups":[3]}"#
    );
}

#[test]
fn hello_round_trips() {
    let message = AgentMessage::Hello {
        token: AgentToken::new("secret".to_owned()),
        protocol: PROTOCOL_VERSION,
        platform: DevicePlatform::Windows,
        local_users: vec![OsUser::new("kid")],
    };
    let json = serde_json::to_string(&message).unwrap();
    assert_eq!(
        serde_json::from_str::<AgentMessage>(&json).unwrap(),
        message
    );
}

#[test]
fn rejection_round_trips() {
    let message = ServerMessage::Rejected {
        reason: Rejection::InvalidToken,
    };
    let json = serde_json::to_string(&message).unwrap();
    assert_eq!(json, r#"{"type":"rejected","reason":"invalid_token"}"#);
    assert_eq!(
        serde_json::from_str::<ServerMessage>(&json).unwrap(),
        message
    );
}

#[test]
fn token_is_not_printed_in_debug_output() {
    let token = AgentToken::new("secret".to_owned());
    assert!(!format!("{token:?}").contains("secret"));
}

#[test]
fn platform_identifiers_round_trip() {
    for platform in [DevicePlatform::Linux, DevicePlatform::Windows] {
        assert_eq!(
            platform.as_str().parse::<DevicePlatform>().unwrap(),
            platform
        );
        let json = serde_json::to_string(&platform).unwrap();
        assert_eq!(json, format!("\"{}\"", platform.as_str()));
    }
    assert!("macos".parse::<DevicePlatform>().is_err());
}
