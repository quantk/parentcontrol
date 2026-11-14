//! Usage by day, group and device; sessions; the parent's actions.

use std::collections::HashMap;
use std::time::Duration;

use askama::Template;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use jiff::{Timestamp, ToSpan};
use parentcontrol_core::calendar::BudgetDay;
use parentcontrol_core::ids::{DeviceId, GroupId};
use parentcontrol_core::policy::{self, Adjustment};
use serde::Deserialize;

use super::{Icon, Layout, format, render, status};
use crate::auth::Parent;
use crate::db::{DEFAULT_PROFILE, actions, devices, groups, settings, usage};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/history";

/// Days shown in the summary.
const DAYS: i32 = 14;

/// Parent actions shown.
const ACTIONS: u32 = 50;

struct Cell {
    group: String,
    used: String,
    allowance: String,
    /// Time used beyond the allowance, if any.
    overspend: Option<String>,
    offline: bool,
    devices: String,
}

struct DayRow {
    day: BudgetDay,
    label: String,
    cells: Vec<Cell>,
}

struct SessionRow {
    start: String,
    device: String,
    group: String,
    duration: String,
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
    days: Vec<DayRow>,
    selected: String,
    sessions: Vec<SessionRow>,
    actions: Vec<ActionRow>,
}

/// Everything the page reads from storage.
struct Stored {
    calendar: parentcontrol_core::calendar::Calendar,
    today: BudgetDay,
    selected: BudgetDay,
    groups: Vec<groups::Group>,
    adjustments: Vec<(GroupId, Adjustment)>,
    devices: Vec<devices::Device>,
    totals: Vec<usage::DayTotal>,
    sessions: Vec<usage::Session>,
    actions: Vec<actions::ParentAction>,
}

#[derive(Debug, Deserialize)]
pub(super) struct HistoryQuery {
    /// The day whose sessions to list; today by default.
    day: Option<jiff::civil::Date>,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, AppError> {
    let now = Timestamp::now();
    let data = state
        .db
        .call(move |c| {
            let calendar = settings::required_calendar(c)?;
            let today = calendar.locate(now).day;
            let first = BudgetDay::new(
                today
                    .date()
                    .checked_sub((DAYS - 1).days())
                    .unwrap_or(today.date()),
            );
            let selected = query.day.map_or(today, BudgetDay::new);
            Ok(Stored {
                groups: groups::list(c, DEFAULT_PROFILE)?,
                adjustments: groups::adjustments_since(c, first)?,
                devices: devices::list(c)?,
                totals: usage::totals(c, first, today)?,
                sessions: usage::sessions(c, selected)?,
                actions: actions::recent(c, ACTIONS)?,
                calendar,
                today,
                selected,
            })
        })
        .await?;
    let names = Names::of(&data);
    let zone = data.calendar.time_zone();
    let page = HistoryPage {
        layout: Layout {
            title: "История",
            active: PAGE,
            flash: None,
        },
        days: day_rows(&data, &names),
        selected: format::day(data.selected),
        sessions: data
            .sessions
            .iter()
            .map(|session| SessionRow {
                start: format::date_time(session.start, zone),
                device: names.device(session.device).to_owned(),
                group: names.group(session.group).to_owned(),
                duration: format::duration(session.duration),
                offline: session.offline,
            })
            .collect(),
        actions: data
            .actions
            .iter()
            .map(|action| ActionRow {
                at: format::date_time(action.at, zone),
                summary: action.summary.clone(),
            })
            .collect(),
    };
    Ok(render(&page)?.into_response())
}

/// Display names of groups and devices, with fallbacks for deleted ones.
struct Names<'a> {
    groups: HashMap<GroupId, &'a str>,
    devices: HashMap<DeviceId, &'a str>,
}

impl<'a> Names<'a> {
    fn of(data: &'a Stored) -> Self {
        Self {
            groups: data
                .groups
                .iter()
                .map(|g| (g.id, g.name.as_str()))
                .collect(),
            devices: data
                .devices
                .iter()
                .map(|d| (d.id, d.name.as_str()))
                .collect(),
        }
    }

    fn group(&self, id: GroupId) -> &'a str {
        self.groups.get(&id).copied().unwrap_or("удалённая группа")
    }

    fn device(&self, id: DeviceId) -> &'a str {
        self.devices
            .get(&id)
            .copied()
            .unwrap_or("удалённое устройство")
    }
}

/// One row per day, newest first, with a cell per group used that day.
fn day_rows(data: &Stored, names: &Names<'_>) -> Vec<DayRow> {
    let mut rows = Vec::new();
    let mut day = data.today;
    for _ in 0..DAYS {
        let cells = data
            .groups
            .iter()
            .filter_map(|group| cell(data, names, group, day))
            .collect();
        rows.push(DayRow {
            day,
            label: format::day(day),
            cells,
        });
        match day.date().yesterday() {
            Ok(previous) => day = BudgetDay::new(previous),
            Err(_) => break,
        }
    }
    rows
}

/// Usage of `group` during `day`, or `None` if it was not used.
fn cell(data: &Stored, names: &Names<'_>, group: &groups::Group, day: BudgetDay) -> Option<Cell> {
    let totals: Vec<_> = data
        .totals
        .iter()
        .filter(|total| total.day == day && total.group == group.id)
        .collect();
    if totals.is_empty() {
        return None;
    }
    let used: Duration = totals.iter().map(|total| total.used).sum();
    let adjustments = status::of_group(&data.adjustments, group.id);
    let allowance = policy::allowance(&group.schedule, &adjustments, day).to_duration();
    let overspend = used.saturating_sub(allowance);
    Some(Cell {
        group: group.name.clone(),
        used: format::duration(used),
        allowance: format::duration(allowance),
        // Under a minute is rounding between ticks and devices, not overspend.
        overspend: (overspend >= Duration::from_mins(1)).then(|| format::duration(overspend)),
        offline: totals.iter().any(|total| total.offline),
        devices: totals
            .iter()
            .map(|total| {
                format!(
                    "{}: {}",
                    names.device(total.device),
                    format::duration(total.used)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    })
}
