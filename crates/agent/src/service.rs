//! The agent's main loop: timing, files and the stop signal around the
//! [`Limiter`].

use std::time::Duration;

use parentcontrol_core::clock::Clock;
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{error, info};

use crate::limiter::Limiter;
use crate::platform::{self, Platform};
use crate::policy_file::PolicyFile;
use crate::store::{Store, StoreError};

/// How often processes are scanned; a forbidden launch lives at most this long.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);

/// How often the journal is saved; at most this much usage is lost on a crash.
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Runs the agent on the files in `store` until asked to stop, then saves
/// the journal.
pub(crate) async fn run(
    platform: impl Platform,
    clock: impl Clock,
    store: Store,
) -> Result<(), StoreError> {
    let mut limiter = Limiter::new(platform, clock, store.load_journal()?, SCAN_INTERVAL);
    let mut policy = PolicyFile::new(store.policy_path());

    let mut ticks = tokio::time::interval(SCAN_INTERVAL);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let shutdown = platform::shutdown_signal();
    tokio::pin!(shutdown);

    let mut last_tick = Instant::now();
    let mut last_flush = last_tick;
    info!(interval = ?SCAN_INTERVAL, "agent started");
    loop {
        tokio::select! {
            now = ticks.tick() => {
                if let Some(policy) = policy.refresh() {
                    limiter.step(policy, now - last_tick);
                }
                last_tick = now;
                if now - last_flush >= FLUSH_INTERVAL {
                    last_flush = now;
                    if let Err(error) = store.save_journal(limiter.journal()) {
                        error!(%error, "cannot save the usage journal");
                    }
                }
            }
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
