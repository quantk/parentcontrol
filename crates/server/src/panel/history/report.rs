//! Aggregations of usage records for the history page, free of
//! presentation.

use std::collections::BTreeMap;
use std::time::Duration;

use jiff::{Timestamp, ToSpan};
use parentcontrol_core::calendar::{BudgetDay, Calendar};
use parentcontrol_core::ids::{DeviceId, GroupId};

use crate::db::usage::Session;

/// Hours in a budget day, the columns of the heatmap.
pub(super) const HOURS: usize = 24;

/// The days a report covers, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Period {
    pub(super) days: Vec<BudgetDay>,
}

impl Period {
    /// `length` days ending with `last`.
    pub(super) fn ending(last: BudgetDay, length: u16) -> Self {
        let days = (0..i64::from(length))
            .rev()
            .filter_map(|back| last.date().checked_sub(back.days()).ok())
            .map(BudgetDay::new)
            .collect();
        Self { days }
    }

    /// The period of the same length right before this one.
    pub(super) fn previous(&self) -> Self {
        let length = u16::try_from(self.days.len()).unwrap_or(u16::MAX);
        match self.first().date().yesterday() {
            Ok(last) => Self::ending(BudgetDay::new(last), length),
            Err(_) => Self { days: Vec::new() },
        }
    }

    pub(super) fn first(&self) -> BudgetDay {
        self.days
            .first()
            .copied()
            .unwrap_or(BudgetDay::new(jiff::civil::Date::MIN))
    }

    pub(super) fn last(&self) -> BudgetDay {
        self.days
            .last()
            .copied()
            .unwrap_or(BudgetDay::new(jiff::civil::Date::MIN))
    }

    pub(super) fn contains(&self, day: BudgetDay) -> bool {
        self.first() <= day && day <= self.last()
    }
}

/// Usage of one day, by group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DayUsage {
    pub(super) day: BudgetDay,
    /// Groups in the order of `groups` given to [`daily`]; zero when unused.
    pub(super) by_group: Vec<Duration>,
    pub(super) offline: bool,
}

impl DayUsage {
    pub(super) fn total(&self) -> Duration {
        self.by_group.iter().sum()
    }
}

/// Usage of every day of `period` by each of `groups`.
pub(super) fn daily(sessions: &[Session], period: &Period, groups: &[GroupId]) -> Vec<DayUsage> {
    period
        .days
        .iter()
        .map(|&day| {
            let of_day: Vec<&Session> = sessions.iter().filter(|s| s.day == day).collect();
            DayUsage {
                day,
                by_group: groups
                    .iter()
                    .map(|&group| {
                        of_day
                            .iter()
                            .filter(|s| s.group == group)
                            .map(|s| s.duration)
                            .sum()
                    })
                    .collect(),
                offline: of_day.iter().any(|s| s.offline),
            }
        })
        .collect()
}

/// Total of the sessions within `period`.
pub(super) fn total(sessions: &[Session], period: &Period) -> Duration {
    sessions
        .iter()
        .filter(|s| period.contains(s.day))
        .map(|s| s.duration)
        .sum()
}

/// Usage by budget weekday (Monday first) and hour since the day start.
pub(super) type Heatmap = [[Duration; HOURS]; 7];

/// Spreads every session over the hours it covered.
///
/// A session is one continuous stretch, so it covers the wall-clock time
/// from its start for its duration.
pub(super) fn heatmap(sessions: &[Session], calendar: &Calendar) -> Heatmap {
    const HOUR: u64 = 3600;
    let mut cells = [[Duration::ZERO; HOURS]; 7];
    for session in sessions {
        let mut moment = session.start;
        let mut left = session.duration.as_secs();
        while left > 0 {
            let position = calendar.locate(moment);
            let elapsed = position.elapsed.as_secs();
            let piece = left.min(HOUR - elapsed % HOUR);
            let row = usize::from(
                position
                    .day
                    .weekday()
                    .to_monday_zero_offset()
                    .unsigned_abs(),
            );
            let column = usize::try_from(elapsed / HOUR).unwrap_or(0).min(HOURS - 1);
            cells[row][column] += Duration::from_secs(piece);
            left -= piece;
            let Ok(next) = moment.checked_add(jiff::SignedDuration::from_secs(
                i64::try_from(piece).unwrap_or(i64::MAX),
            )) else {
                break;
            };
            moment = next;
        }
    }
    cells
}

/// Usage by device, the most used first.
pub(super) fn by_device(sessions: &[Session]) -> Vec<(DeviceId, Duration)> {
    let mut totals: BTreeMap<DeviceId, Duration> = BTreeMap::new();
    for session in sessions {
        *totals.entry(session.device).or_default() += session.duration;
    }
    let mut totals: Vec<_> = totals.into_iter().collect();
    totals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    totals
}

/// The longest session.
pub(super) fn longest(sessions: &[Session]) -> Option<&Session> {
    sessions.iter().max_by_key(|s| s.duration)
}

/// Where a session lies within its budget day, in minutes from the start.
pub(super) fn offset_in_day(session: &Session, calendar: &Calendar) -> (f64, f64) {
    let position = calendar.locate(session.start);
    // Before its day's start only through clock changes; clamp to the day.
    let start = if position.day == session.day {
        position.elapsed.as_secs_f64() / 60.0
    } else {
        0.0
    };
    let length = session.duration.as_secs_f64() / 60.0;
    (start, length.min(24.0 * 60.0 - start))
}

/// When a session ended.
pub(super) fn end_of(session: &Session) -> Timestamp {
    session
        .start
        .checked_add(session.duration)
        .unwrap_or(session.start)
}

#[cfg(test)]
mod tests;
