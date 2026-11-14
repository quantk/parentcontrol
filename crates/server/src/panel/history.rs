//! Usage over a period: totals, a chart by day, the hours of the week
//! when programs are used, devices, one day in detail, and the parent's
//! actions.

mod charts;
mod report;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;

use askama::Template;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use jiff::{Timestamp, ToSpan};
use parentcontrol_core::calendar::{BudgetDay, Calendar};
use parentcontrol_core::ids::{DeviceId, GroupId};
use parentcontrol_core::policy::{self, Adjustment};
use serde::Deserialize;

use self::charts::{Bar, Column, HeatRow, Scale, Series};
use self::report::Period;
use super::format::{self, Percent};
use super::{Icon, Layout, render, status};
use crate::auth::Parent;
use crate::db::usage::Session;
use crate::db::{DEFAULT_PROFILE, actions, devices, groups, settings, usage};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/history";

/// Periods offered, in days; the first is the default.
const PERIODS: [u16; 4] = [14, 7, 30, 90];

/// Parent actions shown.
const ACTIONS: u32 = 30;

#[derive(Debug, Deserialize)]
pub(super) struct HistoryQuery {
    /// Days to show, ending today.
    period: Option<u16>,
    /// The only group to show; all by default.
    group: Option<GroupId>,
    /// The day shown in detail; today by default.
    day: Option<jiff::civil::Date>,
}

/// What the filters select, and links that keep it.
#[derive(Debug, Clone, Copy)]
struct Selection {
    period: u16,
    group: Option<GroupId>,
    day: BudgetDay,
}

impl Selection {
    /// The page with these filters and this day in detail.
    fn link(self) -> String {
        format!("{}&day={}", self.filter_link(), self.day)
    }

    /// The page with these filters and today in detail; filter links are
    /// rendered once, while the day changes in place.
    fn filter_link(self) -> String {
        let mut link = format!("{PAGE}?period={}", self.period);
        if let Some(group) = self.group {
            let _ = write!(link, "&group={group}");
        }
        link
    }
}

struct Choice {
    label: String,
    link: String,
    selected: bool,
}

struct Stat {
    label: &'static str,
    value: String,
    note: String,
    /// Draws attention: overspend happened.
    alert: bool,
}

struct LegendItem {
    name: String,
    class: &'static str,
    total: String,
}

struct DailyChart {
    scale: Scale,
    columns: Vec<(Column, String)>,
    legend: Vec<LegendItem>,
    /// One group is shown, so its allowance is drawn too.
    shows_allowance: bool,
}

struct TimelineBar {
    class: &'static str,
    from: Percent,
    width: Percent,
    tip: String,
}

struct Lane {
    name: String,
    bars: Vec<TimelineBar>,
}

struct SessionRow {
    start: String,
    end: String,
    device: String,
    group: String,
    class: &'static str,
    duration: String,
    offline: bool,
}

struct DayDetail {
    title: String,
    total: String,
    previous: Option<String>,
    next: Option<String>,
    ticks: Vec<String>,
    lanes: Vec<Lane>,
    sessions: Vec<SessionRow>,
}

struct TableRow {
    label: String,
    used: String,
    allowance: Option<String>,
    overspend: Option<String>,
    offline: bool,
}

struct ActionRow {
    at: String,
    summary: String,
}

#[derive(Template)]
#[template(path = "history.html")]
struct HistoryPage {
    layout: Layout,
    periods: Vec<Choice>,
    group_filter: Vec<Choice>,
    stats: Vec<Stat>,
    daily: DailyChart,
    heat_rows: Vec<HeatRow>,
    heat_hours: Vec<String>,
    devices: Vec<Bar>,
    day: DayDetail,
    table: Vec<TableRow>,
    actions: Vec<ActionRow>,
    period_days: u16,
}

/// Everything the page reads from storage.
struct Stored {
    calendar: Calendar,
    today: BudgetDay,
    groups: Vec<groups::Group>,
    adjustments: Vec<(GroupId, Adjustment)>,
    devices: Vec<devices::Device>,
    /// The period and the one before it.
    sessions: Vec<Session>,
    /// The day shown in detail, which may lie outside the period.
    day_sessions: Vec<Session>,
    actions: Vec<actions::ParentAction>,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, AppError> {
    let period_days = query
        .period
        .filter(|days| PERIODS.contains(days))
        .unwrap_or(PERIODS[0]);
    let now = Timestamp::now();
    let stored = state
        .db
        .call(move |c| {
            let calendar = settings::required_calendar(c)?;
            let today = calendar.locate(now).day;
            let period = Period::ending(today, period_days);
            let day = query.day.map_or(today, BudgetDay::new);
            Ok(Stored {
                groups: groups::list(c, DEFAULT_PROFILE)?,
                adjustments: groups::adjustments_since(c, period.first())?,
                devices: devices::list(c)?,
                sessions: usage::sessions(c, period.previous().first(), today)?,
                day_sessions: usage::sessions(c, day, day)?,
                actions: actions::recent(c, ACTIONS)?,
                calendar,
                today,
            })
        })
        .await?;
    let selection = Selection {
        period: period_days,
        group: query
            .group
            .filter(|id| stored.groups.iter().any(|g| g.id == *id)),
        day: query.day.map_or(stored.today, BudgetDay::new),
    };
    Ok(render(&assemble(&stored, selection))?.into_response())
}

/// Names and colors of groups and devices, with fallbacks for deleted ones.
struct Names<'a> {
    /// Groups by id, in id order, which fixes their colors.
    groups: Vec<&'a groups::Group>,
    devices: HashMap<DeviceId, &'a str>,
}

impl<'a> Names<'a> {
    fn of(stored: &'a Stored) -> Self {
        let mut groups: Vec<_> = stored.groups.iter().collect();
        groups.sort_by_key(|group| group.id);
        Self {
            groups,
            devices: stored
                .devices
                .iter()
                .map(|d| (d.id, d.name.as_str()))
                .collect(),
        }
    }

    fn group(&self, id: GroupId) -> &'a str {
        self.groups
            .iter()
            .find(|group| group.id == id)
            .map_or("удалённая группа", |group| {
                group.name.as_str()
            })
    }

    fn class(&self, id: GroupId) -> &'static str {
        charts::series(self.groups.iter().position(|group| group.id == id))
    }

    fn device(&self, id: DeviceId) -> &'a str {
        self.devices
            .get(&id)
            .copied()
            .unwrap_or("удалённое устройство")
    }
}

fn assemble(stored: &Stored, selection: Selection) -> HistoryPage {
    let names = Names::of(stored);
    let period = Period::ending(stored.today, selection.period);
    let shown: Vec<&groups::Group> = names
        .groups
        .iter()
        .copied()
        .filter(|group| selection.group.is_none_or(|id| id == group.id))
        .collect();
    let in_view = |session: &&Session| selection.group.is_none_or(|id| id == session.group);
    let sessions: Vec<Session> = stored
        .sessions
        .iter()
        .filter(in_view)
        .filter(|s| period.contains(s.day))
        .cloned()
        .collect();
    // Both periods, for the comparison with the previous one.
    let both: Vec<Session> = stored.sessions.iter().filter(in_view).cloned().collect();

    let ids: Vec<GroupId> = shown.iter().map(|group| group.id).collect();
    let days = report::daily(&sessions, &period, &ids);
    let allowances: Vec<Vec<Duration>> = shown
        .iter()
        .map(|group| {
            let adjustments = status::of_group(&stored.adjustments, group.id);
            period
                .days
                .iter()
                .map(|&day| policy::allowance(&group.schedule, &adjustments, day).to_duration())
                .collect()
        })
        .collect();

    let (heat_rows, heat_hours) = charts::heat_rows(
        &report::heatmap(&sessions, &stored.calendar),
        stored.calendar.day_start(),
    );
    let day_sessions: Vec<&Session> = stored.day_sessions.iter().filter(in_view).collect();

    HistoryPage {
        layout: Layout {
            title: "История",
            active: PAGE,
            flash: None,
        },
        periods: PERIODS
            .iter()
            .copied()
            .map(|period| Choice {
                label: format!("{period} дн"),
                link: Selection {
                    period,
                    ..selection
                }
                .filter_link(),
                selected: period == selection.period,
            })
            .collect(),
        group_filter: group_filter(&names, selection),
        stats: stats(&sessions, &both, &period, &days, &allowances, &names),
        daily: daily_chart(&days, &shown, &allowances, &names, selection),
        heat_rows,
        heat_hours,
        devices: charts::bars(
            report::by_device(&sessions)
                .into_iter()
                .map(|(device, used)| (names.device(device).to_owned(), used))
                .collect(),
        ),
        day: day_detail(
            &day_sessions,
            &stored.calendar,
            stored.today,
            &names,
            selection,
        ),
        table: table(&days, &allowances),
        actions: stored
            .actions
            .iter()
            .map(|action| ActionRow {
                at: format::date_time(action.at, stored.calendar.time_zone()),
                summary: action.summary.clone(),
            })
            .collect(),
        period_days: selection.period,
    }
}

fn group_filter(names: &Names<'_>, selection: Selection) -> Vec<Choice> {
    if names.groups.len() < 2 {
        return Vec::new();
    }
    let all = Choice {
        label: "Все группы".to_owned(),
        link: Selection {
            group: None,
            ..selection
        }
        .filter_link(),
        selected: selection.group.is_none(),
    };
    std::iter::once(all)
        .chain(names.groups.iter().map(|group| {
            Choice {
                label: group.name.clone(),
                link: Selection {
                    group: Some(group.id),
                    ..selection
                }
                .filter_link(),
                selected: selection.group == Some(group.id),
            }
        }))
        .collect()
}

/// Whether `used` exceeds `allowance` by more than rounding.
fn overspent(used: Duration, allowance: Duration) -> bool {
    used >= allowance + Duration::from_mins(1)
}

fn stats(
    sessions: &[Session],
    all: &[Session],
    period: &Period,
    days: &[report::DayUsage],
    allowances: &[Vec<Duration>],
    names: &Names<'_>,
) -> Vec<Stat> {
    let total = report::total(sessions, period);
    let before = report::total(all, &period.previous());
    let length = u32::try_from(period.days.len()).unwrap_or(1).max(1);
    let change = if before.is_zero() {
        "в прошлом периоде времени не было".to_owned()
    } else {
        let ratio = total.as_secs_f64() / before.as_secs_f64() - 1.0;
        // A percentage of a ratio of durations stays far within i64.
        #[allow(clippy::cast_possible_truncation, reason = "bounded")]
        let percent = (ratio * 100.0).round() as i64;
        match percent {
            0 => "как в прошлый период".to_owned(),
            p if p > 0 => format!("на {p}% больше прошлого периода"),
            p => format!("на {}% меньше прошлого периода", p.unsigned_abs()),
        }
    };
    let over_days = days
        .iter()
        .enumerate()
        .filter(|(index, day)| {
            day.by_group
                .iter()
                .zip(allowances)
                .any(|(used, allowance)| overspent(*used, allowance[*index]))
        })
        .count();
    let longest = report::longest(sessions);
    vec![
        Stat {
            label: "Всего",
            value: format::duration(total),
            note: change,
            alert: false,
        },
        Stat {
            label: "В среднем за день",
            value: format::duration(total / length),
            note: format!("за {} дн.", period.days.len()),
            alert: false,
        },
        Stat {
            label: "Самый долгий сеанс",
            value: longest.map_or_else(|| "—".to_owned(), |s| format::duration(s.duration)),
            note: longest.map_or_else(String::new, |s| {
                format!("{}, {}", format::day(s.day), names.group(s.group))
            }),
            alert: false,
        },
        Stat {
            label: "Дней с перерасходом",
            value: over_days.to_string(),
            note: if over_days == 0 {
                "в рамках нормы".to_owned()
            } else {
                "обычно из-за работы без связи".to_owned()
            },
            alert: over_days > 0,
        },
    ]
}

fn daily_chart(
    days: &[report::DayUsage],
    shown: &[&groups::Group],
    allowances: &[Vec<Duration>],
    names: &Names<'_>,
    selection: Selection,
) -> DailyChart {
    let series: Vec<Series<'_>> = shown
        .iter()
        .map(|group| Series {
            name: &group.name,
            class: names.class(group.id),
        })
        .collect();
    let single = (shown.len() == 1).then(|| allowances[0].as_slice());
    let (scale, columns) = charts::columns(days, &series, single, selection.day);
    let legend = shown
        .iter()
        .enumerate()
        .map(|(index, group)| LegendItem {
            name: group.name.clone(),
            class: names.class(group.id),
            total: format::duration(days.iter().map(|day| day.by_group[index]).sum()),
        })
        .collect();
    DailyChart {
        scale,
        columns: columns
            .into_iter()
            .map(|column| {
                let link = Selection {
                    day: column.day,
                    ..selection
                }
                .link();
                (column, link)
            })
            .collect(),
        legend,
        shows_allowance: single.is_some(),
    }
}

fn day_detail(
    sessions: &[&Session],
    calendar: &Calendar,
    today: BudgetDay,
    names: &Names<'_>,
    selection: Selection,
) -> DayDetail {
    let zone = calendar.time_zone();
    let day_minutes = 24.0 * 60.0;
    let mut devices: Vec<DeviceId> = sessions.iter().map(|s| s.device).collect();
    devices.sort();
    devices.dedup();
    let describe = |session: &Session| {
        format!(
            "{} · {}\n{}–{} · {}{}",
            names.group(session.group),
            names.device(session.device),
            format::clock(session.start, zone),
            format::clock(report::end_of(session), zone),
            format::duration(session.duration),
            if session.offline {
                "\nбез связи"
            } else {
                ""
            }
        )
    };
    let lanes = devices
        .iter()
        .map(|&device| Lane {
            name: names.device(device).to_owned(),
            bars: sessions
                .iter()
                .filter(|s| s.device == device)
                .map(|session| {
                    let (start, length) = report::offset_in_day(session, calendar);
                    TimelineBar {
                        class: names.class(session.group),
                        from: Percent::of(start, day_minutes),
                        width: Percent::of(length, day_minutes),
                        tip: describe(session),
                    }
                })
                .collect(),
        })
        .collect();
    let step = |days: i64| {
        let date = selection.day.date().checked_add(days.days()).ok()?;
        let day = BudgetDay::new(date);
        (day <= today).then(|| Selection { day, ..selection }.link())
    };
    let day_start = calendar.day_start();
    DayDetail {
        title: format::long_day(selection.day),
        total: format::duration(sessions.iter().map(|s| s.duration).sum()),
        previous: step(-1),
        next: step(1),
        ticks: (0..=8)
            .map(|tick| day_start.plus_minutes(tick * 3 * 60).to_string())
            .collect(),
        lanes,
        sessions: sessions
            .iter()
            .map(|session| SessionRow {
                start: format::clock(session.start, zone),
                end: format::clock(report::end_of(session), zone),
                device: names.device(session.device).to_owned(),
                group: names.group(session.group).to_owned(),
                class: names.class(session.group),
                duration: format::duration(session.duration),
                offline: session.offline,
            })
            .collect(),
    }
}

/// The daily chart as a table, newest day first.
fn table(days: &[report::DayUsage], allowances: &[Vec<Duration>]) -> Vec<TableRow> {
    days.iter()
        .enumerate()
        .rev()
        .map(|(index, day)| {
            let total = day.total();
            let allowance: Option<Duration> =
                (!allowances.is_empty()).then(|| allowances.iter().map(|days| days[index]).sum());
            let overspend: Duration = day
                .by_group
                .iter()
                .zip(allowances)
                .map(|(used, allowance)| used.saturating_sub(allowance[index]))
                .filter(|over| *over >= Duration::from_mins(1))
                .sum();
            TableRow {
                label: format::day(day.day),
                used: format::duration(total),
                allowance: allowance.map(format::duration),
                overspend: (!overspend.is_zero()).then(|| format::duration(overspend)),
                offline: day.offline,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_keep_the_filters() {
        let selection = Selection {
            period: 30,
            group: Some(GroupId::new(2)),
            day: BudgetDay::new(jiff::civil::date(2026, 11, 7)),
        };
        assert_eq!(
            selection.link(),
            "/history?period=30&group=2&day=2026-11-07"
        );
        assert_eq!(selection.filter_link(), "/history?period=30&group=2");
    }

    #[test]
    fn overspend_ignores_rounding() {
        let hour = Duration::from_mins(60);
        assert!(!overspent(hour + Duration::from_secs(59), hour));
        assert!(overspent(hour + Duration::from_mins(1), hour));
    }
}
