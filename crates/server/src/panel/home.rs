//! The main page: what is left today, who plays now, quick actions.

use std::collections::HashMap;
use std::time::Duration;

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use jiff::Timestamp;
use parentcontrol_core::budget::{Availability, DenyReason, StopReason};
use parentcontrol_core::ids::{DeviceId, GroupId};
use parentcontrol_core::policy::Bonus;
use parentcontrol_core::units::Minutes;
use serde::Deserialize;

use super::status::{self, GroupToday};
use super::{Feedback, Layout, back, changed, format, render};
use crate::auth::Parent;
use crate::db::{devices, groups, settings};
use crate::error::AppError;
use crate::state::AppState;

/// A device that has not reported for this long is shown as lost.
pub(super) const STALE_AFTER: Duration = Duration::from_hours(1);

/// Bonuses the panel offers.
const BONUS_CHOICES: [u32; 3] = [15, 30, 60];

struct GroupCard {
    id: GroupId,
    name: String,
    remaining: String,
    used: String,
    allowance: String,
    state: String,
    tone: &'static str,
    blocked: bool,
}

struct PlayingRow {
    device: String,
    groups: String,
}

struct DeviceRow {
    name: String,
    online: bool,
    seen: String,
    stale: bool,
    configured: bool,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    layout: Layout,
    today: String,
    groups: Vec<GroupCard>,
    playing: Vec<PlayingRow>,
    devices: Vec<DeviceRow>,
    bonuses: [u32; 3],
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    Query(feedback): Query<Feedback>,
) -> Result<Response, AppError> {
    let now = Timestamp::now();
    let (today, stored) = state
        .db
        .call(move |c| Ok((status::today(c, now)?, devices::list(c)?)))
        .await?;
    let live = state.hub.live();
    let zone = today.calendar.time_zone();

    let names: HashMap<GroupId, &str> = today
        .groups
        .iter()
        .map(|g| (g.group.id, g.group.name.as_str()))
        .collect();
    let device_names: HashMap<DeviceId, &str> =
        stored.iter().map(|d| (d.id, d.name.as_str())).collect();
    let mut playing: Vec<PlayingRow> = live
        .iter()
        .filter(|(_, live)| !live.active_groups.is_empty())
        .map(|(device, live)| PlayingRow {
            device: device_names.get(device).copied().unwrap_or("?").to_owned(),
            groups: live
                .active_groups
                .iter()
                .map(|g| names.get(g).copied().unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    playing.sort_by(|a, b| a.device.cmp(&b.device));

    let devices = stored
        .iter()
        .map(|device| DeviceRow {
            name: device.name.clone(),
            online: live.contains_key(&device.id),
            seen: device
                .last_seen
                .map_or_else(|| "никогда".to_owned(), |seen| format::ago(seen, now)),
            stale: device
                .last_seen
                .is_none_or(|seen| now.duration_since(seen).unsigned_abs() > STALE_AFTER),
            configured: !device.bindings.is_empty(),
        })
        .collect();

    let page = HomePage {
        layout: Layout {
            title: "Главная",
            active: "/",
            error: feedback.error,
        },
        today: format::day(today.day),
        groups: today.groups.iter().map(|group| card(group, zone)).collect(),
        playing,
        devices,
        bonuses: BONUS_CHOICES,
    };
    Ok(render(&page)?.into_response())
}

fn card(today: &GroupToday, zone: &jiff::tz::TimeZone) -> GroupCard {
    let now = Timestamp::now();
    let (state, tone) = match today.status.availability {
        Availability::Allowed { stop_in, reason } => {
            let until = now
                .checked_add(stop_in)
                .map(|end| format::clock(end, zone))
                .unwrap_or_default();
            let reason = match reason {
                StopReason::QuotaEnd => "до конца квоты",
                StopReason::WindowEnd => "до конца окна",
            };
            (format!("Можно играть до {until} ({reason})"), "ok")
        }
        Availability::Denied(DenyReason::Blocked) => ("Заблокировано".to_owned(), "bad"),
        Availability::Denied(DenyReason::QuotaExhausted) => {
            ("Квота на сегодня исчерпана".to_owned(), "warn")
        }
        Availability::Denied(DenyReason::OutsideWindow) => match today.status.next_window {
            Some(next) => (format!("Вне окна, откроется в {next}"), "warn"),
            None => ("Вне окна, сегодня больше не откроется".to_owned(), "warn"),
        },
    };
    GroupCard {
        id: today.group.id,
        name: today.group.name.clone(),
        remaining: format::duration(today.status.remaining),
        used: format::duration(today.used),
        allowance: format::duration(today.allowance),
        state,
        tone,
        blocked: today.group.blocked_since.is_some(),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct BonusForm {
    minutes: u32,
}

pub(super) async fn bonus(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    Form(form): Form<BonusForm>,
) -> Result<Response, AppError> {
    if !(1..=24 * 60).contains(&form.minutes) {
        return Ok(back("/", Some("Бонус должен быть от 1 минуты до 24 часов.")).into_response());
    }
    let minutes = Minutes::new(form.minutes);
    let name = state
        .db
        .call(move |c| {
            let day = settings::required_calendar(c)?.locate(Timestamp::now()).day;
            let name = groups::name(c, group)?;
            if name.is_some() {
                groups::add_bonus(c, group, Bonus { day, minutes })?;
            }
            Ok(name)
        })
        .await?;
    let Some(name) = name else {
        return Ok(back("/", Some("Группа не найдена.")).into_response());
    };
    changed(&state, format!("Бонус +{} мин: {name}", minutes.get())).await?;
    Ok(back("/", None).into_response())
}

pub(super) async fn block(
    state: State<AppState>,
    parent: Parent,
    group: Path<GroupId>,
) -> Result<Response, AppError> {
    set_blocked(state, parent, group, true).await
}

pub(super) async fn unblock(
    state: State<AppState>,
    parent: Parent,
    group: Path<GroupId>,
) -> Result<Response, AppError> {
    set_blocked(state, parent, group, false).await
}

async fn set_blocked(
    State(state): State<AppState>,
    _: Parent,
    Path(group): Path<GroupId>,
    blocked: bool,
) -> Result<Response, AppError> {
    let since = blocked.then(Timestamp::now);
    let name = state
        .db
        .call(move |c| {
            let name = groups::name(c, group)?;
            groups::set_blocked(c, group, since)?;
            Ok(name)
        })
        .await?;
    let Some(name) = name else {
        return Ok(back("/", Some("Группа не найдена.")).into_response());
    };
    let verb = if blocked {
        "Блокировка"
    } else {
        "Разблокировка"
    };
    changed(&state, format!("{verb}: {name}")).await?;
    Ok(back("/", None).into_response())
}
