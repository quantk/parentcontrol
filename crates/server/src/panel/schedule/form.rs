//! A week of one group as the schedule form shows it and submits it.
//!
//! The same structure renders stored schedules and the parent's
//! submission, so a rejected submission comes back with every edit kept.

use jiff::civil::Weekday;
use parentcontrol_core::calendar::TimeOfDay;
use parentcontrol_core::schedule::{DaySchedule, MAX_DAILY_QUOTA, TimeWindow, WeekSchedule};
use parentcontrol_core::units::Minutes;

use crate::panel::format::{self, Percent};

/// Days in display order with their form keys.
pub(super) const WEEK: [(Weekday, &str); 7] = [
    (Weekday::Monday, "monday"),
    (Weekday::Tuesday, "tuesday"),
    (Weekday::Wednesday, "wednesday"),
    (Weekday::Thursday, "thursday"),
    (Weekday::Friday, "friday"),
    (Weekday::Saturday, "saturday"),
    (Weekday::Sunday, "sunday"),
];

/// One window as typed: `HH:MM` strings, possibly empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct WindowField {
    pub(super) start: String,
    pub(super) end: String,
}

impl WindowField {
    fn is_blank(&self) -> bool {
        self.start.trim().is_empty() && self.end.trim().is_empty()
    }
}

/// A stretch of the preview strip, in percent of the budget day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Segment {
    pub(super) from: Percent,
    pub(super) length: Percent,
}

/// One day of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DayField {
    pub(super) key: &'static str,
    pub(super) weekday: Weekday,
    /// Minutes as typed.
    pub(super) quota: String,
    /// "Весь день" rather than "по часам".
    pub(super) all_day: bool,
    /// At least one row, so there is always a place to type a window.
    pub(super) windows: Vec<WindowField>,
}

impl DayField {
    /// The stored `day`.
    pub(super) fn stored(weekday: Weekday, key: &'static str, day: &DaySchedule) -> Self {
        let mut windows: Vec<WindowField> = day
            .windows()
            .iter()
            .map(|window| WindowField {
                start: window.start().to_string(),
                end: window.end().to_string(),
            })
            .collect();
        if windows.is_empty() {
            windows.push(WindowField::default());
        }
        Self {
            key,
            weekday,
            quota: day.quota().get().to_string(),
            all_day: day.windows().is_empty(),
            windows,
        }
    }

    /// The day as submitted in `fields`; starts and ends pair up in order.
    pub(super) fn submitted(
        weekday: Weekday,
        key: &'static str,
        fields: &[(String, String)],
    ) -> Self {
        let values = |prefix: &str| {
            let name = format!("{prefix}_{key}");
            fields
                .iter()
                .filter(move |(field, _)| *field == name)
                .map(|(_, value)| value.clone())
        };
        let mut windows: Vec<WindowField> = values("start")
            .zip(values("end"))
            .map(|(start, end)| WindowField { start, end })
            .collect();
        if windows.is_empty() {
            windows.push(WindowField::default());
        }
        Self {
            key,
            weekday,
            quota: values("quota").next().unwrap_or_default(),
            all_day: values("mode").next().is_none_or(|mode| mode != "windows"),
            windows,
        }
    }

    /// Full name of the day.
    pub(super) const fn label(&self) -> &'static str {
        format::weekday(self.weekday)
    }

    /// The day as a schedule, checked against `day_start`.
    pub(super) fn parse(&self, day_start: TimeOfDay) -> Result<DaySchedule, String> {
        let label = self.label();
        let quota: u32 = self
            .quota
            .trim()
            .parse()
            .map_err(|_| format!("{label}: время на день нужно указать числом минут."))?;
        let windows = if self.all_day {
            Vec::new()
        } else {
            let windows = self
                .windows(day_start)
                .map_err(|error| format!("{label}: {error}"))?;
            if windows.is_empty() {
                return Err(format!(
                    "{label}: укажите хотя бы одно окно или выберите «Весь день»."
                ));
            }
            windows
        };
        DaySchedule::new(Minutes::new(quota), windows).map_err(|_| {
            format!(
                "{label}: больше {} минут в сутках не бывает.",
                MAX_DAILY_QUOTA.get()
            )
        })
    }

    /// The typed windows, skipping blank rows.
    fn windows(&self, day_start: TimeOfDay) -> Result<Vec<TimeWindow>, String> {
        self.windows
            .iter()
            .filter(|field| !field.is_blank())
            .map(|field| {
                let time = |text: &str| {
                    text.trim()
                        .parse::<TimeOfDay>()
                        .map_err(|_| "у окна должны быть указаны начало и конец.".to_owned())
                };
                let (start, end) = (time(&field.start)?, time(&field.end)?);
                let window = TimeWindow::new(start, end)
                    .map_err(|_| format!("окно {start}–{end} пустое."))?;
                if window.span(day_start).is_none() {
                    return Err(format!(
                        "окно {start}–{end} пересекает начало суток в {day_start}."
                    ));
                }
                Ok(window)
            })
            .collect()
    }

    /// Allowed stretches of the budget day for the preview strip; the
    /// windows that do not parse yet are left out.
    pub(super) fn preview(&self, day_start: TimeOfDay) -> Vec<Segment> {
        if self.all_day {
            return vec![Segment {
                from: Percent::of(0.0, 1.0),
                length: Percent::of(1.0, 1.0),
            }];
        }
        self.windows
            .iter()
            .filter_map(|field| {
                let start = field.start.trim().parse::<TimeOfDay>().ok()?;
                let end = field.end.trim().parse::<TimeOfDay>().ok()?;
                let span = TimeWindow::new(start, end).ok()?.span(day_start)?;
                let day = std::time::Duration::from_hours(24);
                Some(Segment {
                    from: Percent::of_duration(span.start, day),
                    length: Percent::of_duration(span.end.saturating_sub(span.start), day),
                })
            })
            .collect()
    }
}

/// Every day of a stored week.
pub(super) fn stored_week(schedule: &WeekSchedule) -> Vec<DayField> {
    WEEK.iter()
        .map(|&(weekday, key)| DayField::stored(weekday, key, schedule.day(weekday)))
        .collect()
}

/// Every day of a submitted week.
pub(super) fn submitted_week(fields: &[(String, String)]) -> Vec<DayField> {
    WEEK.iter()
        .map(|&(weekday, key)| DayField::submitted(weekday, key, fields))
        .collect()
}

/// The week the form describes, or the first problem in it.
pub(super) fn parse_week(days: &[DayField], day_start: TimeOfDay) -> Result<WeekSchedule, String> {
    let parsed = days
        .iter()
        .map(|day| day.parse(day_start))
        .collect::<Result<Vec<_>, _>>()?;
    let [
        monday,
        tuesday,
        wednesday,
        thursday,
        friday,
        saturday,
        sunday,
    ] = <[DaySchedule; 7]>::try_from(parsed).map_err(|_| "Неполное расписание.".to_owned())?;
    Ok(WeekSchedule {
        monday,
        tuesday,
        wednesday,
        thursday,
        friday,
        saturday,
        sunday,
    })
}

#[cfg(test)]
mod tests;
