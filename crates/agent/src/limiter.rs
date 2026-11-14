//! One scan step: find running groups, account usage, decide, act.

use std::collections::HashMap;
use std::time::Duration;

use parentcontrol_core::budget::{self, GroupStatus};
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::clock::Clock;
use parentcontrol_core::enforcement::{Action, Enforcer, Notice, Observation, Termination};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::process::{OsUser, ProcessId};
use parentcontrol_core::protocol::Acknowledgement;
use parentcontrol_core::usage::{self, Journal, Tick, UsageRecordId};
use tracing::warn;
use uuid::Uuid;

use crate::platform::Platform;
use crate::sync::Report;
use crate::{actions, scan};

/// Applies a policy to the processes of this machine.
#[derive(Debug)]
pub(crate) struct Limiter<P, C> {
    platform: P,
    clock: C,
    journal: Journal,
    enforcer: Enforcer,
    /// The interval between steps; longer gaps are not credited as usage.
    interval: Duration,
    /// Groups running at the last step.
    active: Vec<GroupId>,
    /// Every group's status at the last step.
    statuses: Vec<GroupStatus>,
}

/// What a step leaves for others to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// Messages for the child.
    pub(crate) notices: Vec<Notice>,
    /// Programs whose windows the tray of their owner must close, because
    /// the platform cannot close them softly by itself.
    pub(crate) soft_close: Vec<(OsUser, ProcessId)>,
}

impl<P: Platform, C: Clock> Limiter<P, C> {
    /// A limiter continuing the usage recorded in `journal`.
    pub(crate) fn new(platform: P, clock: C, journal: Journal, interval: Duration) -> Self {
        Self {
            platform,
            clock,
            journal,
            enforcer: Enforcer::default(),
            interval,
            active: Vec::new(),
            statuses: Vec::new(),
        }
    }

    /// The usage recorded so far.
    pub(crate) const fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Every group's status at the last step.
    pub(crate) fn statuses(&self) -> &[GroupStatus] {
        &self.statuses
    }

    /// What to tell the server: unconfirmed usage and what runs now.
    pub(crate) fn report(&self) -> Report {
        Report {
            records: self.journal.unsynced().cloned().collect(),
            active_groups: self.active.clone(),
        }
    }

    /// Notes what the server has stored and forgets records it no longer
    /// needs locally: confirmed ones from before yesterday.
    pub(crate) fn acknowledge(&mut self, acknowledgements: &[Acknowledgement], today: BudgetDay) {
        for acknowledgement in acknowledgements {
            self.journal
                .acknowledge(acknowledgement.id, acknowledgement.duration);
        }
        if let Ok(yesterday) = today.date().yesterday() {
            self.journal.prune_before(BudgetDay::new(yesterday));
        }
    }

    /// The clock the limiter decides by.
    pub(crate) const fn clock(&self) -> &C {
        &self.clock
    }

    /// Runs one step, `elapsed` after the previous one; `online` tells
    /// whether the server is reachable, to mark usage counted without it.
    pub(crate) fn step(&mut self, policy: &Policy, elapsed: Duration, online: bool) -> Outcome {
        let processes = match self.platform.processes() {
            Ok(processes) => processes,
            Err(error) => {
                warn!(%error, "cannot list processes");
                return Outcome::default();
            }
        };
        let owners: HashMap<ProcessId, &OsUser> = processes
            .iter()
            .filter_map(|process| Some((process.id, process.user.as_ref()?)))
            .collect();
        let mut running = scan::running_groups(policy, &processes, &self.platform);

        let now = self.clock.now();
        let position = policy.calendar().locate(now);
        self.active = running.keys().copied().collect();
        let tick = Tick {
            now,
            day: position.day,
            credited: usage::credit(elapsed, self.interval),
            online,
        };
        self.journal
            .record(&self.active, tick, || UsageRecordId::new(Uuid::new_v4()));

        let day_start = policy.calendar().day_start();
        let observations: Vec<Observation> = policy
            .groups()
            .iter()
            .map(|group| {
                let local_usage = self.journal.usage(group.id, position.day);
                Observation {
                    status: budget::evaluate(group, day_start, position, local_usage),
                    processes: running.remove(&group.id).unwrap_or_default(),
                }
            })
            .collect();
        self.statuses = observations
            .iter()
            .map(|observation| observation.status)
            .collect();

        let mut outcome = Outcome::default();
        for action in self.enforcer.step(&observations, elapsed) {
            match action {
                Action::Terminate {
                    process,
                    mode: Termination::Graceful,
                } if !self.platform.closes_softly() => {
                    if let Some(owner) = owners.get(&process) {
                        outcome.soft_close.push(((*owner).clone(), process));
                    }
                }
                Action::Terminate { process, mode } => {
                    actions::terminate(&self.platform, process, mode);
                }
                Action::Notify(notice) => {
                    actions::log(policy, notice);
                    outcome.notices.push(notice);
                }
            }
        }
        outcome
    }
}

#[cfg(test)]
mod tests;
