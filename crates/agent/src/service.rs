//! The agent's main loop: timing, files, the server link, the trays and
//! the stop signal around the [`Limiter`].

use std::time::Duration;

use parentcontrol_core::clock::Clock as _;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{error, info, warn};

use crate::clock::ServerClock;
use crate::limiter::Limiter;
use crate::platform::{self, Platform};
use crate::policy_file::PolicyFile;
use crate::store::{Store, StoreError};
use crate::sync::{self, Event};
use crate::trays::Trays;

/// How often processes are scanned; a forbidden launch lives at most this long.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);

/// How often the journal is saved; at most this much usage is lost on a crash.
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Runs the agent on the files in `store` until asked to stop, then saves
/// the journal.
pub(crate) async fn run(platform: impl Platform, store: Store) -> Result<(), StoreError> {
    let clock = ServerClock::new(store.load_clock_offset()?);
    let mut limiter = Limiter::new(platform, clock, store.load_journal()?, SCAN_INTERVAL);
    let mut policy = PolicyFile::new(store.policy_path());

    let (events_sender, mut events) = mpsc::channel(16);
    let (reports, report) = watch::channel(limiter.report());
    sync::spawn(store.link_path(), events_sender, report);

    let mut trays = Trays::start()
        .inspect_err(|error| warn!(%error, "trays cannot connect; limits apply without them"))
        .ok();

    let mut ticks = tokio::time::interval(SCAN_INTERVAL);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let shutdown = platform::shutdown_signal();
    tokio::pin!(shutdown);

    let mut online = false;
    let mut last_tick = Instant::now();
    let mut last_flush = last_tick;
    info!(interval = ?SCAN_INTERVAL, "agent started");
    loop {
        tokio::select! {
            now = ticks.tick() => {
                if let Some(policy) = policy.refresh() {
                    let outcome = limiter.step(policy, now - last_tick, online);
                    if let Some(trays) = &mut trays {
                        trays.update(policy, limiter.statuses());
                        for notice in outcome.notices {
                            trays.notify(policy, notice);
                        }
                        for (owner, process) in outcome.soft_close {
                            trays.close_windows(owner, process);
                        }
                    }
                }
                last_tick = now;
                reports.send_if_modified(|current| {
                    let report = limiter.report();
                    let changed = *current != report;
                    *current = report;
                    changed
                });
                if now - last_flush >= FLUSH_INTERVAL {
                    last_flush = now;
                    if let Err(error) = store.save_journal(limiter.journal()) {
                        error!(%error, "cannot save the usage journal");
                    }
                }
            }
            Some(event) = events.recv() => apply(event, &store, &mut policy, &mut limiter, &mut online),
            result = &mut shutdown => {
                if let Err(error) = result {
                    error!(%error, "cannot wait for a stop signal; stopping");
                }
                break;
            }
        }
    }
    info!("agent stopping");
    store.save_journal(limiter.journal())
}

/// Applies what the server sent or what happened to the connection.
fn apply<P: Platform>(
    event: Event,
    store: &Store,
    policy: &mut PolicyFile,
    limiter: &mut Limiter<P, ServerClock>,
    online: &mut bool,
) {
    match event {
        Event::Policy {
            policy: new,
            server_time,
        } => {
            let offset = limiter.clock().synchronize(server_time);
            if let Err(error) = store.save_clock_offset(offset) {
                error!(%error, "cannot save the clock correction");
            }
            if let Err(error) = policy.replace(new) {
                error!(%error, "cannot save the new policy; it applies until the agent restarts");
            }
        }
        Event::Online(connected) => *online = connected,
        Event::Acknowledged(acknowledgements) => {
            let today = policy
                .refresh()
                .map(|policy| policy.calendar().locate(limiter.clock().now()).day);
            if let Some(today) = today {
                limiter.acknowledge(&acknowledgements, today);
            }
        }
    }
}
