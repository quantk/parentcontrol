use parentcontrol_core::units::Minutes;

use super::*;

fn group(state: TrayState) -> TrayStatus {
    TrayStatus {
        restricted: true,
        groups: vec![TrayGroup {
            name: "Игры".to_owned(),
            remaining: Duration::from_mins(40),
            state,
        }],
    }
}

#[test]
fn unrestricted_session_says_so() {
    assert_eq!(status_lines(&TrayStatus::default()), ["Ограничений нет"]);
}

#[test]
fn every_state_has_a_line() {
    let cases = [
        (
            TrayState::Allowed {
                stop_in: Duration::from_secs(61),
                reason: StopReason::QuotaEnd,
            },
            "Игры: осталось 2 мин",
        ),
        (
            TrayState::Allowed {
                stop_in: Duration::from_mins(65),
                reason: StopReason::WindowEnd,
            },
            "Игры: ещё 1 ч 05 мин, потом перерыв",
        ),
        (
            TrayState::Denied {
                reason: DenyReason::Blocked,
                next_window: None,
            },
            "Игры: заблокировано",
        ),
        (
            TrayState::Denied {
                reason: DenyReason::QuotaExhausted,
                next_window: None,
            },
            "Игры: на сегодня всё",
        ),
        (
            TrayState::Denied {
                reason: DenyReason::OutsideWindow,
                next_window: Some("16:00".parse().unwrap()),
            },
            "Игры: можно с 16:00, будет 40 мин",
        ),
        (
            TrayState::Denied {
                reason: DenyReason::OutsideWindow,
                next_window: None,
            },
            "Игры: сегодня больше нельзя",
        ),
    ];
    for (state, line) in cases {
        assert_eq!(status_lines(&group(state)), [line]);
    }
}

#[test]
fn warning_names_the_minutes_and_the_reason() {
    let message = notice_message(&TrayNotice::Warning {
        group: "Игры".to_owned(),
        left: Minutes::new(5),
        reason: StopReason::WindowEnd,
    });
    assert_eq!(message.summary, "Осталось 5 мин");
    assert!(message.body.contains("перерыв"));
}

#[test]
fn closing_explains_why_and_asks_to_save() {
    let message = notice_message(&TrayNotice::Closing {
        group: "Игры".to_owned(),
        reason: DenyReason::Blocked,
    });
    assert!(message.body.starts_with("Заблокировано родителем."));
    assert!(message.body.contains("сохранитесь"));
}
