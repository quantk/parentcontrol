use super::*;

fn four() -> TimeOfDay {
    "04:00".parse().unwrap()
}

/// A submission with the same day every day of the week.
fn submission(quota: &str, mode: &str, windows: &[(&str, &str)]) -> Vec<(String, String)> {
    WEEK.iter()
        .flat_map(|(_, key)| {
            let mut fields = vec![
                (format!("quota_{key}"), quota.to_owned()),
                (format!("mode_{key}"), mode.to_owned()),
            ];
            for (start, end) in windows {
                fields.push((format!("start_{key}"), (*start).to_owned()));
                fields.push((format!("end_{key}"), (*end).to_owned()));
            }
            fields
        })
        .collect()
}

fn parse(fields: &[(String, String)]) -> Result<WeekSchedule, String> {
    parse_week(&submitted_week(fields), four())
}

#[test]
fn parses_windows_and_skips_blank_rows() {
    let week = parse(&submission(
        "60",
        "windows",
        &[("16:00", "20:00"), ("", ""), ("21:00", "22:30")],
    ))
    .unwrap();
    assert_eq!(week.friday.quota(), Minutes::new(60));
    let windows: Vec<String> = week
        .friday
        .windows()
        .iter()
        .map(|w| format!("{}-{}", w.start(), w.end()))
        .collect();
    assert_eq!(windows, ["16:00-20:00", "21:00-22:30"]);
}

#[test]
fn all_day_ignores_typed_windows() {
    let week = parse(&submission("90", "all", &[("16:00", "20:00")])).unwrap();
    assert_eq!(week.monday.windows(), []);
}

#[test]
fn windows_mode_needs_a_window() {
    let error = parse(&submission("60", "windows", &[("", "")])).unwrap_err();
    assert!(
        error.starts_with("Понедельник: укажите хотя бы одно окно"),
        "{error}"
    );
}

#[test]
fn explains_bad_windows() {
    let half = parse(&submission("60", "windows", &[("16:00", "")])).unwrap_err();
    assert!(half.contains("начало и конец"), "{half}");
    let crossing = parse(&submission("60", "windows", &[("03:00", "05:00")])).unwrap_err();
    assert!(crossing.contains("пересекает начало суток"), "{crossing}");
    let empty = parse(&submission("60", "windows", &[("16:00", "16:00")])).unwrap_err();
    assert!(empty.contains("пустое"), "{empty}");
}

#[test]
fn explains_bad_quotas() {
    assert!(
        parse(&submission("много", "all", &[]))
            .unwrap_err()
            .contains("числом минут")
    );
    assert!(
        parse(&submission("2000", "all", &[]))
            .unwrap_err()
            .contains("1440")
    );
}

#[test]
fn stored_days_round_trip_through_the_form() {
    let week = parse(&submission("45", "windows", &[("21:00", "01:00")])).unwrap();
    let days = stored_week(&week);
    assert!(!days[0].all_day);
    assert_eq!(
        days[0].windows,
        [WindowField {
            start: "21:00".into(),
            end: "01:00".into()
        }]
    );
    assert_eq!(parse_week(&days, four()).unwrap(), week);

    let open = parse(&submission("45", "all", &[])).unwrap();
    let days = stored_week(&open);
    assert!(days[0].all_day);
    assert_eq!(days[0].windows.len(), 1, "a blank row to type into");
}

#[test]
fn the_preview_places_windows_within_the_budget_day() {
    let days = submitted_week(&submission(
        "60",
        "windows",
        &[("10:00", "16:00"), ("21:00", "01:00"), ("бред", "")],
    ));
    let preview = days[0].preview(four());
    assert_eq!(preview.len(), 2, "unparsable rows are skipped");
    assert_eq!(preview[0].from.to_string(), "25.0%");
    assert_eq!(preview[0].length.to_string(), "25.0%");
    assert_eq!(preview[1].from.to_string(), "70.8%");
    assert_eq!(preview[1].length.to_string(), "16.7%");

    let all_day = submitted_week(&submission("60", "all", &[]));
    assert_eq!(all_day[0].preview(four())[0].length.to_string(), "100.0%");
}
