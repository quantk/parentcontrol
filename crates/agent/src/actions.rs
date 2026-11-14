//! Carrying out the enforcer's decisions.

use parentcontrol_core::enforcement::{Action, Notice};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
use tracing::{info, warn};

use crate::platform::Platform;

/// Performs `action`. Failures are logged: the next tick decides again, so
/// a process that survived is dealt with then.
pub(crate) fn perform(platform: &impl Platform, policy: &Policy, action: Action) {
    match action {
        Action::Terminate { process, mode } => match platform.terminate(process, mode) {
            Ok(()) => info!(%process, ?mode, "terminating process"),
            Err(error) => warn!(%process, ?mode, %error, "cannot terminate process"),
        },
        Action::Notify(notice) => announce(policy, notice),
    }
}

/// Records a notice for the child in the log.
fn announce(policy: &Policy, notice: Notice) {
    match notice {
        Notice::Warning {
            group,
            left,
            reason,
        } => {
            info!(group = name(policy, group), %left, ?reason, "time is running out");
        }
        Notice::Closing { group, reason } => {
            info!(
                group = name(policy, group),
                ?reason,
                "closing running programs"
            );
        }
        Notice::LaunchDenied { group, reason } => {
            info!(group = name(policy, group), ?reason, "launch denied");
        }
        Notice::CloseCancelled { group } => {
            info!(
                group = name(policy, group),
                "allowed again; forced close cancelled"
            );
        }
    }
}

fn name(policy: &Policy, group: GroupId) -> &str {
    policy
        .groups()
        .iter()
        .find(|candidate| candidate.id == group)
        .map_or("?", |group| group.name.as_str())
}
