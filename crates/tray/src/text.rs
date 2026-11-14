//! What the child reads: status lines and notifications, in Russian.

use std::time::Duration;

use parentcontrol_core::budget::{DenyReason, StopReason};
use parentcontrol_core::tray::{TrayGroup, TrayNotice, TrayState, TrayStatus};

/// Name of the program as the child sees it.
pub(crate) const APP_NAME: &str = "Родительский контроль";

/// A notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) summary: String,
    pub(crate) body: String,
}

/// One line per limited group, or a single line saying nothing is limited.
pub(crate) fn status_lines(status: &TrayStatus) -> Vec<String> {
    if !status.restricted {
        return vec!["Ограничений нет".to_owned()];
    }
    if status.groups.is_empty() {
        return vec!["Ограничения не настроены".to_owned()];
    }
    status.groups.iter().map(group_line).collect()
}

fn group_line(group: &TrayGroup) -> String {
    let name = &group.name;
    match group.state {
        TrayState::Allowed {
            stop_in,
            reason: StopReason::QuotaEnd,
        } => {
            format!("{name}: осталось {}", duration(stop_in))
        }
        TrayState::Allowed {
            stop_in,
            reason: StopReason::WindowEnd,
        } => {
            format!("{name}: ещё {}, потом перерыв", duration(stop_in))
        }
        TrayState::Denied {
            reason: DenyReason::Blocked,
            ..
        } => format!("{name}: заблокировано"),
        TrayState::Denied {
            reason: DenyReason::QuotaExhausted,
            ..
        } => {
            format!("{name}: на сегодня всё")
        }
        TrayState::Denied {
            reason: DenyReason::OutsideWindow,
            next_window: Some(next),
        } => {
            format!(
                "{name}: можно с {next}, будет {}",
                duration(group.remaining)
            )
        }
        TrayState::Denied {
            reason: DenyReason::OutsideWindow,
            next_window: None,
        } => {
            format!("{name}: сегодня больше нельзя")
        }
    }
}

/// The notification for `notice`.
pub(crate) fn notice_message(notice: &TrayNotice) -> Message {
    match notice {
        TrayNotice::Warning {
            group,
            left,
            reason,
        } => Message {
            summary: format!("Осталось {} мин", left.get()),
            body: match reason {
                StopReason::QuotaEnd => {
                    format!("«{group}»: время на сегодня заканчивается. Сохранитесь заранее.")
                }
                StopReason::WindowEnd => format!("«{group}»: скоро перерыв. Сохранитесь заранее."),
            },
        },
        TrayNotice::Closing { group, reason } => Message {
            summary: format!("«{group}» закрывается"),
            body: format!(
                "{} Игра закроется через 30 секунд — сохранитесь.",
                deny_reason(*reason)
            ),
        },
        TrayNotice::LaunchDenied { group, reason } => Message {
            summary: format!("«{group}» сейчас недоступно"),
            body: deny_reason(*reason).to_owned(),
        },
        TrayNotice::CloseCancelled { group } => Message {
            summary: format!("«{group}» снова доступно"),
            body: "Закрытие отменено.".to_owned(),
        },
    }
}

fn deny_reason(reason: DenyReason) -> &'static str {
    match reason {
        DenyReason::Blocked => "Заблокировано родителем.",
        DenyReason::OutsideWindow => "Сейчас не время для игр.",
        DenyReason::QuotaExhausted => "Время на сегодня закончилось.",
    }
}

/// `1 ч 05 мин`, `25 мин`, rounded up so that a running clock never shows
/// zero while time is left.
fn duration(duration: Duration) -> String {
    let minutes = duration.as_secs().div_ceil(60);
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} мин"),
        (hours, 0) => format!("{hours} ч"),
        (hours, minutes) => format!("{hours} ч {minutes:02} мин"),
    }
}

#[cfg(test)]
mod tests;
