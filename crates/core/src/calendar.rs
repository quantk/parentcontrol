//! Budget days: civil days that start at a configurable time instead of
//! midnight, in the server's time zone.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MINUTES_PER_DAY: u16 = 24 * 60;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// A wall-clock time with minute precision, `00:00` to `23:59`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeOfDay(u16);

/// A string or numbers that do not form a valid [`TimeOfDay`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid time of day {0:?}, expected HH:MM between 00:00 and 23:59")]
pub struct InvalidTimeOfDay(String);

impl TimeOfDay {
    /// Midnight, `00:00`.
    pub const MIDNIGHT: Self = Self(0);

    /// Builds a time from hours and minutes.
    ///
    /// # Errors
    ///
    /// Fails when `hour > 23` or `minute > 59`.
    pub fn new(hour: u8, minute: u8) -> Result<Self, InvalidTimeOfDay> {
        if hour > 23 || minute > 59 {
            return Err(InvalidTimeOfDay(format!("{hour}:{minute}")));
        }
        Ok(Self(u16::from(hour) * 60 + u16::from(minute)))
    }

    /// Minutes since midnight.
    #[must_use]
    pub const fn minutes_since_midnight(self) -> u16 {
        self.0
    }

    /// Minutes from `day_start` forward to this time, in `0..1440`.
    #[must_use]
    pub const fn minutes_after(self, day_start: Self) -> u16 {
        (self.0 + MINUTES_PER_DAY - day_start.0) % MINUTES_PER_DAY
    }

    /// The time `minutes` after `self`, wrapping around midnight.
    #[must_use]
    pub const fn plus_minutes(self, minutes: u16) -> Self {
        Self((self.0 + minutes % MINUTES_PER_DAY) % MINUTES_PER_DAY)
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.0 / 60, self.0 % 60)
    }
}

impl FromStr for TimeOfDay {
    type Err = InvalidTimeOfDay;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidTimeOfDay(s.to_owned());
        let (hour, minute) = s.split_once(':').ok_or_else(invalid)?;
        if hour.len() != 2 || minute.len() != 2 {
            return Err(invalid());
        }
        let hour = hour.parse().map_err(|_| invalid())?;
        let minute = minute.parse().map_err(|_| invalid())?;
        Self::new(hour, minute).map_err(|_| invalid())
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = InvalidTimeOfDay;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<TimeOfDay> for String {
    fn from(value: TimeOfDay) -> Self {
        value.to_string()
    }
}

/// A budget day, named by the civil date on which it starts.
///
/// The quota of the day of week is taken from this date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BudgetDay(Date);

impl BudgetDay {
    /// The budget day starting on `date`.
    #[must_use]
    pub const fn new(date: Date) -> Self {
        Self(date)
    }

    /// The civil date on which the budget day starts.
    #[must_use]
    pub const fn date(self) -> Date {
        self.0
    }

    /// The day of week whose schedule applies.
    #[must_use]
    pub fn weekday(self) -> Weekday {
        self.0.weekday()
    }
}

impl fmt::Display for BudgetDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Where a moment falls relative to budget days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayPosition {
    /// The budget day containing the moment.
    pub day: BudgetDay,
    /// Wall-clock time elapsed since the budget day started, below 24 hours.
    pub elapsed: Duration,
}

/// Maps moments to budget days in a fixed time zone.
#[derive(Debug, Clone, PartialEq)]
pub struct Calendar {
    time_zone: TimeZone,
    day_start: TimeOfDay,
}

impl Calendar {
    /// A calendar whose days start at `day_start` in `time_zone`.
    #[must_use]
    pub const fn new(time_zone: TimeZone, day_start: TimeOfDay) -> Self {
        Self {
            time_zone,
            day_start,
        }
    }

    /// The time zone of the server, in which days and windows are counted.
    #[must_use]
    pub const fn time_zone(&self) -> &TimeZone {
        &self.time_zone
    }

    /// The wall-clock time at which every budget day starts.
    #[must_use]
    pub const fn day_start(&self) -> TimeOfDay {
        self.day_start
    }

    /// Finds the budget day containing `now` and how far into it `now` is.
    ///
    /// The position is measured in wall-clock time, so on days with a DST
    /// change windows keep their nominal clock times.
    #[must_use]
    pub fn locate(&self, now: Timestamp) -> DayPosition {
        let civil = now.to_zoned(self.time_zone.clone()).datetime();
        let time = civil.time();
        let second_of_day = u64::from(time.hour().unsigned_abs()) * 3600
            + u64::from(time.minute().unsigned_abs()) * 60
            + u64::from(time.second().unsigned_abs());
        let start_second = u64::from(self.day_start.minutes_since_midnight()) * 60;

        let (date, elapsed) = if second_of_day >= start_second {
            (civil.date(), second_of_day - start_second)
        } else {
            // `yesterday` fails only at `Date::MIN`, which no real clock reaches.
            let date = civil.date().yesterday().unwrap_or(civil.date());
            (date, SECONDS_PER_DAY - start_second + second_of_day)
        };
        DayPosition {
            day: BudgetDay(date),
            elapsed: Duration::from_secs(elapsed),
        }
    }
}

#[cfg(test)]
mod tests;
