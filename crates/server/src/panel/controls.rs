//! Quick actions on a group today: more or less time, an exact amount
//! left, a block.

use std::time::Duration;

use axum::Form;
use axum::extract::{Path, State};
use axum::response::Response;
use jiff::Timestamp;
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Adjustment;
use parentcontrol_core::units::{MinuteDelta, Minutes};
use serde::Deserialize;

use super::{back, done, rejected, status};
use crate::auth::Parent;
use crate::db::groups;
use crate::error::AppError;
use crate::state::AppState;

/// Where the actions are offered.
const PAGE: &str = "/";

/// The largest change and the most time left that can be set at once: a
/// whole budget day.
const MAX_MINUTES: u32 = 24 * 60;

#[derive(Debug, Deserialize)]
pub(super) struct AdjustForm {
    /// Minutes to add, negative to take away.
    minutes: i32,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemainingForm {
    /// Minutes the child should have left today.
    minutes: u32,
}

/// What a request asks to do with today's allowance.
#[derive(Debug, Clone, Copy)]
enum Request {
    Change(MinuteDelta),
    Remaining(Minutes),
}

/// Adds or takes away time for today.
pub(super) async fn adjust(
    state: State<AppState>,
    _: Parent,
    group: Path<GroupId>,
    Form(form): Form<AdjustForm>,
) -> Result<Response, AppError> {
    if form.minutes == 0 || form.minutes.unsigned_abs() > MAX_MINUTES {
        return Ok(rejected(
            PAGE,
            "Изменение должно быть от 1 минуты до 24 часов.",
        ));
    }
    apply(
        state,
        group,
        Request::Change(MinuteDelta::new(form.minutes)),
    )
    .await
}

/// Sets how much time is left today.
pub(super) async fn set_remaining(
    state: State<AppState>,
    _: Parent,
    group: Path<GroupId>,
    Form(form): Form<RemainingForm>,
) -> Result<Response, AppError> {
    if form.minutes > MAX_MINUTES {
        return Ok(rejected(PAGE, "Остаток не может быть больше 24 часов."));
    }
    apply(state, group, Request::Remaining(Minutes::new(form.minutes))).await
}

/// What [`apply`] did.
enum Outcome {
    Applied { name: String, summary: String },
    Unchanged,
    NoGroup,
}

async fn apply(
    State(state): State<AppState>,
    Path(group): Path<GroupId>,
    request: Request,
) -> Result<Response, AppError> {
    let now = Timestamp::now();
    let outcome = state
        .db
        .call(move |c| {
            let today = status::today(c, now)?;
            let Some(current) = today.groups.iter().find(|g| g.group.id == group) else {
                return Ok(Outcome::NoGroup);
            };
            let allowance = whole_minutes(current.allowance);
            let target = match request {
                Request::Change(delta) => delta.apply(allowance),
                Request::Remaining(left) => remaining_target(current.used, left),
            };
            let delta = MinuteDelta::between(allowance, target);
            if delta == MinuteDelta::ZERO {
                return Ok(Outcome::Unchanged);
            }
            groups::add_adjustment(
                c,
                group,
                Adjustment {
                    day: today.day,
                    minutes: delta,
                },
            )?;
            Ok(Outcome::Applied {
                name: current.group.name.clone(),
                summary: describe(request, delta),
            })
        })
        .await?;
    match outcome {
        Outcome::Applied { name, summary } => {
            done(&state, PAGE, format!("{name}: {summary}")).await
        }
        Outcome::Unchanged => Ok(back(PAGE)),
        Outcome::NoGroup => Ok(rejected(PAGE, "Группа не найдена.")),
    }
}

/// The allowance that leaves `left` after `used`, counting the used time to
/// the nearest minute as the panel shows it.
fn remaining_target(used: Duration, left: Minutes) -> Minutes {
    Minutes::new(whole_minutes(used).get().saturating_add(left.get()))
}

/// `duration` rounded to the nearest minute.
fn whole_minutes(duration: Duration) -> Minutes {
    Minutes::new(u32::try_from((duration.as_secs() + 30) / 60).unwrap_or(u32::MAX))
}

/// The entry in the parent's action log.
fn describe(request: Request, delta: MinuteDelta) -> String {
    let change = signed(delta);
    match request {
        Request::Change(_) => format!("{change} на сегодня"),
        Request::Remaining(left) => {
            format!("на сегодня осталось {} мин ({change})", left.get())
        }
    }
}

/// `+15 мин`, `−15 мин`, with a real minus sign.
fn signed(delta: MinuteDelta) -> String {
    let minutes = delta.get();
    let sign = if minutes < 0 { '−' } else { '+' };
    format!("{sign}{} мин", minutes.unsigned_abs())
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
        return Ok(rejected(PAGE, "Группа не найдена."));
    };
    let verb = if blocked {
        "заблокировано"
    } else {
        "разблокировано"
    };
    done(&state, PAGE, format!("{name}: {verb}")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_counts_from_used_time_as_shown() {
        let used = Duration::from_secs(40 * 60 + 31);
        assert_eq!(remaining_target(used, Minutes::new(20)), Minutes::new(61));
        assert_eq!(
            remaining_target(Duration::ZERO, Minutes::ZERO),
            Minutes::ZERO
        );
    }

    #[test]
    fn log_entries_show_the_sign() {
        let less = MinuteDelta::new(-15);
        assert_eq!(describe(Request::Change(less), less), "−15 мин на сегодня");
        let more = MinuteDelta::new(25);
        assert_eq!(
            describe(Request::Remaining(Minutes::new(30)), more),
            "на сегодня осталось 30 мин (+25 мин)"
        );
    }
}
