//! Russian presentation of times and durations for the panel.

use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use parentcontrol_core::calendar::BudgetDay;

/// `1 ч 05 мин`, `25 мин`, `0 мин`, rounded to the nearest minute.
pub(super) fn duration(duration: Duration) -> String {
    let minutes = (duration.as_secs() + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} мин"),
        (hours, 0) => format!("{hours} ч"),
        (hours, minutes) => format!("{hours} ч {minutes:02} мин"),
    }
}

/// Wall-clock `HH:MM` of `moment` in `zone`.
pub(super) fn clock(moment: Timestamp, zone: &TimeZone) -> String {
    moment.to_zoned(zone.clone()).strftime("%H:%M").to_string()
}

/// `14.11 16:05` of `moment` in `zone`.
pub(super) fn date_time(moment: Timestamp, zone: &TimeZone) -> String {
    moment
        .to_zoned(zone.clone())
        .strftime("%d.%m %H:%M")
        .to_string()
}

/// `сб 14.11`.
pub(super) fn day(day: BudgetDay) -> String {
    format!(
        "{} {}",
        weekday_short(day.weekday()),
        day.date().strftime("%d.%m")
    )
}

/// How long ago `moment` was: `только что`, `5 мин назад`, `3 ч назад`.
pub(super) fn ago(moment: Timestamp, now: Timestamp) -> String {
    let seconds = now.as_second().saturating_sub(moment.as_second()).max(0);
    match seconds {
        0..60 => "только что".to_owned(),
        60..3600 => format!("{} мин назад", seconds / 60),
        3600..86400 => format!("{} ч назад", seconds / 3600),
        _ => format!("{} дн назад", seconds / 86400),
    }
}

/// Full Russian name of `weekday`.
pub(super) const fn weekday(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Понедельник",
        Weekday::Tuesday => "Вторник",
        Weekday::Wednesday => "Среда",
        Weekday::Thursday => "Четверг",
        Weekday::Friday => "Пятница",
        Weekday::Saturday => "Суббота",
        Weekday::Sunday => "Воскресенье",
    }
}

const fn weekday_short(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "пн",
        Weekday::Tuesday => "вт",
        Weekday::Wednesday => "ср",
        Weekday::Thursday => "чт",
        Weekday::Friday => "пт",
        Weekday::Saturday => "сб",
        Weekday::Sunday => "вс",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(Duration::ZERO), "0 мин");
        assert_eq!(duration(Duration::from_secs(29)), "0 мин");
        assert_eq!(duration(Duration::from_secs(31)), "1 мин");
        assert_eq!(duration(Duration::from_mins(60)), "1 ч");
        assert_eq!(duration(Duration::from_mins(65)), "1 ч 05 мин");
    }

    #[test]
    fn ago_picks_a_unit() {
        let now: Timestamp = "2026-11-14T12:00:00Z".parse().unwrap();
        let before = |seconds: i64| {
            now.checked_sub(jiff::SignedDuration::from_secs(seconds))
                .unwrap()
        };
        assert_eq!(ago(before(10), now), "только что");
        assert_eq!(ago(before(300), now), "5 мин назад");
        assert_eq!(ago(before(7200), now), "2 ч назад");
        assert_eq!(ago(before(200_000), now), "2 дн назад");
    }

    #[test]
    fn day_shows_weekday_and_date() {
        assert_eq!(
            day(BudgetDay::new(jiff::civil::date(2026, 11, 14))),
            "сб 14.11"
        );
    }
}
