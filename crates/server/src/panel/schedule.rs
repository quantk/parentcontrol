//! Quota and windows per day of week.

use std::collections::HashMap;

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use jiff::civil::Weekday;
use parentcontrol_core::calendar::TimeOfDay;
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::schedule::{DaySchedule, MAX_DAILY_QUOTA, TimeWindow, WeekSchedule};
use parentcontrol_core::units::Minutes;

use super::{Feedback, Layout, back, changed, format, render};
use crate::auth::Parent;
use crate::db::{DEFAULT_PROFILE, groups, settings};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/schedule";

const WEEK: [(Weekday, &str); 7] = [
    (Weekday::Monday, "monday"),
    (Weekday::Tuesday, "tuesday"),
    (Weekday::Wednesday, "wednesday"),
    (Weekday::Thursday, "thursday"),
    (Weekday::Friday, "friday"),
    (Weekday::Saturday, "saturday"),
    (Weekday::Sunday, "sunday"),
];

struct DayRow {
    key: &'static str,
    label: &'static str,
    quota: u32,
    windows: String,
}

struct GroupSchedule {
    id: GroupId,
    name: String,
    days: Vec<DayRow>,
}

#[derive(Template)]
#[template(path = "schedule.html")]
struct SchedulePage {
    layout: Layout,
    day_start: TimeOfDay,
    groups: Vec<GroupSchedule>,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    Query(feedback): Query<Feedback>,
) -> Result<Response, AppError> {
    let (calendar, stored) = state
        .db
        .call(|c| {
            Ok((
                settings::required_calendar(c)?,
                groups::list(c, DEFAULT_PROFILE)?,
            ))
        })
        .await?;
    let groups = stored
        .into_iter()
        .map(|group| GroupSchedule {
            id: group.id,
            name: group.name,
            days: WEEK
                .iter()
                .map(|&(weekday, key)| {
                    let day = group.schedule.day(weekday);
                    DayRow {
                        key,
                        label: format::weekday(weekday),
                        quota: day.quota().get(),
                        windows: windows_text(day),
                    }
                })
                .collect(),
        })
        .collect();
    let page = SchedulePage {
        layout: Layout {
            title: "Расписание",
            active: PAGE,
            error: feedback.error,
        },
        day_start: calendar.day_start(),
        groups,
    };
    Ok(render(&page)?.into_response())
}

fn windows_text(day: &DaySchedule) -> String {
    day.windows()
        .iter()
        .map(|window| format!("{}-{}", window.start(), window.end()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parses `16:00-20:00, 21:00-22:00`; empty means no window restriction.
fn parse_windows(text: &str) -> Result<Vec<TimeWindow>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let invalid = || format!("Окно «{part}» не распознано, нужно ЧЧ:ММ-ЧЧ:ММ.");
            let (start, end) = part.split_once('-').ok_or_else(invalid)?;
            let start = start.trim().parse().map_err(|_| invalid())?;
            let end = end.trim().parse().map_err(|_| invalid())?;
            TimeWindow::new(start, end).map_err(|_| format!("Окно «{part}» пустое."))
        })
        .collect()
}

/// Builds a week from the form, checking windows against `day_start`.
fn parse_week(
    form: &HashMap<String, String>,
    day_start: TimeOfDay,
) -> Result<WeekSchedule, String> {
    let mut days = Vec::with_capacity(WEEK.len());
    for (weekday, key) in WEEK {
        let label = format::weekday(weekday);
        let field = |prefix: &str| {
            form.get(&format!("{prefix}_{key}"))
                .map_or("", String::as_str)
        };
        let quota: u32 = field("quota")
            .trim()
            .parse()
            .map_err(|_| format!("{label}: квота должна быть числом минут."))?;
        let windows =
            parse_windows(field("windows")).map_err(|error| format!("{label}: {error}"))?;
        if let Some(window) = windows
            .iter()
            .find(|window| window.span(day_start).is_none())
        {
            return Err(format!(
                "{label}: окно {}-{} пересекает начало суток {day_start}.",
                window.start(),
                window.end()
            ));
        }
        let day = DaySchedule::new(Minutes::new(quota), windows)
            .map_err(|_| format!("{label}: квота больше {} минут.", MAX_DAILY_QUOTA.get()))?;
        days.push(day);
    }
    let [
        monday,
        tuesday,
        wednesday,
        thursday,
        friday,
        saturday,
        sunday,
    ] = <[DaySchedule; 7]>::try_from(days).map_err(|_| "Неполное расписание.".to_owned())?;
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

pub(super) async fn save(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let day_start = state
        .db
        .call(|c| settings::required_calendar(c))
        .await?
        .day_start();
    let schedule = match parse_week(&form, day_start) {
        Ok(schedule) => schedule,
        Err(error) => return Ok(back(PAGE, Some(&error)).into_response()),
    };
    let name = state
        .db
        .call(move |c| {
            let name = groups::name(c, group)?;
            groups::set_schedule(c, group, &schedule)?;
            Ok(name)
        })
        .await?;
    if let Some(name) = name {
        changed(&state, format!("Изменено расписание: {name}")).await?;
    }
    Ok(back(PAGE, None).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(windows: &str) -> HashMap<String, String> {
        WEEK.iter()
            .flat_map(|(_, key)| {
                [
                    (format!("quota_{key}"), "60".to_owned()),
                    (format!("windows_{key}"), windows.to_owned()),
                ]
            })
            .collect()
    }

    fn four() -> TimeOfDay {
        "04:00".parse().unwrap()
    }

    #[test]
    fn parses_a_full_week() {
        let week = parse_week(&form("16:00-20:00, 21:00 - 22:30"), four()).unwrap();
        assert_eq!(week.friday.quota(), Minutes::new(60));
        assert_eq!(windows_text(&week.friday), "16:00-20:00, 21:00-22:30");
    }

    #[test]
    fn empty_windows_mean_no_restriction() {
        let week = parse_week(&form("  "), four()).unwrap();
        assert_eq!(week.monday.windows(), []);
    }

    #[test]
    fn rejects_windows_crossing_day_start() {
        let error = parse_week(&form("03:00-05:00"), four()).unwrap_err();
        assert!(error.contains("пересекает"), "{error}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_week(&form("вечером"), four()).is_err());
        let mut bad_quota = form("");
        bad_quota.insert("quota_sunday".to_owned(), "много".to_owned());
        assert!(
            parse_week(&bad_quota, four())
                .unwrap_err()
                .starts_with("Воскресенье")
        );
    }
}
