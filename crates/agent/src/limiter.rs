//! One scan step: find running groups, account usage, decide, act.

use std::time::Duration;

use parentcontrol_core::budget;
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::clock::Clock;
use parentcontrol_core::enforcement::{Enforcer, Observation};
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
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
        }
    }

    /// The usage recorded so far.
    pub(crate) const fn journal(&self) -> &Journal {
        &self.journal
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
    pub(crate) fn step(&mut self, policy: &Policy, elapsed: Duration, online: bool) {
        let processes = match self.platform.processes() {
            Ok(processes) => processes,
            Err(error) => {
                warn!(%error, "cannot list processes");
                return;
            }
        };
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
        for action in self.enforcer.step(&observations, elapsed) {
            actions::perform(&self.platform, policy, action);
        }
    }
}

#[cfg(test)]
mod tests;
