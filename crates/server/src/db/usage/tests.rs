use jiff::civil::date;
use parentcontrol_core::usage::UsageRecordId;
use uuid::Uuid;

use super::*;
use crate::db::Db;

const GAMES: GroupId = GroupId::new(1);
const PC: DeviceId = DeviceId::new(1);
const LAPTOP: DeviceId = DeviceId::new(2);

fn day_14() -> BudgetDay {
    BudgetDay::new(date(2026, 11, 14))
}

fn record(id: u128, start: &str, seconds: u64) -> UsageRecord {
    UsageRecord {
        id: UsageRecordId::new(Uuid::from_u128(id)),
        group: GAMES,
        day: day_14(),
        start: start.parse().unwrap(),
        duration: Duration::from_secs(seconds),
        offline: false,
    }
}

#[tokio::test]
async fn merging_is_idempotent_and_keeps_the_longest_version() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let first = record(1, "2026-11-14T10:00:00Z", 60);
        let longer = record(1, "2026-11-14T10:00:00Z", 120);
        merge(c, PC, std::slice::from_ref(&first))?;
        merge(c, PC, std::slice::from_ref(&longer))?;
        let acks = merge(c, PC, &[first])?;
        assert_eq!(
            acks[0].duration,
            Duration::from_secs(120),
            "a stale resend does not shrink it"
        );

        let totals = totals(c, day_14(), day_14())?;
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].used, Duration::from_secs(120));
        assert!(!totals[0].offline);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn offline_flag_sticks_once_set() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        let offline = UsageRecord {
            offline: true,
            ..record(1, "2026-11-14T10:00:00Z", 60)
        };
        merge(c, PC, &[offline])?;
        merge(c, PC, &[record(1, "2026-11-14T10:00:00Z", 120)])?;
        assert!(sessions(c, day_14())?[0].offline);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_device_cannot_overwrite_another_devices_record() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        merge(c, PC, &[record(1, "2026-11-14T10:00:00Z", 60)])?;
        let acks = merge(c, LAPTOP, &[record(1, "2026-11-14T10:00:00Z", 6000)])?;
        assert_eq!(acks, []);
        assert_eq!(sessions(c, day_14())?[0].duration, Duration::from_secs(60));
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn totals_are_split_by_device() {
    let db = Db::in_memory().unwrap();
    db.call(|c| {
        merge(
            c,
            PC,
            &[
                record(1, "2026-11-14T10:00:00Z", 60),
                record(2, "2026-11-14T11:00:00Z", 30),
            ],
        )?;
        merge(c, LAPTOP, &[record(3, "2026-11-14T11:00:00Z", 30)])?;
        let totals = totals(c, day_14(), day_14())?;
        let used: Vec<(DeviceId, u64)> = totals
            .iter()
            .map(|t| (t.device, t.used.as_secs()))
            .collect();
        assert_eq!(used, [(PC, 90), (LAPTOP, 30)]);
        Ok(())
    })
    .await
    .unwrap();
}
