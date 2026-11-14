//! Carrying out the enforcer's decisions on this machine.

use parentcontrol_core::enforcement::{Notice, Termination};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::process::ProcessId;
use tracing::{info, warn};

use crate::platform::Platform;

/// Ends `process`. Failures are logged: the next step decides again, so
/// a process that survived is dealt with then.
pub(crate) fn terminate(platform: &impl Platform, process: ProcessId, mode: Termination) {
    match platform.terminate(process, mode) {
        Ok(()) => info!(%process, ?mode, "terminating process"),
        Err(error) => warn!(%process, ?mode, %error, "cannot terminate process"),
    }
}

/// Records a notice for the child in the log.
pub(crate) fn log(policy: &Policy, notice: Notice) {
    match notice {
        Notice::Warning {
            group,
            left,
            reason,
        } => {
            info!(group = group_name(policy, group), %left, ?reason, "time is running out");
        }
        Notice::Closing { group, reason } => {
            info!(
                group = group_name(policy, group),
                ?reason,
                "closing running programs"
            );
        }
        Notice::LaunchDenied { group, reason } => {
            info!(group = group_name(policy, group), ?reason, "launch denied");
        }
        Notice::CloseCancelled { group } => {
            info!(
                group = group_name(policy, group),
                "allowed again; forced close cancelled"
            );
        }
    }
}

/// The name of `group`, or `?` if the policy no longer has it.
pub(crate) fn group_name(policy: &Policy, group: GroupId) -> &str {
    policy
        .groups()
        .iter()
        .find(|candidate| candidate.id == group)
        .map_or("?", |group| group.name.as_str())
}
