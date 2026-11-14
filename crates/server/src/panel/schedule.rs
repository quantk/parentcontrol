//! Time allowed per day of week and the windows when it may be used, edited
//! one group at a time.

mod form;

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use parentcontrol_core::calendar::TimeOfDay;
use parentcontrol_core::ids::GroupId;

use self::form::{DayField, Segment};
use super::{Flash, Icon, Layout, back, done, render};
use crate::auth::Parent;
use crate::db::{DEFAULT_PROFILE, groups, settings};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/schedule";

/// Hours between the labels under the preview strips.
const TICK_HOURS: u16 = 6;

/// One day as the template shows it.
struct DayView {
    field: DayField,
    preview: Vec<Segment>,
}

struct GroupView {
    id: GroupId,
    name: String,
    days: Vec<DayView>,
}

#[derive(Template)]
#[template(path = "schedule.html")]
struct SchedulePage {
    layout: Layout,
    day_start: TimeOfDay,
    /// Clock times under the preview strips, from one day start to the next.
    ticks: Vec<TimeOfDay>,
    groups: Vec<GroupView>,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    flash: Option<Flash>,
) -> Result<Response, AppError> {
    show(&state, flash, None).await
}

/// Renders the page; `edited` replaces the stored week of one group.
async fn show(
    state: &AppState,
    flash: Option<Flash>,
    edited: Option<(GroupId, Vec<DayField>)>,
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
    let day_start = calendar.day_start();
    let groups = stored
        .into_iter()
        .map(|group| {
            let fields = match &edited {
                Some((id, days)) if *id == group.id => days.clone(),
                _ => form::stored_week(&group.schedule),
            };
            GroupView {
                id: group.id,
                name: group.name,
                days: fields
                    .into_iter()
                    .map(|field| DayView {
                        preview: field.preview(day_start),
                        field,
                    })
                    .collect(),
            }
        })
        .collect();
    let page = SchedulePage {
        layout: Layout {
            title: "Расписание",
            active: PAGE,
            flash,
        },
        day_start,
        ticks: (0..=24 / TICK_HOURS)
            .map(|tick| day_start.plus_minutes(tick * TICK_HOURS * 60))
            .collect(),
        groups,
    };
    Ok(render(&page)?.into_response())
}

pub(super) async fn save(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Result<Response, AppError> {
    let day_start = state
        .db
        .call(|c| settings::required_calendar(c))
        .await?
        .day_start();
    let days = form::submitted_week(&fields);
    let schedule = match form::parse_week(&days, day_start) {
        Ok(schedule) => schedule,
        // Shown at once with the edits kept, rather than redirected.
        Err(error) => return show(&state, Some(Flash::error(error)), Some((group, days))).await,
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
        return done(&state, PAGE, format!("Сохранено расписание: {name}")).await;
    }
    Ok(back(PAGE))
}
