use proptest::prelude::*;

use super::*;

fn time(text: &str) -> TimeOfDay {
    text.parse().unwrap()
}

fn window(start: &str, end: &str) -> TimeWindow {
    TimeWindow::new(time(start), time(end)).unwrap()
}

fn hours(h: u64) -> Duration {
    Duration::from_hours(h)
}

#[test]
fn empty_window_is_rejected() {
    assert_eq!(
        TimeWindow::new(time("10:00"), time("10:00")),
        Err(ScheduleError::EmptyWindow(time("10:00")))
    );
}

#[test]
fn quota_above_a_day_is_rejected() {
    assert!(DaySchedule::new(Minutes::new(24 * 60 + 1), vec![]).is_err());
    assert!(DaySchedule::new(MAX_DAILY_QUOTA, vec![]).is_ok());
}

#[test]
fn span_is_measured_from_day_start() {
    let span = window("16:00", "20:00").span(time("04:00")).unwrap();
    assert_eq!(
        span,
        Span {
            start: hours(12),
            end: hours(16)
        }
    );
}

#[test]
fn window_may_cross_midnight() {
    let span = window("22:00", "01:00").span(time("04:00")).unwrap();
    assert_eq!(
        span,
        Span {
            start: hours(18),
            end: hours(21)
        }
    );
}

#[test]
fn window_ending_at_day_start_reaches_end_of_day() {
    let span = window("20:00", "04:00").span(time("04:00")).unwrap();
    assert_eq!(
        span,
        Span {
            start: hours(16),
            end: hours(24)
        }
    );
}

#[test]
fn window_crossing_day_start_has_no_span() {
    assert_eq!(window("03:00", "05:00").span(time("04:00")), None);
}

#[test]
fn day_without_windows_is_unrestricted() {
    let day = DaySchedule::new(Minutes::new(60), vec![]).unwrap();
    assert_eq!(day.spans(time("04:00")), None);
}

#[test]
fn overlapping_windows_are_merged() {
    let day = DaySchedule::new(
        Minutes::new(60),
        vec![
            window("18:00", "21:00"),
            window("10:00", "12:00"),
            window("16:00", "19:00"),
        ],
    )
    .unwrap();
    assert_eq!(
        day.spans(time("04:00")).unwrap(),
        vec![
            Span {
                start: hours(6),
                end: hours(8)
            },
            Span {
                start: hours(12),
                end: hours(17)
            },
        ]
    );
}

#[test]
fn deserializes_day_from_json() {
    let day: DaySchedule =
        serde_json::from_str(r#"{"quota": 90, "windows": [{"start": "16:00", "end": "20:00"}]}"#)
            .unwrap();
    assert_eq!(day.quota(), Minutes::new(90));
    assert_eq!(day.windows(), &[window("16:00", "20:00")]);
}

#[test]
fn weekday_lookup_picks_matching_field() {
    let mut week = WeekSchedule::every_day(&DaySchedule::new(Minutes::new(30), vec![]).unwrap());
    week.saturday = DaySchedule::new(Minutes::new(120), vec![]).unwrap();
    assert_eq!(week.day(Weekday::Saturday).quota(), Minutes::new(120));
    assert_eq!(week.day(Weekday::Monday).quota(), Minutes::new(30));
}

proptest! {
    #[test]
    fn merged_spans_are_sorted_and_disjoint(
        bounds in prop::collection::vec((0_u16..1440, 1_u16..1440), 1..8),
    ) {
        let day_start = time("04:00");
        let windows = bounds
            .into_iter()
            .map(|(start, length)| {
                let start = day_start.plus_minutes(start);
                TimeWindow::new(start, start.plus_minutes(length)).unwrap()
            })
            .collect();
        let day = DaySchedule::new(Minutes::new(60), windows).unwrap();
        let spans = day.spans(day_start).unwrap();
        for span in &spans {
            prop_assert!(span.start < span.end);
            prop_assert!(span.end <= hours(24));
        }
        for pair in spans.windows(2) {
            prop_assert!(pair[0].end < pair[1].start);
        }
    }
}
