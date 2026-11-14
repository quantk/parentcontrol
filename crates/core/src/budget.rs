//! Whether a group may be used right now, and for how long.

use std::time::Duration;

use crate::calendar::{DayPosition, TimeOfDay};
use crate::ids::GroupId;
use crate::policy::GroupPolicy;

/// Why a group may not be used.
///
/// When several reasons hold, the one the child can do least about wins:
/// a block, then the window, then the quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DenyReason {
    /// The parent has blocked the group.
    Blocked,
    /// The current moment is outside every allowed window.
    OutsideWindow,
    /// Today's quota, including bonuses, is used up.
    QuotaExhausted,
}

/// What will end the current permission first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopReason {
    /// The quota runs out.
    QuotaEnd,
    /// The allowed window closes.
    WindowEnd,
}

/// Whether the group may be used now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Use is allowed for `stop_in` more, after which `reason` stops it.
    Allowed {
        /// Time left until the permission ends.
        stop_in: Duration,
        /// What ends it.
        reason: StopReason,
    },
    /// Use is not allowed.
    Denied(DenyReason),
}

/// The state of one group at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupStatus {
    /// The group.
    pub group: GroupId,
    /// Quota left today, regardless of windows and blocks.
    pub remaining: Duration,
    /// Whether the group may be used now.
    pub availability: Availability,
    /// When the next window opens today, if the group is now outside one.
    pub next_window: Option<TimeOfDay>,
}

/// Where the current moment is relative to the day's windows.
enum Window {
    /// The day has no windows.
    Unrestricted,
    /// Inside a window that closes after `left`.
    Inside { left: Duration },
    /// Outside every window.
    Outside,
}

/// Evaluates `group` at `position` given `local_usage`, this device's own
/// usage of the group during the day.
#[must_use]
pub fn evaluate(
    group: &GroupPolicy,
    day_start: TimeOfDay,
    position: DayPosition,
    local_usage: Duration,
) -> GroupStatus {
    let day = group.schedule.day(position.day.weekday());
    let allowance = day.quota().to_duration() + group.bonus(position.day).to_duration();
    let remaining = allowance
        .saturating_sub(group.remote_usage(position.day))
        .saturating_sub(local_usage);

    let now = position.elapsed;
    let (window, next_window) = match day.spans(day_start) {
        None => (Window::Unrestricted, None),
        Some(spans) => {
            if let Some(span) = spans
                .iter()
                .find(|span| span.start <= now && now < span.end)
            {
                (
                    Window::Inside {
                        left: span.end.saturating_sub(now),
                    },
                    None,
                )
            } else {
                let next = spans.iter().find(|span| span.start > now);
                (
                    Window::Outside,
                    next.map(|span| clock_time(day_start, span.start)),
                )
            }
        }
    };

    let availability = match window {
        _ if group.blocked => Availability::Denied(DenyReason::Blocked),
        Window::Outside => Availability::Denied(DenyReason::OutsideWindow),
        _ if remaining.is_zero() => Availability::Denied(DenyReason::QuotaExhausted),
        Window::Inside { left } if left < remaining => Availability::Allowed {
            stop_in: left,
            reason: StopReason::WindowEnd,
        },
        Window::Inside { .. } | Window::Unrestricted => Availability::Allowed {
            stop_in: remaining,
            reason: StopReason::QuotaEnd,
        },
    };

    GroupStatus {
        group: group.id,
        remaining,
        availability,
        next_window,
    }
}

/// The wall-clock time `offset` after `day_start`.
fn clock_time(day_start: TimeOfDay, offset: Duration) -> TimeOfDay {
    // Offsets of span starts are whole minutes below a day, so this fits.
    let minutes = u16::try_from(offset.as_secs() / 60).unwrap_or(0);
    day_start.plus_minutes(minutes)
}

#[cfg(test)]
mod tests;
