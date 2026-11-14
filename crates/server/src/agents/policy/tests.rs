use jiff::tz::TimeZone;
use parentcontrol_core::calendar::Calendar;
use parentcontrol_core::policy::Adjustment;
use parentcontrol_core::process::OsUser;
use parentcontrol_core::protocol::DevicePlatform;
use parentcontrol_core::units::MinuteDelta;
use parentcontrol_core::usage::{UsageRecord, UsageRecordId};
use uuid::Uuid;

use super::*;
use crate::db::Db;

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

/// 17:00 in Moscow on Saturday 2026-11-14.
const NOW: &str = "2026-11-14T14:00:00Z";

fn record(id: u128, group: parentcontrol_core::ids::GroupId, seconds: u64) -> UsageRecord {
    UsageRecord {
        id: UsageRecordId::new(Uuid::from_u128(id)),
        group,
        day: BudgetDay::new(jiff::civil::date(2026, 11, 14)),
        start: at("2026-11-14T13:00:00Z"),
        duration: Duration::from_secs(seconds),
        offline: false,
    }
}

#[tokio::test]
async fn policy_combines_bindings_groups_adjustments_and_other_devices_usage() {
    let db = Db::in_memory().unwrap();
    let policy = db
        .call(|c| {
            let calendar = Calendar::new(
                TimeZone::get("Europe/Moscow").unwrap(),
                "04:00".parse().unwrap(),
            );
            settings::set_calendar(c, &calendar)?;
            let games = groups::create(c, DEFAULT_PROFILE, "Игры")?;
            let today = calendar.locate(at(NOW)).day;
            groups::add_adjustment(
                c,
                games,
                Adjustment {
                    day: today,
                    minutes: MinuteDelta::new(15),
                },
            )?;

            let pc = devices::create(c, "ПК", "a", at("2026-11-15T00:00:00Z"))?;
            let laptop = devices::create(c, "Ноутбук", "b", at("2026-11-15T00:00:00Z"))?;
            devices::enroll(c, "a", "ta", DevicePlatform::Linux, at(NOW))?;
            devices::enroll(c, "b", "tb", DevicePlatform::Linux, at(NOW))?;
            devices::bind(c, pc, &OsUser::new("kid"), DEFAULT_PROFILE)?;
            usage::merge(c, pc, &[record(1, games, 600)])?;
            usage::merge(c, laptop, &[record(2, games, 300)])?;

            device_policy(c, pc, at(NOW))
        })
        .await
        .unwrap()
        .unwrap();

    assert!(policy.restricts(&OsUser::new("kid")));
    let group = &policy.groups()[0];
    assert_eq!(group.name, "Игры");
    assert_eq!(group.adjustments.len(), 1);
    let today = policy.calendar().locate(at(NOW)).day;
    assert_eq!(
        group.remote_usage(today),
        Duration::from_secs(300),
        "only the laptop's usage is remote"
    );
}

#[tokio::test]
async fn deleted_device_has_no_policy() {
    let db = Db::in_memory().unwrap();
    let policy = db
        .call(|c| {
            let calendar = Calendar::new(
                TimeZone::get("Europe/Moscow").unwrap(),
                "04:00".parse().unwrap(),
            );
            settings::set_calendar(c, &calendar)?;
            device_policy(c, DeviceId::new(42), at(NOW))
        })
        .await
        .unwrap();
    assert!(policy.is_none());
}
