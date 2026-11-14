use jiff::civil::date;

use super::*;

fn minutes(n: u64) -> Duration {
    Duration::from_mins(n)
}

fn labels(scale: &Scale) -> Vec<&str> {
    scale.ticks.iter().map(|tick| tick.label.as_str()).collect()
}

#[test]
fn the_axis_rounds_up_to_few_round_ticks() {
    assert_eq!(
        labels(&Scale::up_to(minutes(50))),
        ["0 мин", "15 мин", "30 мин", "45 мин", "1 ч"]
    );
    assert_eq!(
        labels(&Scale::up_to(minutes(130))),
        ["0 мин", "1 ч", "2 ч", "3 ч"]
    );
    assert_eq!(labels(&Scale::up_to(Duration::ZERO)), ["0 мин", "5 мин"]);
    let scale = Scale::up_to(minutes(130));
    assert_eq!(scale.place(minutes(90)).to_string(), "50.0%");
}

#[test]
fn long_axes_still_get_few_ticks() {
    assert!(Scale::up_to(minutes(100 * 60)).ticks.len() <= 5);
}

fn day(d: i8, by_group: &[u64]) -> DayUsage {
    DayUsage {
        day: BudgetDay::new(date(2026, 11, d)),
        by_group: by_group.iter().map(|&m| minutes(m)).collect(),
        offline: false,
    }
}

#[test]
fn columns_stack_groups_and_mark_overspend() {
    let days = [day(13, &[30, 0]), day(14, &[50, 20])];
    let series = [
        Series {
            name: "Игры",
            class: "series-1",
        },
        Series {
            name: "Видео",
            class: "series-2",
        },
    ];
    let selected = BudgetDay::new(date(2026, 11, 14));
    let (_, drawn) = columns(&days, &series, None, selected);
    assert_eq!(drawn[0].stacks.len(), 1, "unused groups draw nothing");
    assert_eq!(drawn[1].stacks.len(), 2);
    assert!(drawn[1].selected && !drawn[0].selected);
    assert!(drawn[1].tip.contains("Видео: 20 мин"));

    let one = [day(13, &[30]), day(14, &[70])];
    let allowances = [minutes(60), minutes(60)];
    let (scale, columns) = columns(&one, &series[..1], Some(&allowances), selected);
    assert!(!columns[0].over && columns[1].over);
    assert_eq!(columns[0].allowance, Some(scale.place(minutes(60))));
    assert!(
        !columns[0].tip.contains("Игры:"),
        "one group needs no breakdown"
    );
}

#[test]
fn long_periods_label_some_mondays_only() {
    let days: Vec<DayUsage> = (1..=30).map(|d| day(d, &[0])).collect();
    let (_, drawn) = columns(&days, &[], None, days[0].day);
    let labelled: Vec<_> = drawn.iter().filter_map(|c| c.label.clone()).collect();
    assert_eq!(labelled, ["02.11", "09.11", "16.11", "23.11", "30.11"]);
}

#[test]
fn heat_levels_follow_the_busiest_hour() {
    let mut cells = [[Duration::ZERO; HOURS]; 7];
    cells[5][12] = minutes(60);
    cells[5][13] = minutes(10);
    cells[0][0] = Duration::from_secs(10);
    let (rows, hours) = heat_rows(&cells, "04:00".parse().unwrap());
    assert_eq!(rows[5].label, "сб");
    assert_eq!(rows[5].cells[12].level, 6);
    assert_eq!(rows[5].cells[13].level, 1);
    assert_eq!(rows[0].cells[0].level, 0, "seconds are noise");
    assert_eq!(rows[5].cells[12].tip, "сб, 16:00–17:00\n1 ч");
    assert_eq!(hours[0], "04:00");
    assert_eq!(hours[1], "");
    assert_eq!(hours[3], "07:00");
}

#[test]
fn bars_scale_to_the_largest() {
    let bars = bars(vec![
        ("ПК".to_owned(), minutes(90)),
        ("Ноутбук".to_owned(), minutes(30)),
    ]);
    assert_eq!(bars[0].width.to_string(), "100.0%");
    assert_eq!(bars[1].width.to_string(), "33.3%");
    assert_eq!(bars[1].value, "30 мин");
}

#[test]
fn colors_follow_group_order_then_fold() {
    assert_eq!(series(Some(0)), "series-1");
    assert_eq!(series(Some(7)), "series-8");
    assert_eq!(series(Some(8)), "series-other");
    assert_eq!(series(None), "series-other");
}
