//! Matching running processes to the policy's groups.

use std::collections::{BTreeMap, BTreeSet};

use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::process::{ProcessId, ProcessInfo, ProcessInspector};

/// Running processes of each group, counting only processes of restricted
/// accounts. Groups with no running process are absent.
pub(crate) fn running_groups(
    policy: &Policy,
    processes: &[ProcessInfo],
    inspector: &dyn ProcessInspector,
) -> BTreeMap<GroupId, BTreeSet<ProcessId>> {
    let mut running: BTreeMap<GroupId, BTreeSet<ProcessId>> = BTreeMap::new();
    let restricted = processes.iter().filter(|process| {
        process
            .user
            .as_ref()
            .is_some_and(|user| policy.restricts(user))
    });
    for process in restricted {
        for group in policy.groups() {
            if group
                .rules
                .iter()
                .any(|rule| rule.matches(process, inspector))
            {
                running.entry(group.id).or_default().insert(process.id);
            }
        }
    }
    running
}

#[cfg(test)]
mod tests;
