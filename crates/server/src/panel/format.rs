//! Russian presentation of times and durations for the panel.

use std::fmt;
use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use parentcontrol_core::calendar::BudgetDay;

/// A share written as a percentage with one decimal, for inline styles
/// that size chart marks.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub(super) struct Percent(f64);

impl Percent {
    /// `part` of `whole`, clamped to `0%..=100%`; zero when `whole` is.
    pub(super) fn of(part: f64, whole: f64) -> Self {
        if whole > 0.0 {
            Self((part / whole * 100.0).clamp(0.0, 100.0))
        } else {
            Self(0.0)
        }
    }

    /// `part` of `whole`.
    pub(super) fn of_duration(part: Duration, whole: Duration) -> Self {
        Self::of(part.as_secs_f64(), whole.as_secs_f64())
    }

    /// The share in `0.0..=1.0`.
    pub(super) fn fraction(self) -> f64 {
        self.0 / 100.0
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}%", self.0)
    }
}

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

/// `суббота, 14 ноября`.
pub(super) fn long_day(day: BudgetDay) -> String {
    let date = day.date();
    format!(
        "{}, {} {}",
        weekday(day.weekday()).to_lowercase(),
        date.day(),
        month_genitive(date.month())
    )
}

/// Russian month name in the genitive, as used after a day number.
const fn month_genitive(month: i8) -> &'static str {
    match month {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        _ => "декабря",
    }
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

pub(super) const fn weekday_short(weekday: Weekday) -> &'static str {
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
    fn percentages_are_clamped() {
        assert_eq!(Percent::of(1.0, 4.0).to_string(), "25.0%");
        assert_eq!(Percent::of(5.0, 4.0).to_string(), "100.0%");
        assert_eq!(Percent::of(1.0, 0.0).to_string(), "0.0%");
    }

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
    fn long_day_reads_as_a_date() {
        assert_eq!(
            long_day(BudgetDay::new(jiff::civil::date(2026, 11, 14))),
            "суббота, 14 ноября"
        );
    }

    #[test]
    fn day_shows_weekday_and_date() {
        assert_eq!(
            day(BudgetDay::new(jiff::civil::date(2026, 11, 14))),
            "сб 14.11"
        );
    }
}
