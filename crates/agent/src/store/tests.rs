use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::usage::{Tick, UsageRecordId};

use super::*;

fn journal_with_usage() -> Journal {
    let mut journal = Journal::default();
    let tick = Tick {
        now: "2026-11-14T14:00:00Z".parse().unwrap(),
        day: BudgetDay::new(jiff::civil::date(2026, 11, 14)),
        credited: std::time::Duration::from_secs(5),
        online: true,
    };
    journal.record(&[GroupId::new(1)], tick, || {
        UsageRecordId::new(uuid::Uuid::from_u128(1))
    });
    journal
}

#[test]
fn missing_journal_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("data")).unwrap();
    assert_eq!(store.load_journal().unwrap(), Journal::default());
}

#[test]
fn journal_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_owned()).unwrap();
    let journal = journal_with_usage();
    store.save_journal(&journal).unwrap();
    assert!(
        store
            .load_journal()
            .unwrap()
            .records()
            .eq(journal.records())
    );
}

#[test]
fn corrupt_journal_is_moved_aside() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_owned()).unwrap();
    fs::write(dir.path().join(JOURNAL_FILE), "{not json").unwrap();
    assert_eq!(store.load_journal().unwrap(), Journal::default());
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .any(|name| name.starts_with("journal.corrupt-")),
        "{names:?}"
    );
}
