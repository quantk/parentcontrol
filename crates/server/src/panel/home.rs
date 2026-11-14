//! The main page: what is left today, the last week, who plays now, and
//! quick changes of today's time.
//!
//! The page refreshes its status parts on its own (see `home.html`); the
//! forms are left alone so that typing is never interrupted.

use std::collections::HashMap;
use std::time::Duration;

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use jiff::{Timestamp, ToSpan};
use parentcontrol_core::budget::{Availability, DenyReason, StopReason};
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::GroupId;

use super::presence::Presence;
use super::status::{self, GroupToday};
use super::{Flash, Icon, Layout, format, render};
use crate::auth::Parent;
use crate::db::{devices, usage};
use crate::error::AppError;
use crate::state::AppState;

/// Changes of today's time offered as buttons, in minutes.
const ADJUSTMENTS: [i32; 4] = [-15, 15, 30, 60];

/// Days in the small chart of each group, today included.
const WEEK: i64 = 7;

/// How full today's allowance is, for the color of the meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Low,
    High,
    Full,
    Blocked,
}

impl Fill {
    fn of(used: Duration, allowance: Duration, blocked: bool) -> Self {
        if blocked {
            return Self::Blocked;
        }
        if used >= allowance {
            Self::Full
        } else if used.as_secs_f64() >= allowance.as_secs_f64() * 0.75 {
            Self::High
        } else {
            Self::Low
        }
    }

    const fn class(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Full => "full",
            Self::Blocked => "blocked",
        }
    }
}

/// Whether the group may be used now, in words.
struct StateLine {
    badge: &'static str,
    tone: &'static str,
    detail: String,
}

/// One day in a group's small chart.
struct WeekBar {
    label: &'static str,
    tip: String,
    /// Height relative to the busiest day of the week, in percent.
    height: u32,
    today: bool,
}

struct GroupCard {
    id: GroupId,
    name: String,
    remaining: String,
    /// What the "time left" field starts with.
    remaining_minutes: u64,
    used: String,
    allowance: String,
    /// Share of today's allowance used, in percent, at most 100.
    used_percent: u32,
    fill: &'static str,
    state: StateLine,
    blocked: bool,
    week: Vec<WeekBar>,
    week_total: String,
}

struct DeviceRow {
    name: String,
    badge: &'static str,
    tone: &'static str,
    detail: String,
    /// Enrolled, but no child's account is bound yet.
    unconfigured: bool,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    layout: Layout,
    today: String,
    groups: Vec<GroupCard>,
    devices: Vec<DeviceRow>,
    adjustments: [i32; 4],
    /// Elements the page refreshes, as an htmx selector.
    live_parts: String,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    flash: Option<Flash>,
) -> Result<Response, AppError> {
    let now = Timestamp::now();
    let (today, week, stored) = state
        .db
        .call(move |c| {
            let today = status::today(c, now)?;
            let first = BudgetDay::new(
                today
                    .day
                    .date()
                    .checked_sub((WEEK - 1).days())
                    .unwrap_or(today.day.date()),
            );
            let week = usage::totals(c, first, today.day)?;
            Ok((today, week, devices::list(c)?))
        })
        .await?;
    let live = state.hub.live();
    let zone = today.calendar.time_zone();

    let names: HashMap<GroupId, &str> = today
        .groups
        .iter()
        .map(|g| (g.group.id, g.group.name.as_str()))
        .collect();
    let devices = stored
        .iter()
        .map(|device| {
            let presence = Presence::of(device, live.get(&device.id), now, |group| {
                names.get(&group).copied().unwrap_or("удалённая группа")
            });
            DeviceRow {
                name: device.name.clone(),
                badge: presence.label(),
                tone: presence.tone(),
                detail: presence.detail(),
                unconfigured: device.enrolled && device.bindings.is_empty(),
            }
        })
        .collect();

    let groups: Vec<GroupCard> = today
        .groups
        .iter()
        .map(|group| card(group, today.day, &week, now, zone))
        .collect();
    let live_parts = groups
        .iter()
        .map(|group| format!("#status-{}", group.id))
        .chain(["#devices".to_owned()])
        .collect::<Vec<_>>()
        .join(", ");

    let page = HomePage {
        layout: Layout {
            title: "Главная",
            active: "/",
            flash,
        },
        today: format::long_day(today.day),
        groups,
        devices,
        adjustments: ADJUSTMENTS,
        live_parts,
    };
    Ok(render(&page)?.into_response())
}

fn card(
    today: &GroupToday,
    day: BudgetDay,
    week: &[usage::DayTotal],
    now: Timestamp,
    zone: &jiff::tz::TimeZone,
) -> GroupCard {
    let blocked = today.group.blocked_since.is_some();
    let used_percent = if today.allowance.is_zero() {
        100
    } else {
        let share = today.used.as_secs_f64() / today.allowance.as_secs_f64();
        // Bounded to 0..=100 before the cast.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded"
        )]
        let percent = (share.min(1.0) * 100.0).round() as u32;
        percent
    };
    let (week, week_total) = week_bars(today.group.id, day, week);
    GroupCard {
        id: today.group.id,
        name: today.group.name.clone(),
        remaining: format::duration(today.status.remaining),
        remaining_minutes: today.status.remaining.as_secs() / 60,
        used: format::duration(today.used),
        allowance: format::duration(today.allowance),
        used_percent,
        fill: Fill::of(today.used, today.allowance, blocked).class(),
        state: state_line(today, now, zone),
        blocked,
        week,
        week_total,
    }
}

fn state_line(today: &GroupToday, now: Timestamp, zone: &jiff::tz::TimeZone) -> StateLine {
    match today.status.availability {
        Availability::Allowed { stop_in, reason } => {
            let until = now
                .checked_add(stop_in)
                .map(|end| format::clock(end, zone))
                .unwrap_or_default();
            let then = match reason {
                StopReason::QuotaEnd => "потом закончится время",
                StopReason::WindowEnd => "потом закроется окно",
            };
            StateLine {
                badge: "Можно играть",
                tone: "ok",
                detail: format!("До {until}, {then}."),
            }
        }
        Availability::Denied(DenyReason::Blocked) => StateLine {
            badge: "Заблокировано",
            tone: "bad",
            detail: "Запуск запрещён, пока вы не снимете блокировку.".to_owned(),
        },
        Availability::Denied(DenyReason::QuotaExhausted) => StateLine {
            badge: "Время вышло",
            tone: "warn",
            detail: "Время на сегодня закончилось.".to_owned(),
        },
        Availability::Denied(DenyReason::OutsideWindow) => StateLine {
            badge: "Вне окна",
            tone: "warn",
            detail: match today.status.next_window {
                Some(next) => format!("Окно откроется в {next}."),
                None => "Сегодня окон больше нет.".to_owned(),
            },
        },
    }
}

/// The last [`WEEK`] days of `group` up to `today`, oldest first, and their
/// total.
fn week_bars(
    group: GroupId,
    today: BudgetDay,
    totals: &[usage::DayTotal],
) -> (Vec<WeekBar>, String) {
    let days: Vec<(BudgetDay, Duration)> = (0..WEEK)
        .rev()
        .filter_map(|back| today.date().checked_sub(back.days()).ok())
        .map(BudgetDay::new)
        .map(|day| {
            let used = totals
                .iter()
                .filter(|total| total.group == group && total.day == day)
                .map(|total| total.used)
                .sum();
            (day, used)
        })
        .collect();
    let busiest = days
        .iter()
        .map(|(_, used)| *used)
        .max()
        .unwrap_or_default()
        .max(Duration::from_mins(1));
    let bars = days
        .iter()
        .map(|&(day, used)| {
            let share = used.as_secs_f64() / busiest.as_secs_f64();
            // A share of the busiest day, so at most 100.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "bounded"
            )]
            let height = (share * 100.0).round() as u32;
            WeekBar {
                label: format::weekday_short(day.weekday()),
                tip: format!("{}\n{}", format::day(day), format::duration(used)),
                height,
                today: day == today,
            }
        })
        .collect();
    let total = days.iter().map(|(_, used)| *used).sum();
    (bars, format::duration(total))
}

#[cfg(test)]
mod tests {
    use parentcontrol_core::ids::DeviceId;

    use super::*;

    #[test]
    fn the_meter_warns_before_the_end() {
        let hour = Duration::from_mins(60);
        assert_eq!(Fill::of(Duration::from_mins(30), hour, false), Fill::Low);
        assert_eq!(Fill::of(Duration::from_mins(50), hour, false), Fill::High);
        assert_eq!(Fill::of(hour, hour, false), Fill::Full);
        assert_eq!(Fill::of(Duration::ZERO, Duration::ZERO, false), Fill::Full);
        assert_eq!(Fill::of(Duration::ZERO, hour, true), Fill::Blocked);
    }

    #[test]
    fn the_week_is_scaled_to_its_busiest_day() {
        let today = BudgetDay::new(jiff::civil::date(2026, 11, 14));
        let games = GroupId::new(1);
        let total = |back: i64, minutes: u64, device: i64| usage::DayTotal {
            day: BudgetDay::new(today.date().checked_sub(back.days()).unwrap()),
            group: games,
            device: DeviceId::new(device),
            used: Duration::from_mins(minutes),
            offline: false,
        };
        let totals = [
            total(0, 20, 1),
            total(0, 20, 2),
            total(3, 10, 1),
            total(9, 500, 1),
        ];
        let (bars, sum) = week_bars(games, today, &totals);
        assert_eq!(bars.len(), 7);
        assert!(bars[6].today);
        assert_eq!(bars[6].height, 100, "both devices add up");
        assert_eq!(bars[3].height, 25);
        assert_eq!(bars[0].height, 0);
        assert_eq!(sum, "50 мин", "days outside the week are ignored");
    }
}
