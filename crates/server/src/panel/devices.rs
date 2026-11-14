//! Devices: enrollment codes, bound accounts, health.

use std::time::Duration;

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::header::HOST;
use axum::response::{IntoResponse, Response};
use jiff::Timestamp;
use parentcontrol_core::ids::DeviceId;
use parentcontrol_core::process::OsUser;
use serde::Deserialize;

use super::presence::Presence;
use super::{Flash, Icon, Layout, back, changed, done, format, rejected, render, valid_name};
use crate::auth::Parent;
use crate::db::{DEFAULT_PROFILE, devices, groups, settings};
use crate::error::AppError;
use crate::secret;
use crate::state::AppState;

const PAGE: &str = "/devices";

/// Tells htmx which address to show for a swapped page.
const HX_PUSH_URL: &str = "hx-push-url";

/// How long an enrollment code stays valid.
const CODE_LIFETIME: Duration = Duration::from_mins(30);

struct DeviceView {
    id: DeviceId,
    name: String,
    platform: String,
    badge: &'static str,
    tone: &'static str,
    detail: String,
    enrolled: bool,
    bound: Vec<String>,
    unbound: Vec<String>,
}

#[derive(Template)]
#[template(path = "devices.html")]
struct DevicesPage {
    layout: Layout,
    devices: Vec<DeviceView>,
    fingerprint: String,
}

#[derive(Template)]
#[template(path = "enroll.html")]
struct EnrollPage {
    layout: Layout,
    name: String,
    code: String,
    expires: String,
    server: String,
    fingerprint: String,
}

pub(super) async fn page(
    State(state): State<AppState>,
    _: Parent,
    flash: Option<Flash>,
) -> Result<Response, AppError> {
    let now = Timestamp::now();
    let (stored, groups) = state
        .db
        .call(|c| Ok((devices::list(c)?, groups::list(c, DEFAULT_PROFILE)?)))
        .await?;
    let live = state.hub.live();
    let group_name = |id| {
        groups
            .iter()
            .find(|group| group.id == id)
            .map_or("удалённая группа", |group| {
                group.name.as_str()
            })
    };
    let devices = stored
        .into_iter()
        .map(|device| {
            let presence = Presence::of(&device, live.get(&device.id), now, group_name);
            DeviceView {
                badge: presence.label(),
                tone: presence.tone(),
                detail: presence.detail(),
                platform: device
                    .platform
                    .map_or_else(String::new, |platform| platform.to_string()),
                unbound: device
                    .local_users
                    .iter()
                    .filter(|user| !device.bindings.contains(*user))
                    .map(ToString::to_string)
                    .collect(),
                bound: device.bindings.iter().map(ToString::to_string).collect(),
                enrolled: device.enrolled,
                name: device.name,
                id: device.id,
            }
        })
        .collect();
    let page = DevicesPage {
        layout: Layout {
            title: "Устройства",
            active: PAGE,
            flash,
        },
        devices,
        fingerprint: grouped(&state.fingerprint),
    };
    Ok(render(&page)?.into_response())
}

/// `3f2a91bc…` as `3f2a 91bc …` for comparing by eye.
fn grouped(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Deserialize)]
pub(super) struct NameForm {
    name: String,
}

pub(super) async fn create(
    State(state): State<AppState>,
    _: Parent,
    headers: HeaderMap,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    let name = match valid_name(&form.name) {
        Ok(name) => name,
        Err(error) => return Ok(rejected(PAGE, error)),
    };
    let code = secret::enrollment_code()?;
    let (hash, expires) = (secret::hash(&secret::normalize_code(&code)), expires());
    let stored = name.clone();
    state
        .db
        .call(move |c| devices::create(c, &stored, &hash, expires))
        .await?;
    changed(&state, format!("Добавлено устройство: {name}")).await?;
    enroll_page(&state, name, code, expires, &headers).await
}

pub(super) async fn new_code(
    State(state): State<AppState>,
    _: Parent,
    Path(device): Path<DeviceId>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let code = secret::enrollment_code()?;
    let (hash, expires) = (secret::hash(&secret::normalize_code(&code)), expires());
    let name = state
        .db
        .call(move |c| {
            let name = devices::list(c)?
                .into_iter()
                .find(|d| d.id == device)
                .map(|d| d.name);
            devices::reissue_code(c, device, &hash, expires)?;
            Ok(name)
        })
        .await?;
    let Some(name) = name else {
        return Ok(rejected(PAGE, "Устройство не найдено."));
    };
    enroll_page(&state, name, code, expires, &headers).await
}

fn expires() -> Timestamp {
    Timestamp::now()
        .checked_add(CODE_LIFETIME)
        .unwrap_or(Timestamp::MAX)
}

async fn enroll_page(
    state: &AppState,
    name: String,
    code: String,
    expires: Timestamp,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let zone = state
        .db
        .call(|c| settings::required_calendar(c))
        .await?
        .time_zone()
        .clone();
    let host = headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .unwrap_or("localhost:8443");
    let page = EnrollPage {
        layout: Layout {
            title: "Подключение устройства",
            active: PAGE,
            flash: None,
        },
        name,
        code,
        expires: format::clock(expires, &zone),
        server: format!("https://{host}"),
        fingerprint: grouped(&state.fingerprint),
    };
    // The page answers a POST; keep the address of the device list so
    // that reloading does not resend the form.
    Ok(([(HX_PUSH_URL, "false")], render(&page)?).into_response())
}

pub(super) async fn rename(
    State(state): State<AppState>,
    _: Parent,
    Path(device): Path<DeviceId>,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    let name = match valid_name(&form.name) {
        Ok(name) => name,
        Err(error) => return Ok(rejected(PAGE, error)),
    };
    let stored = name.clone();
    if state
        .db
        .call(move |c| devices::rename(c, device, &stored))
        .await?
    {
        return done(&state, PAGE, format!("Устройство переименовано: {name}")).await;
    }
    Ok(back(PAGE))
}

pub(super) async fn delete(
    State(state): State<AppState>,
    _: Parent,
    Path(device): Path<DeviceId>,
) -> Result<Response, AppError> {
    if state.db.call(move |c| devices::delete(c, device)).await? {
        return done(&state, PAGE, "Удалено устройство".to_owned()).await;
    }
    Ok(back(PAGE))
}

#[derive(Debug, Deserialize)]
pub(super) struct UserForm {
    user: String,
}

pub(super) async fn bind(
    State(state): State<AppState>,
    _: Parent,
    Path(device): Path<DeviceId>,
    Form(form): Form<UserForm>,
) -> Result<Response, AppError> {
    let user = OsUser::new(form.user.trim());
    let known = state
        .db
        .call({
            let user = user.clone();
            move |c| {
                let reported = devices::list(c)?
                    .into_iter()
                    .any(|d| d.id == device && d.local_users.contains(&user));
                if reported {
                    devices::bind(c, device, &user, DEFAULT_PROFILE)?;
                }
                Ok(reported)
            }
        })
        .await?;
    if !known {
        return Ok(rejected(
            PAGE,
            "Такой учётной записи устройство не сообщало.",
        ));
    }
    done(&state, PAGE, format!("Привязана учётная запись {user}")).await
}

pub(super) async fn unbind(
    State(state): State<AppState>,
    _: Parent,
    Path(device): Path<DeviceId>,
    Form(form): Form<UserForm>,
) -> Result<Response, AppError> {
    let user = OsUser::new(form.user);
    let stored = user.clone();
    if state
        .db
        .call(move |c| devices::unbind(c, device, &stored))
        .await?
    {
        return done(&state, PAGE, format!("Отвязана учётная запись {user}")).await;
    }
    Ok(back(PAGE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_grouped_by_four() {
        assert_eq!(grouped("0123456789ab"), "0123 4567 89ab");
    }
}
