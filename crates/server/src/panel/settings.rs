//! Password, time zone and day start.

use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use jiff::tz::TimeZone;
use parentcontrol_core::calendar::{Calendar, TimeOfDay};
use parentcontrol_core::schedule::DaySchedule;
use serde::Deserialize;

use super::{Flash, Icon, Layout, back, changed, done, rejected, render};
use crate::auth::{Parent, PasswordError, hash_password, verify_password};
use crate::db::{DEFAULT_PROFILE, DbError, groups, sessions, settings};
use crate::error::AppError;
use crate::state::AppState;

const PAGE: &str = "/settings";

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    layout: Layout,
    time_zone: String,
    day_start: TimeOfDay,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    flash: Option<Flash>,
) -> Result<Response, AppError> {
    let calendar = state.db.call(|c| settings::required_calendar(c)).await?;
    let page = SettingsPage {
        layout: Layout {
            title: "Настройки",
            active: PAGE,
            flash,
        },
        time_zone: calendar.time_zone().iana_name().unwrap_or("UTC").to_owned(),
        day_start: calendar.day_start(),
    };
    Ok(render(&page)?.into_response())
}

#[derive(Debug, Deserialize)]
pub(super) struct PasswordForm {
    current: String,
    new: String,
    repeat: String,
}

pub(super) async fn password(
    State(state): State<AppState>,
    _: Parent,
    Form(form): Form<PasswordForm>,
) -> Result<Response, AppError> {
    if form.new != form.repeat {
        return Ok(rejected(PAGE, "Новые пароли не совпадают."));
    }
    let db = state.db.clone();
    let current = form.current;
    let verified = state
        .throttle
        .check(async move {
            let hash = db.call(|c| settings::password_hash(c)).await?;
            let verified = tokio::task::spawn_blocking(move || {
                hash.is_some_and(|hash| verify_password(&current, &hash))
            })
            .await
            .map_err(DbError::from)?;
            Ok::<_, DbError>(verified)
        })
        .await?;
    if !verified {
        return Ok(rejected(PAGE, "Текущий пароль неверен."));
    }
    let hash = match tokio::task::spawn_blocking(move || hash_password(&form.new))
        .await
        .map_err(DbError::from)?
    {
        Ok(hash) => hash,
        Err(PasswordError::TooShort) => {
            return Ok(rejected(PAGE, &PasswordError::TooShort.to_string()));
        }
        Err(error) => return Err(error.into()),
    };
    state
        .db
        .call(move |c| {
            settings::set_password_hash(c, &hash)?;
            // Other browsers stay logged in with the old password otherwise.
            sessions::delete_all(c)
        })
        .await?;
    changed(&state, "Изменён пароль".to_owned()).await?;
    Ok(back("/login"))
}

#[derive(Debug, Deserialize)]
pub(super) struct CalendarForm {
    time_zone: String,
    day_start: String,
}

pub(super) async fn calendar(
    State(state): State<AppState>,
    _: Parent,
    Form(form): Form<CalendarForm>,
) -> Result<Response, AppError> {
    let Ok(zone) = TimeZone::get(form.time_zone.trim()) else {
        return Ok(rejected(
            PAGE,
            "Неизвестный часовой пояс; нужен идентификатор вроде Europe/Moscow.",
        ));
    };
    let Ok(day_start) = form.day_start.trim().parse::<TimeOfDay>() else {
        return Ok(rejected(PAGE, "Начало суток нужно указать как ЧЧ:ММ."));
    };
    let calendar = Calendar::new(zone, day_start);
    let conflict = state
        .db
        .call(move |c| {
            // Windows are stored as clock times; a new day start may make
            // one of them cross the day boundary.
            let conflict = groups::list(c, DEFAULT_PROFILE)?.into_iter().find(|group| {
                group
                    .schedule
                    .days()
                    .flat_map(DaySchedule::windows)
                    .any(|window| window.span(day_start).is_none())
            });
            if conflict.is_none() {
                settings::set_calendar(c, &calendar)?;
            }
            Ok(conflict.map(|group| group.name))
        })
        .await?;
    if let Some(group) = conflict {
        let error = format!(
            "В расписании группы «{group}» есть окно, которое пересечёт новое начало суток. Сначала измените расписание."
        );
        return Ok(rejected(PAGE, &error));
    }
    done(
        &state,
        PAGE,
        format!(
            "Часовой пояс {}, начало суток {day_start}",
            form.time_zone.trim()
        ),
    )
    .await
}
