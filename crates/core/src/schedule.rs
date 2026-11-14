//! Weekly schedule of a group: daily quota and allowed time windows.

use std::time::Duration;

use jiff::civil::Weekday;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::calendar::TimeOfDay;
use crate::units::Minutes;

/// The largest quota that makes sense: a whole day.
pub const MAX_DAILY_QUOTA: Minutes = Minutes::new(24 * 60);

/// A schedule element that cannot be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    /// A window starts and ends at the same time.
    #[error("time window {0}-{0} is empty")]
    EmptyWindow(TimeOfDay),
    /// The quota exceeds [`MAX_DAILY_QUOTA`].
    #[error("daily quota of {0} exceeds a whole day")]
    QuotaTooLarge(Minutes),
}

/// A daily interval during which the group may be used, in wall-clock time.
///
/// It may cross midnight (`22:00`–`01:00`) but has to stay inside one budget
/// day; that is checked against the day start when a policy is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawWindow", into = "RawWindow")]
pub struct TimeWindow {
    start: TimeOfDay,
    end: TimeOfDay,
}

#[derive(Serialize, Deserialize)]
struct RawWindow {
    start: TimeOfDay,
    end: TimeOfDay,
}

/// A half-open interval `[start, end)` measured from the budget day start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Offset of the first allowed moment.
    pub start: Duration,
    /// Offset of the first moment no longer allowed.
    pub end: Duration,
}

impl TimeWindow {
    /// A window from `start` (inclusive) to `end` (exclusive).
    ///
    /// # Errors
    ///
    /// Fails when `start == end`.
    pub fn new(start: TimeOfDay, end: TimeOfDay) -> Result<Self, ScheduleError> {
        if start == end {
            return Err(ScheduleError::EmptyWindow(start));
        }
        Ok(Self { start, end })
    }

    /// When the window opens.
    #[must_use]
    pub const fn start(self) -> TimeOfDay {
        self.start
    }

    /// When the window closes.
    #[must_use]
    pub const fn end(self) -> TimeOfDay {
        self.end
    }

    /// The window relative to a budget day starting at `day_start`, or
    /// `None` when it would cross that start.
    #[must_use]
    pub fn span(self, day_start: TimeOfDay) -> Option<Span> {
        let start = self.start.minutes_after(day_start);
        // An end exactly at the day start means the very end of the day.
        let end = match self.end.minutes_after(day_start) {
            0 => 24 * 60,
            end => end,
        };
        (start < end).then(|| Span {
            start: Minutes::new(start.into()).to_duration(),
            end: Minutes::new(end.into()).to_duration(),
        })
    }
}

impl TryFrom<RawWindow> for TimeWindow {
    type Error = ScheduleError;

    fn try_from(raw: RawWindow) -> Result<Self, Self::Error> {
        Self::new(raw.start, raw.end)
    }
}

impl From<TimeWindow> for RawWindow {
    fn from(window: TimeWindow) -> Self {
        Self {
            start: window.start,
            end: window.end,
        }
    }
}

/// Rules for one day of week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawDay", into = "RawDay")]
pub struct DaySchedule {
    quota: Minutes,
    windows: Vec<TimeWindow>,
}

#[derive(Serialize, Deserialize)]
struct RawDay {
    quota: Minutes,
    #[serde(default)]
    windows: Vec<TimeWindow>,
}

impl DaySchedule {
    /// A day with `quota` minutes, usable only inside `windows`.
    ///
    /// No windows means the whole day is open and only the quota limits use.
    ///
    /// # Errors
    ///
    /// Fails when `quota` exceeds [`MAX_DAILY_QUOTA`].
    pub fn new(quota: Minutes, windows: Vec<TimeWindow>) -> Result<Self, ScheduleError> {
        if quota > MAX_DAILY_QUOTA {
            return Err(ScheduleError::QuotaTooLarge(quota));
        }
        Ok(Self { quota, windows })
    }

    /// The daily quota.
    #[must_use]
    pub const fn quota(&self) -> Minutes {
        self.quota
    }

    /// The allowed windows as configured.
    #[must_use]
    pub fn windows(&self) -> &[TimeWindow] {
        &self.windows
    }

    /// Allowed spans of the day, sorted and merged so they do not overlap.
    ///
    /// `None` means the day has no window restriction. Windows that cross
    /// the day start are rejected when a policy is built, so none are lost
    /// here in a valid policy.
    #[must_use]
    pub fn spans(&self, day_start: TimeOfDay) -> Option<Vec<Span>> {
        if self.windows.is_empty() {
            return None;
        }
        let mut spans: Vec<Span> = self
            .windows
            .iter()
            .filter_map(|w| w.span(day_start))
            .collect();
        spans.sort_by_key(|span| span.start);
        let mut merged: Vec<Span> = Vec::with_capacity(spans.len());
        for span in spans {
            match merged.last_mut() {
                Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
                _ => merged.push(span),
            }
        }
        Some(merged)
    }
}

impl TryFrom<RawDay> for DaySchedule {
    type Error = ScheduleError;

    fn try_from(raw: RawDay) -> Result<Self, Self::Error> {
        Self::new(raw.quota, raw.windows)
    }
}

impl From<DaySchedule> for RawDay {
    fn from(day: DaySchedule) -> Self {
        Self {
            quota: day.quota,
            windows: day.windows,
        }
    }
}

/// A schedule for every day of the week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    missing_docs,
    reason = "each field is the schedule of the day it names"
)]
pub struct WeekSchedule {
    pub monday: DaySchedule,
    pub tuesday: DaySchedule,
    pub wednesday: DaySchedule,
    pub thursday: DaySchedule,
    pub friday: DaySchedule,
    pub saturday: DaySchedule,
    pub sunday: DaySchedule,
}

impl WeekSchedule {
    /// The same schedule on every day.
    #[must_use]
    pub fn every_day(day: &DaySchedule) -> Self {
        Self {
            monday: day.clone(),
            tuesday: day.clone(),
            wednesday: day.clone(),
            thursday: day.clone(),
            friday: day.clone(),
            saturday: day.clone(),
            sunday: day.clone(),
        }
    }

    /// The schedule of `weekday`.
    #[must_use]
    pub const fn day(&self, weekday: Weekday) -> &DaySchedule {
        match weekday {
            Weekday::Monday => &self.monday,
            Weekday::Tuesday => &self.tuesday,
            Weekday::Wednesday => &self.wednesday,
            Weekday::Thursday => &self.thursday,
            Weekday::Friday => &self.friday,
            Weekday::Saturday => &self.saturday,
            Weekday::Sunday => &self.sunday,
        }
    }

    /// All seven days, Monday first.
    pub fn days(&self) -> impl Iterator<Item = &DaySchedule> {
        [
            &self.monday,
            &self.tuesday,
            &self.wednesday,
            &self.thursday,
            &self.friday,
            &self.saturday,
            &self.sunday,
        ]
        .into_iter()
    }
}

#[cfg(test)]
mod tests;
