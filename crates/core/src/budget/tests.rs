use proptest::prelude::*;

use super::*;
use crate::policy::{Adjustment, DayUsage};
use crate::test_support::{at, day_start, group, minutes, saturday, time};
use crate::units::MinuteDelta;

fn status(group: &GroupPolicy, clock: &str, local: u64) -> GroupStatus {
    evaluate(group, day_start(), at(clock), minutes(local))
}

#[test]
fn quota_left_inside_window_is_allowed_until_quota_end() {
    let status = status(&group(60, &[("16:00", "20:00")]), "17:00", 20);
    assert_eq!(status.remaining, minutes(40));
    assert_eq!(
        status.availability,
        Availability::Allowed {
            stop_in: minutes(40),
            reason: StopReason::QuotaEnd
        }
    );
}

#[test]
fn window_closing_first_is_the_stop_reason() {
    let status = status(&group(60, &[("16:00", "20:00")]), "19:30", 0);
    assert_eq!(
        status.availability,
        Availability::Allowed {
            stop_in: minutes(30),
            reason: StopReason::WindowEnd
        }
    );
}

#[test]
fn outside_window_is_denied_even_with_quota() {
    let status = status(&group(60, &[("16:00", "20:00")]), "21:00", 0);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::OutsideWindow)
    );
    assert_eq!(status.next_window, None);
}

#[test]
fn before_window_reports_when_it_opens() {
    let status = status(&group(60, &[("16:00", "20:00")]), "10:00", 0);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::OutsideWindow)
    );
    assert_eq!(status.next_window, Some(time("16:00")));
}

#[test]
fn window_end_is_exclusive() {
    let status = status(&group(60, &[("16:00", "20:00")]), "20:00", 0);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::OutsideWindow)
    );
}

#[test]
fn exhausted_quota_is_denied() {
    let status = status(&group(60, &[]), "12:00", 60);
    assert_eq!(status.remaining, Duration::ZERO);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::QuotaExhausted)
    );
}

#[test]
fn block_wins_over_everything() {
    let mut group = group(60, &[("16:00", "20:00")]);
    group.blocked = true;
    assert_eq!(
        status(&group, "17:00", 0).availability,
        Availability::Denied(DenyReason::Blocked)
    );
    assert_eq!(
        status(&group, "21:00", 60).availability,
        Availability::Denied(DenyReason::Blocked)
    );
}

#[test]
fn window_wins_over_quota() {
    let status = status(&group(60, &[("16:00", "20:00")]), "21:00", 60);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::OutsideWindow)
    );
}

fn adjust(group: &mut GroupPolicy, day: crate::calendar::BudgetDay, minutes: i32) {
    group.adjustments.push(Adjustment {
        day,
        minutes: MinuteDelta::new(minutes),
    });
}

#[test]
fn adjustments_change_the_quota_of_their_day_only() {
    let mut group = group(30, &[]);
    adjust(&mut group, saturday(), 15);
    adjust(&mut group, saturday(), 10);
    let next_day = crate::calendar::BudgetDay::new(saturday().date().tomorrow().unwrap());
    adjust(&mut group, next_day, 100);
    assert_eq!(status(&group, "12:00", 30).remaining, minutes(25));
}

#[test]
fn taking_time_away_can_end_the_day() {
    let mut group = group(60, &[]);
    adjust(&mut group, saturday(), -20);
    assert_eq!(status(&group, "12:00", 30).remaining, minutes(10));
    adjust(&mut group, saturday(), -100);
    let status = status(&group, "12:00", 30);
    assert_eq!(status.remaining, Duration::ZERO);
    assert_eq!(
        status.availability,
        Availability::Denied(DenyReason::QuotaExhausted)
    );
}

#[test]
fn added_time_does_not_open_a_closed_window() {
    let mut group = group(30, &[("16:00", "20:00")]);
    adjust(&mut group, saturday(), 60);
    assert_eq!(
        status(&group, "21:00", 0).availability,
        Availability::Denied(DenyReason::OutsideWindow)
    );
}

#[test]
fn remote_usage_of_the_same_day_is_subtracted() {
    let mut group = group(60, &[]);
    group.remote_usage = Some(DayUsage {
        day: saturday(),
        used: minutes(25),
    });
    assert_eq!(status(&group, "12:00", 5).remaining, minutes(30));
}

#[test]
fn remote_usage_of_another_day_is_ignored() {
    let mut group = group(60, &[]);
    let friday = crate::calendar::BudgetDay::new(saturday().date().yesterday().unwrap());
    group.remote_usage = Some(DayUsage {
        day: friday,
        used: minutes(60),
    });
    assert_eq!(status(&group, "12:00", 0).remaining, minutes(60));
}

#[test]
fn window_across_midnight_counts_after_midnight() {
    let group = group(120, &[("22:00", "01:00")]);
    assert_eq!(
        status(&group, "00:30", 0).availability,
        Availability::Allowed {
            stop_in: minutes(30),
            reason: StopReason::WindowEnd
        }
    );
}

#[test]
fn early_morning_belongs_to_previous_day_quota() {
    // 03:00 is the end of the budget day that started at 04:00.
    assert_eq!(status(&group(60, &[]), "03:00", 59).remaining, minutes(1));
}

proptest! {
    #[test]
    fn remaining_never_exceeds_allowance_and_shrinks_with_use(
        quota in 0_u32..=1440,
        adjustment in -600_i32..600,
        used in 0_u64..2000,
        extra in 0_u64..100,
        minute in 0_u16..1440,
    ) {
        let mut group = group(quota, &[]);
        adjust(&mut group, saturday(), adjustment);
        let clock = day_start().plus_minutes(minute).to_string();
        let less = status(&group, &clock, used);
        let more = status(&group, &clock, used + extra);
        let allowance = u64::try_from((i64::from(quota) + i64::from(adjustment)).max(0)).unwrap();
        prop_assert!(less.remaining <= minutes(allowance));
        prop_assert!(more.remaining <= less.remaining);
        prop_assert_eq!(less.remaining.is_zero(), matches!(less.availability, Availability::Denied(_)));
    }

    #[test]
    fn allowed_stop_never_exceeds_remaining(quota in 1_u32..=1440, used in 0_u64..1440, minute in 0_u16..1440) {
        let group = group(quota, &[("16:00", "20:00"), ("22:00", "01:00")]);
        let clock = day_start().plus_minutes(minute).to_string();
        let status = status(&group, &clock, used);
        if let Availability::Allowed { stop_in, .. } = status.availability {
            prop_assert!(stop_in <= status.remaining);
            prop_assert!(!stop_in.is_zero());
        }
    }
}
