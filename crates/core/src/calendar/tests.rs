use jiff::civil::date;
use proptest::prelude::*;

use super::*;

fn moscow(day_start: TimeOfDay) -> Calendar {
    Calendar::new(TimeZone::get("Europe/Moscow").unwrap(), day_start)
}

fn at(rfc3339: &str) -> Timestamp {
    rfc3339.parse().unwrap()
}

fn four_am() -> TimeOfDay {
    TimeOfDay::new(4, 0).unwrap()
}

#[test]
fn time_of_day_parses_and_formats() {
    let time: TimeOfDay = "16:05".parse().unwrap();
    assert_eq!(time.minutes_since_midnight(), 16 * 60 + 5);
    assert_eq!(time.to_string(), "16:05");
}

#[test]
fn time_of_day_rejects_garbage() {
    for input in ["24:00", "12:60", "7:00", "12-00", "", "ab:cd", "12:5"] {
        assert!(input.parse::<TimeOfDay>().is_err(), "{input}");
    }
}

#[test]
fn minutes_after_wraps_around_midnight() {
    let start = four_am();
    assert_eq!(TimeOfDay::new(4, 0).unwrap().minutes_after(start), 0);
    assert_eq!(TimeOfDay::new(3, 59).unwrap().minutes_after(start), 1439);
    assert_eq!(TimeOfDay::MIDNIGHT.minutes_after(start), 20 * 60);
}

#[test]
fn moment_after_day_start_belongs_to_same_date() {
    // 10:00 MSK
    let position = moscow(four_am()).locate(at("2026-11-14T07:00:00Z"));
    assert_eq!(position.day, BudgetDay::new(date(2026, 11, 14)));
    assert_eq!(position.elapsed, Duration::from_hours(6));
}

#[test]
fn moment_before_day_start_belongs_to_previous_date() {
    // 02:30 MSK on the 15th is still the budget day of the 14th.
    let position = moscow(four_am()).locate(at("2026-11-14T23:30:00Z"));
    assert_eq!(position.day, BudgetDay::new(date(2026, 11, 14)));
    assert_eq!(position.elapsed, Duration::from_mins(22 * 60 + 30));
}

#[test]
fn day_start_itself_opens_the_day() {
    // 04:00 MSK
    let position = moscow(four_am()).locate(at("2026-11-15T01:00:00Z"));
    assert_eq!(position.day, BudgetDay::new(date(2026, 11, 15)));
    assert_eq!(position.elapsed, Duration::ZERO);
}

#[test]
fn weekday_comes_from_start_date() {
    // Sunday 01:00 MSK still belongs to Saturday.
    let position = moscow(four_am()).locate(at("2026-11-14T22:00:00Z"));
    assert_eq!(position.day.weekday(), Weekday::Saturday);
}

proptest! {
    #[test]
    fn elapsed_is_always_within_a_day(
        seconds in 0_i64..4_000_000_000,
        start_minute in 0_u16..1440,
    ) {
        let start = TimeOfDay::MIDNIGHT.plus_minutes(start_minute);
        let calendar = Calendar::new(TimeZone::get("America/New_York").unwrap(), start);
        let position = calendar.locate(Timestamp::from_second(seconds).unwrap());
        prop_assert!(position.elapsed < Duration::from_secs(SECONDS_PER_DAY));
    }

    #[test]
    fn time_of_day_round_trips_through_text(minute in 0_u16..1440) {
        let time = TimeOfDay::MIDNIGHT.plus_minutes(minute);
        prop_assert_eq!(time.to_string().parse::<TimeOfDay>().unwrap(), time);
    }
}
