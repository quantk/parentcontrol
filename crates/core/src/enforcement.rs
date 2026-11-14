//! Turning group statuses into warnings and terminations.
//!
//! The [`Enforcer`] remembers what it saw in previous ticks, which it needs
//! to tell a game that ran out of time (closed softly, with a grace period
//! to save) from one launched while forbidden (terminated at once).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::budget::{Availability, DenyReason, GroupStatus, StopReason};
use crate::ids::GroupId;
use crate::process::ProcessId;
use crate::units::Minutes;

/// Time between the soft close and the forced termination.
pub const GRACE_PERIOD: Duration = Duration::from_secs(30);

/// How long before a stop the child is warned, largest first.
pub const WARNINGS: [Minutes; 3] = [Minutes::new(10), Minutes::new(5), Minutes::new(1)];

/// How to end a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Termination {
    /// Ask the program to quit so it can save (`SIGTERM`, `WM_CLOSE`).
    Graceful,
    /// End it immediately.
    Forced,
}

/// A message for the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Notice {
    /// The group will stop in `left`.
    Warning {
        /// The group concerned.
        group: GroupId,
        /// Time left, rounded up.
        left: Minutes,
        /// What will stop it.
        reason: StopReason,
    },
    /// Running programs of the group are being closed.
    Closing {
        /// The group concerned.
        group: GroupId,
        /// Why.
        reason: DenyReason,
    },
    /// A program of the group was started while that is not allowed.
    LaunchDenied {
        /// The group concerned.
        group: GroupId,
        /// Why.
        reason: DenyReason,
    },
    /// The group became allowed again during the grace period, so the
    /// forced termination is called off. A soft close already requested
    /// cannot be taken back.
    CloseCancelled {
        /// The group concerned.
        group: GroupId,
    },
}

/// Something the agent must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Show a notice to the child.
    Notify(Notice),
    /// End a process.
    Terminate {
        /// The process.
        process: ProcessId,
        /// How.
        mode: Termination,
    },
}

/// One group as observed in one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// The group's status now.
    pub status: GroupStatus,
    /// Its running processes owned by restricted accounts.
    pub processes: BTreeSet<ProcessId>,
}

/// Decides actions tick by tick.
#[derive(Debug, Default)]
pub struct Enforcer {
    groups: BTreeMap<GroupId, GroupState>,
}

#[derive(Debug, Default)]
struct GroupState {
    /// Processes seen in the previous tick.
    previous: BTreeSet<ProcessId>,
    /// Processes being terminated, with the grace time they have left.
    terminating: BTreeMap<ProcessId, Duration>,
    /// The smallest warning threshold already announced.
    warned: Option<Minutes>,
}

impl Enforcer {
    /// Processes one tick. `elapsed` is the real time since the previous
    /// tick and runs down grace periods.
    pub fn step(&mut self, observations: &[Observation], elapsed: Duration) -> Vec<Action> {
        self.groups
            .retain(|group, _| observations.iter().any(|o| o.status.group == *group));
        let mut actions = Vec::new();
        for observation in observations {
            let state = self.groups.entry(observation.status.group).or_default();
            match observation.status.availability {
                Availability::Denied(reason) => {
                    state.deny(observation, reason, elapsed, &mut actions);
                }
                Availability::Allowed { stop_in, reason } => {
                    state.allow(observation, stop_in, reason, &mut actions);
                }
            }
            state.previous.clone_from(&observation.processes);
        }
        actions
    }
}

impl GroupState {
    fn deny(
        &mut self,
        observation: &Observation,
        reason: DenyReason,
        elapsed: Duration,
        actions: &mut Vec<Action>,
    ) {
        let group = observation.status.group;
        let (mut closing, mut launched) = (false, false);
        self.terminating
            .retain(|process, _| observation.processes.contains(process));
        for &process in &observation.processes {
            let mode = if let Some(left) = self.terminating.get_mut(&process) {
                *left = left.saturating_sub(elapsed);
                if !left.is_zero() {
                    continue;
                }
                Termination::Forced
            } else if self.previous.contains(&process) {
                closing = true;
                self.terminating.insert(process, GRACE_PERIOD);
                Termination::Graceful
            } else {
                launched = true;
                self.terminating.insert(process, Duration::ZERO);
                Termination::Forced
            };
            actions.push(Action::Terminate { process, mode });
        }
        if closing {
            actions.push(Action::Notify(Notice::Closing { group, reason }));
        }
        if launched {
            actions.push(Action::Notify(Notice::LaunchDenied { group, reason }));
        }
    }

    fn allow(
        &mut self,
        observation: &Observation,
        stop_in: Duration,
        reason: StopReason,
        actions: &mut Vec<Action>,
    ) {
        let group = observation.status.group;
        if !self.terminating.is_empty() {
            self.terminating.clear();
            actions.push(Action::Notify(Notice::CloseCancelled { group }));
        }

        // The smallest threshold `stop_in` has reached, if any.
        let Some(threshold) = WARNINGS
            .into_iter()
            .rev()
            .find(|t| stop_in <= t.to_duration())
        else {
            self.warned = None;
            return;
        };
        match self.warned {
            // Already announced this threshold or a smaller one.
            Some(warned) if warned <= threshold => self.warned = Some(threshold),
            _ if observation.processes.is_empty() => {}
            _ => {
                self.warned = Some(threshold);
                let left = Minutes::ceil_from(stop_in);
                actions.push(Action::Notify(Notice::Warning {
                    group,
                    left,
                    reason,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests;
