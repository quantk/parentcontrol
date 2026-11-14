use std::fs::File;
use std::time::{Duration, SystemTime};

use super::*;
use crate::test_support::policy;

/// Writes `contents` and moves the modification time forward, so a change
/// is noticed even on file systems with coarse timestamps.
fn write(path: &std::path::Path, contents: &str, age: u64) {
    fs::write(path, contents).unwrap();
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + age);
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
fn documented_example_is_valid() {
    let example = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/policy.example.json"
    );
    let mut file = PolicyFile::new(PathBuf::from(example));
    assert!(file.refresh().is_some());
}

#[test]
fn missing_file_means_no_policy() {
    let dir = tempfile::tempdir().unwrap();
    let mut file = PolicyFile::new(dir.path().join("policy.json"));
    assert_eq!(file.refresh(), None);
}

#[test]
fn loads_and_reloads_on_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let mut file = PolicyFile::new(path.clone());

    write(&path, &serde_json::to_string(&policy(60)).unwrap(), 1);
    assert_eq!(file.refresh(), Some(&policy(60)));

    write(&path, &serde_json::to_string(&policy(30)).unwrap(), 2);
    assert_eq!(file.refresh(), Some(&policy(30)));
}

#[test]
fn invalid_edit_keeps_previous_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let mut file = PolicyFile::new(path.clone());
    write(&path, &serde_json::to_string(&policy(60)).unwrap(), 1);
    file.refresh();

    write(&path, "{ broken", 2);
    assert_eq!(file.refresh(), Some(&policy(60)));
}

#[test]
fn deleted_file_lifts_the_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let mut file = PolicyFile::new(path.clone());
    write(&path, &serde_json::to_string(&policy(60)).unwrap(), 1);
    file.refresh();

    fs::remove_file(&path).unwrap();
    assert_eq!(file.refresh(), None);
}

#[test]
fn replaced_policy_is_in_force_and_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.json");
    let mut file = PolicyFile::new(path.clone());
    file.replace(policy(45)).unwrap();
    assert_eq!(file.refresh(), Some(&policy(45)));

    let mut reopened = PolicyFile::new(path);
    assert_eq!(reopened.refresh(), Some(&policy(45)));
}
