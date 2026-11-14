use super::*;
use crate::test_support::{day_start, group, time};

fn calendar() -> Calendar {
    Calendar::new(TimeZone::get("Europe/Moscow").unwrap(), day_start())
}

const SNAPSHOT: &str = r#"{
    "time_zone": "Europe/Moscow",
    "day_start": "04:00",
    "accounts": ["kid"],
    "groups": [{
        "id": 1,
        "name": "Игры",
        "rules": [{"builtin": "minecraft_java"}, {"executable": "tetris"}],
        "schedule": {
            "monday":    {"quota": 60, "windows": [{"start": "16:00", "end": "20:00"}]},
            "tuesday":   {"quota": 60, "windows": [{"start": "16:00", "end": "20:00"}]},
            "wednesday": {"quota": 60, "windows": [{"start": "16:00", "end": "20:00"}]},
            "thursday":  {"quota": 60, "windows": [{"start": "16:00", "end": "20:00"}]},
            "friday":    {"quota": 90, "windows": [{"start": "16:00", "end": "22:00"}]},
            "saturday":  {"quota": 120},
            "sunday":    {"quota": 120}
        }
    }]
}"#;

#[test]
fn parses_a_hand_written_snapshot() {
    let policy: Policy = serde_json::from_str(SNAPSHOT).unwrap();
    assert!(policy.restricts(&OsUser::new("kid")));
    assert!(!policy.restricts(&OsUser::new("parent")));
    assert_eq!(policy.calendar().day_start(), time("04:00"));
    let group = &policy.groups()[0];
    assert_eq!(group.rules.len(), 2);
    assert!(!group.blocked);
    assert_eq!(group.schedule.friday.quota(), Minutes::new(90));
}

#[test]
fn round_trips_through_json() {
    let policy: Policy = serde_json::from_str(SNAPSHOT).unwrap();
    let again: Policy = serde_json::from_str(&serde_json::to_string(&policy).unwrap()).unwrap();
    assert_eq!(again, policy);
}

#[test]
fn without_accounts_nothing_is_restricted() {
    let policy = Policy::new(calendar(), BTreeSet::new(), vec![group(60, &[])]).unwrap();
    assert!(!policy.restricts(&OsUser::new("kid")));
}

#[test]
fn window_crossing_day_start_is_rejected() {
    let error = Policy::new(
        calendar(),
        BTreeSet::new(),
        vec![group(60, &[("03:00", "05:00")])],
    )
    .unwrap_err();
    assert!(matches!(error, PolicyError::WindowCrossesDayStart { .. }));
}

#[test]
fn duplicate_groups_are_rejected() {
    let error = Policy::new(
        calendar(),
        BTreeSet::new(),
        vec![group(60, &[]), group(30, &[])],
    )
    .unwrap_err();
    assert_eq!(error, PolicyError::DuplicateGroup(GroupId::new(1)));
}

#[test]
fn unknown_time_zone_is_rejected() {
    let snapshot = SNAPSHOT.replace("Europe/Moscow", "Mars/Olympus");
    assert!(serde_json::from_str::<Policy>(&snapshot).is_err());
}
