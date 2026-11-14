use super::*;
use crate::test_support::{FakePlatform, minecraft, policy};

#[test]
fn groups_list_only_restricted_matching_processes() {
    let mut other = minecraft(3, "kid");
    other.command_line = vec!["/usr/bin/firefox".to_owned()];
    let processes = [minecraft(1, "kid"), minecraft(2, "parent"), other];

    let running = running_groups(&policy(60), &processes, &FakePlatform::default());

    let games: Vec<u32> = running[&GroupId::new(1)]
        .iter()
        .map(|id| id.pid())
        .collect();
    assert_eq!(games, [1]);
}

#[test]
fn processes_without_a_known_owner_are_ignored() {
    let mut orphan = minecraft(1, "kid");
    orphan.user = None;
    assert!(running_groups(&policy(60), &[orphan], &FakePlatform::default()).is_empty());
}
