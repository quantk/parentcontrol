//! Chart marks of the history page: aggregated usage turned into sizes,
//! colors, labels and tooltips the template only lays out.
//!
//! Charts are plain HTML sized by percentages, so their text keeps its
//! real size on every screen; each mark carries its description for the
//! tooltip and for screen readers.

use std::time::Duration;

use jiff::civil::Weekday;
use parentcontrol_core::calendar::{BudgetDay, TimeOfDay};

use super::report::{DayUsage, HOURS, Heatmap};
use crate::panel::format::{self, Percent};

/// Categorical colors in their fixed order; more groups share "other".
const SERIES: usize = 8;

/// Steps of the duration axis, in minutes.
const STEPS: [u64; 12] = [5, 10, 15, 30, 60, 90, 120, 180, 240, 360, 480, 720];

/// Gridlines above zero on the duration axis, at most.
const MAX_TICKS: u64 = 4;

/// Levels of the heatmap's sequential scale above zero.
const LEVELS: f64 = 6.0;

/// The CSS class of the color of the `index`-th group by id.
pub(super) fn series(index: Option<usize>) -> &'static str {
    const CLASSES: [&str; SERIES] = [
        "series-1", "series-2", "series-3", "series-4", "series-5", "series-6", "series-7",
        "series-8",
    ];
    index
        .and_then(|index| CLASSES.get(index).copied())
        .unwrap_or("series-other")
}

/// A gridline of the duration axis.
pub(super) struct Tick {
    pub(super) at: Percent,
    pub(super) label: String,
}

/// A duration axis from zero to a round value above `max`.
pub(super) struct Scale {
    top: Duration,
    pub(super) ticks: Vec<Tick>,
}

impl Scale {
    pub(super) fn up_to(max: Duration) -> Self {
        let minutes = max.as_secs().div_ceil(60).max(1);
        let step = STEPS
            .iter()
            .copied()
            .find(|step| minutes.div_ceil(*step) <= MAX_TICKS)
            .unwrap_or_else(|| minutes.div_ceil(MAX_TICKS).div_ceil(60) * 60);
        let count = minutes.div_ceil(step);
        let top = Duration::from_mins(step * count);
        let ticks = (0..=count)
            .map(|tick| {
                let value = Duration::from_mins(step * tick);
                Tick {
                    at: Percent::of_duration(value, top),
                    label: format::duration(value),
                }
            })
            .collect();
        Self { top, ticks }
    }

    pub(super) fn place(&self, value: Duration) -> Percent {
        Percent::of_duration(value, self.top)
    }
}

/// One group's part of a day's column.
pub(super) struct Stack {
    pub(super) series: &'static str,
    pub(super) height: Percent,
}

/// One day of the daily chart.
pub(super) struct Column {
    pub(super) day: BudgetDay,
    /// Shown under some columns only, so labels never collide.
    pub(super) label: Option<String>,
    pub(super) tip: String,
    pub(super) stacks: Vec<Stack>,
    /// The day's allowance, when one group is shown.
    pub(super) allowance: Option<Percent>,
    pub(super) over: bool,
    pub(super) selected: bool,
}

/// What [`columns`] needs about the groups shown.
pub(super) struct Series<'a> {
    pub(super) name: &'a str,
    pub(super) class: &'static str,
}

/// The daily chart: one column per day, groups stacked in `series` order.
pub(super) fn columns(
    days: &[DayUsage],
    series: &[Series<'_>],
    allowances: Option<&[Duration]>,
    selected: BudgetDay,
) -> (Scale, Vec<Column>) {
    let highest = days
        .iter()
        .map(DayUsage::total)
        .chain(allowances.into_iter().flatten().copied())
        .max()
        .unwrap_or_default();
    let scale = Scale::up_to(highest);
    let label_every = label_spacing(days.len());
    let columns = days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let allowance = allowances.and_then(|all| all.get(index)).copied();
            let total = day.total();
            let mut tip = vec![
                format::day(day.day),
                format!("Всего {}", format::duration(total)),
            ];
            if series.len() > 1 {
                tip.extend(
                    series
                        .iter()
                        .zip(&day.by_group)
                        .filter(|(_, used)| !used.is_zero())
                        .map(|(series, used)| {
                            format!("{}: {}", series.name, format::duration(*used))
                        }),
                );
            }
            if let Some(allowance) = allowance {
                tip.push(format!("Можно было {}", format::duration(allowance)));
            }
            if day.offline {
                tip.push("Часть времени — без связи".to_owned());
            }
            Column {
                day: day.day,
                label: labelled(day.day, index, days.len(), label_every)
                    .then(|| short_date(day.day, days.len())),
                tip: tip.join("\n"),
                stacks: series
                    .iter()
                    .zip(&day.by_group)
                    .filter(|(_, used)| !used.is_zero())
                    .map(|(series, used)| Stack {
                        series: series.class,
                        height: scale.place(*used),
                    })
                    .collect(),
                allowance: allowance.map(|allowance| scale.place(allowance)),
                // Under a minute is rounding between ticks and devices.
                over: allowance
                    .is_some_and(|allowance| total >= allowance + Duration::from_mins(1)),
                selected: day.day == selected,
            }
        })
        .collect();
    (scale, columns)
}

/// Every how many Mondays a long period gets a label.
fn label_spacing(days: usize) -> usize {
    days.div_ceil(7 * 6).max(1)
}

/// Short periods label every day; long ones label some Mondays.
fn labelled(day: BudgetDay, index: usize, days: usize, every: usize) -> bool {
    if days <= 14 {
        return true;
    }
    let week = (days - 1 - index) / 7;
    day.weekday() == Weekday::Monday && week.is_multiple_of(every)
}

fn short_date(day: BudgetDay, days: usize) -> String {
    if days <= 14 {
        format!(
            "{}\n{}",
            format::weekday_short(day.weekday()),
            day.date().day()
        )
    } else {
        day.date().strftime("%d.%m").to_string()
    }
}

/// One cell of the heatmap.
pub(super) struct Cell {
    /// `0` for no use, then `1..=6` by share of the busiest hour.
    pub(super) level: u8,
    pub(super) tip: String,
}

pub(super) struct HeatRow {
    pub(super) label: &'static str,
    pub(super) cells: Vec<Cell>,
}

/// The heatmap's rows, Monday first, and the hour labels of its columns.
pub(super) fn heat_rows(cells: &Heatmap, day_start: TimeOfDay) -> (Vec<HeatRow>, Vec<String>) {
    let busiest = cells.iter().flatten().max().copied().unwrap_or_default();
    let hour = |column: usize| day_start.plus_minutes(u16::try_from(column * 60).unwrap_or(0));
    let rows = WEEKDAYS
        .iter()
        .zip(cells)
        .map(|(&weekday, row)| HeatRow {
            label: format::weekday_short(weekday),
            cells: row
                .iter()
                .enumerate()
                .map(|(column, used)| Cell {
                    level: level(*used, busiest),
                    tip: format!(
                        "{}, {}–{}\n{}",
                        format::weekday_short(weekday),
                        hour(column),
                        hour(column + 1),
                        format::duration(*used)
                    ),
                })
                .collect(),
        })
        .collect();
    let labels = (0..HOURS)
        .map(|column| {
            // Every third hour is labelled.
            if column % 3 == 0 {
                hour(column).to_string()
            } else {
                String::new()
            }
        })
        .collect();
    (rows, labels)
}

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

fn level(used: Duration, busiest: Duration) -> u8 {
    if used < Duration::from_secs(30) || busiest.is_zero() {
        return 0;
    }
    let share = Percent::of_duration(used, busiest).fraction();
    // In 1..=6 by construction.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded"
    )]
    let level = (share * LEVELS).ceil().clamp(1.0, LEVELS) as u8;
    level
}

/// A horizontal bar of a ranking.
pub(super) struct Bar {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) width: Percent,
}

/// Bars for `(name, value)` pairs, scaled to the largest.
pub(super) fn bars(items: Vec<(String, Duration)>) -> Vec<Bar> {
    let largest = items
        .iter()
        .map(|(_, value)| *value)
        .max()
        .unwrap_or_default();
    items
        .into_iter()
        .map(|(name, value)| Bar {
            name,
            value: format::duration(value),
            width: Percent::of_duration(value, largest),
        })
        .collect()
}

#[cfg(test)]
mod tests;
