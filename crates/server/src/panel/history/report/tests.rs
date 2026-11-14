use jiff::civil::date;
use jiff::tz::TimeZone;

use super::*;

const GAMES: GroupId = GroupId::new(1);
const VIDEO: GroupId = GroupId::new(2);
const PC: DeviceId = DeviceId::new(1);
const LAPTOP: DeviceId = DeviceId::new(2);

/// Moscow, days starting at 04:00.
fn calendar() -> Calendar {
    Calendar::new(
        TimeZone::get("Europe/Moscow").unwrap(),
        "04:00".parse().unwrap(),
    )
}

/// A session starting at Moscow time `start` (`2026-11-14T16:30`).
fn session(start: &str, minutes: u64, group: GroupId, device: DeviceId) -> Session {
    let start: Timestamp = format!("{start}:00+03:00").parse().unwrap();
    Session {
        day: calendar().locate(start).day,
        device,
        group,
        start,
        duration: Duration::from_mins(minutes),
        offline: false,
    }
}

fn day(d: i8) -> BudgetDay {
    BudgetDay::new(date(2026, 11, d))
}

#[test]
fn periods_end_on_their_last_day() {
    let week = Period::ending(day(14), 7);
    assert_eq!(week.first(), day(8));
    assert_eq!(week.last(), day(14));
    let before = week.previous();
    assert_eq!((before.first(), before.last()), (day(1), day(7)));
    assert!(week.contains(day(10)) && !week.contains(day(7)));
}

#[test]
fn daily_usage_is_split_by_group() {
    let sessions = [
        session("2026-11-14T16:00", 30, GAMES, PC),
        session("2026-11-14T18:00", 20, GAMES, LAPTOP),
        session("2026-11-14T19:00", 10, VIDEO, PC),
        // Before 04:00 belongs to the previous budget day.
        session("2026-11-14T02:00", 15, GAMES, PC),
    ];
    let days = daily(&sessions, &Period::ending(day(14), 2), &[GAMES, VIDEO]);
    assert_eq!(days[0].by_group, [Duration::from_mins(15), Duration::ZERO]);
    assert_eq!(
        days[1].by_group,
        [Duration::from_mins(50), Duration::from_mins(10)]
    );
    assert_eq!(days[1].total(), Duration::from_mins(60));
}

#[test]
fn the_heatmap_spreads_sessions_over_hours() {
    // Saturday 16:40 for 90 minutes: 20 in the 16:00 hour, 60 in 17:00, 10 in 18:00.
    let cells = heatmap(&[session("2026-11-14T16:40", 90, GAMES, PC)], &calendar());
    let saturday = 5;
    let column = |clock: usize| clock - 4;
    assert_eq!(cells[saturday][column(16)], Duration::from_mins(20));
    assert_eq!(cells[saturday][column(17)], Duration::from_mins(60));
    assert_eq!(cells[saturday][column(18)], Duration::from_mins(10));
    let total: Duration = cells.iter().flatten().sum();
    assert_eq!(total, Duration::from_mins(90));
}

#[test]
fn after_midnight_counts_toward_the_evening_before() {
    // 01:30 on Sunday is still Saturday's budget day, 21 hours after its start.
    let cells = heatmap(&[session("2026-11-15T01:30", 10, GAMES, PC)], &calendar());
    assert_eq!(cells[5][21], Duration::from_mins(10));
}

#[test]
fn devices_are_ranked_by_use() {
    let sessions = [
        session("2026-11-14T16:00", 10, GAMES, PC),
        session("2026-11-14T17:00", 30, GAMES, LAPTOP),
        session("2026-11-14T18:00", 5, VIDEO, PC),
    ];
    assert_eq!(
        by_device(&sessions),
        [
            (LAPTOP, Duration::from_mins(30)),
            (PC, Duration::from_mins(15))
        ]
    );
    assert_eq!(longest(&sessions).unwrap().device, LAPTOP);
}

#[test]
fn sessions_are_placed_within_their_day() {
    let (start, length) = offset_in_day(&session("2026-11-14T16:00", 30, GAMES, PC), &calendar());
    assert!((start - 12.0 * 60.0).abs() < f64::EPSILON);
    assert!((length - 30.0).abs() < f64::EPSILON);
    let (_, clipped) = offset_in_day(&session("2026-11-15T03:30", 60, GAMES, PC), &calendar());
    assert!(
        (clipped - 30.0).abs() < f64::EPSILON,
        "cut at the next day start"
    );
}
