use proptest::prelude::*;

use super::*;
use crate::test_support::saturday;

const A: GroupId = GroupId::new(1);
const B: GroupId = GroupId::new(2);

fn ids() -> impl FnMut() -> UsageRecordId {
    let mut next = 0_u128;
    move || {
        next += 1;
        UsageRecordId::new(Uuid::from_u128(next))
    }
}

fn tick(second: i64, credited: u64) -> Tick {
    Tick {
        now: Timestamp::from_second(1_800_000_000 + second).unwrap(),
        day: saturday(),
        credited: Duration::from_secs(credited),
        online: true,
    }
}

#[test]
fn credit_is_capped_at_two_intervals() {
    let interval = Duration::from_secs(5);
    assert_eq!(
        credit(Duration::from_secs(4), interval),
        Duration::from_secs(4)
    );
    assert_eq!(
        credit(Duration::from_secs(3600), interval),
        Duration::from_secs(10)
    );
}

#[test]
fn continuous_use_extends_one_record() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    for second in [5, 10, 15] {
        journal.record(&[A], tick(second, 5), &mut ids);
    }
    let records: Vec<_> = journal.records().collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].duration, Duration::from_secs(15));
    assert_eq!(records[0].start, tick(0, 0).now);
}

#[test]
fn a_pause_starts_a_new_record() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    journal.record(&[], tick(10, 5), &mut ids);
    journal.record(&[A], tick(15, 5), &mut ids);
    assert_eq!(journal.records().count(), 2);
    assert_eq!(journal.usage(A, saturday()), Duration::from_secs(10));
}

#[test]
fn new_budget_day_starts_a_new_record() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    let sunday = BudgetDay::new(saturday().date().tomorrow().unwrap());
    journal.record(
        &[A],
        Tick {
            day: sunday,
            ..tick(10, 5)
        },
        &mut ids,
    );
    assert_eq!(journal.usage(A, saturday()), Duration::from_secs(5));
    assert_eq!(journal.usage(A, sunday), Duration::from_secs(5));
}

#[test]
fn groups_are_counted_independently() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A, B], tick(5, 5), &mut ids);
    journal.record(&[A], tick(10, 5), &mut ids);
    assert_eq!(journal.usage(A, saturday()), Duration::from_secs(10));
    assert_eq!(journal.usage(B, saturday()), Duration::from_secs(5));
}

#[test]
fn zero_credit_creates_nothing() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 0), &mut ids);
    assert_eq!(journal.records().count(), 0);
}

#[test]
fn survives_a_round_trip_through_json_as_closed_records() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    let mut restored: Journal =
        serde_json::from_str(&serde_json::to_string(&journal).unwrap()).unwrap();
    assert!(restored.records().eq(journal.records()));
    restored.record(&[A], tick(10, 5), &mut ids);
    assert_eq!(
        restored.records().count(),
        2,
        "a restart opens a new record"
    );
    assert_eq!(restored.usage(A, saturday()), Duration::from_secs(10));
}

proptest! {
    #[test]
    fn usage_equals_total_credited_time(ticks in prop::collection::vec((any::<bool>(), 0_u64..20), 0..60)) {
        let (mut journal, mut ids) = (Journal::default(), ids());
        let mut expected = Duration::ZERO;
        for (index, (active, credited)) in ticks.into_iter().enumerate() {
            let groups: &[GroupId] = if active { &[A] } else { &[] };
            journal.record(groups, tick(i64::try_from(index).unwrap() * 5, credited), &mut ids);
            if active {
                expected += Duration::from_secs(credited);
            }
        }
        prop_assert_eq!(journal.usage(A, saturday()), expected);
    }
}

#[test]
fn unsynced_lists_records_until_fully_acknowledged() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    let id = journal.records().next().unwrap().id;
    journal.acknowledge(id, Duration::from_secs(5));
    assert_eq!(journal.unsynced().count(), 0);

    journal.record(&[A], tick(10, 5), &mut ids);
    assert_eq!(
        journal.unsynced().count(),
        1,
        "a grown record needs another upload"
    );
}

#[test]
fn late_shorter_acknowledgement_does_not_undo_a_newer_one() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    journal.record(&[A], tick(10, 5), &mut ids);
    let id = journal.records().next().unwrap().id;
    journal.acknowledge(id, Duration::from_secs(10));
    journal.acknowledge(id, Duration::from_secs(5));
    assert_eq!(journal.unsynced().count(), 0);
}

#[test]
fn pruning_keeps_recent_and_unsynced_records() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    let sunday = BudgetDay::new(saturday().date().tomorrow().unwrap());
    journal.record(&[A], tick(5, 5), &mut ids);
    journal.record(&[B], tick(10, 5), &mut ids);
    journal.record(
        &[A],
        Tick {
            day: sunday,
            ..tick(15, 5)
        },
        &mut ids,
    );
    let first = journal.records().next().unwrap().id;
    journal.acknowledge(first, Duration::from_secs(5));

    journal.prune_before(sunday);

    let groups: Vec<GroupId> = journal.records().map(|record| record.group).collect();
    assert_eq!(groups, [B, A], "synced Saturday record of A is gone");
}

#[test]
fn time_counted_offline_marks_the_record() {
    let (mut journal, mut ids) = (Journal::default(), ids());
    journal.record(&[A], tick(5, 5), &mut ids);
    assert!(!journal.records().next().unwrap().offline);
    journal.record(
        &[A],
        Tick {
            online: false,
            ..tick(10, 5)
        },
        &mut ids,
    );
    assert!(journal.records().next().unwrap().offline);
}
