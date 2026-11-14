//! Builders shared by unit tests.

use jiff::civil::date;

use crate::calendar::{BudgetDay, DayPosition, TimeOfDay};
use crate::ids::GroupId;
use crate::policy::GroupPolicy;
use crate::rules::{BuiltinProfile, Rule};
use crate::schedule::{DaySchedule, TimeWindow, WeekSchedule};
use crate::units::Minutes;
use std::time::Duration;

pub(crate) fn time(text: &str) -> TimeOfDay {
    text.parse().unwrap()
}

pub(crate) fn day_start() -> TimeOfDay {
    time("04:00")
}

/// Saturday 2026-11-14.
pub(crate) fn saturday() -> BudgetDay {
    BudgetDay::new(date(2026, 11, 14))
}

/// A position on [`saturday`] at wall-clock `clock`.
pub(crate) fn at(clock: &str) -> DayPosition {
    let minutes = time(clock).minutes_after(day_start());
    DayPosition {
        day: saturday(),
        elapsed: Duration::from_mins(u64::from(minutes)),
    }
}

pub(crate) fn minutes(n: u64) -> Duration {
    Duration::from_mins(n)
}

/// A Minecraft group with `quota` minutes and `windows` every day.
pub(crate) fn group(quota: u32, windows: &[(&str, &str)]) -> GroupPolicy {
    let windows = windows
        .iter()
        .map(|(start, end)| TimeWindow::new(time(start), time(end)).unwrap())
        .collect();
    GroupPolicy {
        id: GroupId::new(1),
        name: "Игры".to_owned(),
        rules: vec![Rule::Builtin(BuiltinProfile::MinecraftJava)],
        schedule: WeekSchedule::every_day(&DaySchedule::new(Minutes::new(quota), windows).unwrap()),
        blocked: false,
        bonuses: Vec::new(),
        remote_usage: None,
    }
}
