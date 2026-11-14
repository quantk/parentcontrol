use parentcontrol_core::enforcement::Termination;

use super::*;
use crate::test_support::{FakeClock, FakePlatform, minecraft, policy};

const STEP: Duration = Duration::from_secs(5);

/// 17:00 Moscow time, inside the 16:00–20:00 window.
const INSIDE_WINDOW: &str = "2026-11-14T14:00:00Z";

fn run_steps(limiter: &mut Limiter<&FakePlatform, &FakeClock>, policy: &Policy, steps: usize) {
    for _ in 0..steps {
        limiter.step(policy, STEP, true);
    }
}

#[test]
fn game_is_closed_softly_when_quota_runs_out_then_forced() {
    let (platform, clock, policy) = (
        FakePlatform::default(),
        FakeClock::at(INSIDE_WINDOW),
        policy(1),
    );
    platform.processes.borrow_mut().push(minecraft(100, "kid"));
    let mut limiter = Limiter::new(&platform, &clock, Journal::default(), STEP);

    // Twelve steps of five seconds use up the minute.
    run_steps(&mut limiter, &policy, 11);
    assert_eq!(*platform.terminated.borrow(), []);
    run_steps(&mut limiter, &policy, 1);
    assert_eq!(
        *platform.terminated.borrow(),
        [(100, Termination::Graceful)]
    );

    run_steps(&mut limiter, &policy, 6);
    assert_eq!(
        *platform.terminated.borrow(),
        [(100, Termination::Graceful), (100, Termination::Forced)]
    );
}

#[test]
fn launch_outside_window_is_terminated_at_once() {
    // 21:00 Moscow time.
    let (platform, clock, policy) = (
        FakePlatform::default(),
        FakeClock::at("2026-11-14T18:00:00Z"),
        policy(60),
    );
    platform.processes.borrow_mut().push(minecraft(100, "kid"));
    let mut limiter = Limiter::new(&platform, &clock, Journal::default(), STEP);
    run_steps(&mut limiter, &policy, 1);
    assert_eq!(*platform.terminated.borrow(), [(100, Termination::Forced)]);
}

#[test]
fn processes_of_other_accounts_are_left_alone() {
    let (platform, clock, policy) = (
        FakePlatform::default(),
        FakeClock::at("2026-11-14T18:00:00Z"),
        policy(60),
    );
    platform
        .processes
        .borrow_mut()
        .push(minecraft(100, "parent"));
    let mut limiter = Limiter::new(&platform, &clock, Journal::default(), STEP);
    run_steps(&mut limiter, &policy, 3);
    assert_eq!(*platform.terminated.borrow(), []);
    assert_eq!(limiter.journal().records().count(), 0);
}

#[test]
fn usage_survives_a_restart_through_the_journal() {
    let (platform, clock, policy) = (
        FakePlatform::default(),
        FakeClock::at(INSIDE_WINDOW),
        policy(1),
    );
    platform.processes.borrow_mut().push(minecraft(100, "kid"));
    let mut limiter = Limiter::new(&platform, &clock, Journal::default(), STEP);
    run_steps(&mut limiter, &policy, 13);
    let journal: Journal =
        serde_json::from_str(&serde_json::to_string(limiter.journal()).unwrap()).unwrap();

    // After the restart the quota is still used up, so a relaunch dies at once.
    let platform = FakePlatform::default();
    platform.processes.borrow_mut().push(minecraft(200, "kid"));
    let mut limiter = Limiter::new(&platform, &clock, journal, STEP);
    run_steps(&mut limiter, &policy, 1);
    assert_eq!(*platform.terminated.borrow(), [(200, Termination::Forced)]);
}

#[test]
fn long_gap_between_steps_is_not_credited() {
    let (platform, clock, policy) = (
        FakePlatform::default(),
        FakeClock::at(INSIDE_WINDOW),
        policy(60),
    );
    platform.processes.borrow_mut().push(minecraft(100, "kid"));
    let mut limiter = Limiter::new(&platform, &clock, Journal::default(), STEP);
    limiter.step(&policy, Duration::from_hours(2), true);
    let day = policy.calendar().locate(clock.now()).day;
    assert_eq!(limiter.journal().usage(GroupId::new(1), day), STEP * 2);
}
