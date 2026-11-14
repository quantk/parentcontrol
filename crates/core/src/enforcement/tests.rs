use super::*;
use crate::test_support::minutes;

const GAMES: GroupId = GroupId::new(1);
const TICK: Duration = Duration::from_secs(5);

fn allowed(stop_in: Duration, reason: StopReason) -> Availability {
    Availability::Allowed { stop_in, reason }
}

fn observe(availability: Availability, pids: &[u32]) -> Vec<Observation> {
    vec![Observation {
        status: GroupStatus {
            group: GAMES,
            remaining: Duration::ZERO,
            availability,
            next_window: None,
        },
        processes: pids.iter().map(|&pid| ProcessId::new(pid, 0)).collect(),
    }]
}

fn terminate(pid: u32, mode: Termination) -> Action {
    Action::Terminate {
        process: ProcessId::new(pid, 0),
        mode,
    }
}

fn notify(notice: Notice) -> Action {
    Action::Notify(notice)
}

fn warning(left: u32, reason: StopReason) -> Action {
    notify(Notice::Warning {
        group: GAMES,
        left: Minutes::new(left),
        reason,
    })
}

const NOTHING: Vec<Action> = Vec::new();

const QUOTA: DenyReason = DenyReason::QuotaExhausted;

#[test]
fn nothing_happens_with_plenty_of_time() {
    let mut enforcer = Enforcer::default();
    assert_eq!(
        enforcer.step(
            &observe(allowed(minutes(30), StopReason::QuotaEnd), &[10]),
            TICK
        ),
        NOTHING
    );
}

#[test]
fn warns_once_per_threshold() {
    let mut enforcer = Enforcer::default();
    let mut warnings = Vec::new();
    for left in [11, 10, 9, 6, 5, 4, 2, 1] {
        warnings.extend(enforcer.step(
            &observe(allowed(minutes(left), StopReason::QuotaEnd), &[10]),
            TICK,
        ));
    }
    let q = StopReason::QuotaEnd;
    assert_eq!(warnings, vec![warning(10, q), warning(5, q), warning(1, q)]);
}

#[test]
fn starting_late_warns_once_with_the_actual_time() {
    let mut enforcer = Enforcer::default();
    let actions = enforcer.step(
        &observe(allowed(minutes(3), StopReason::WindowEnd), &[10]),
        TICK,
    );
    assert_eq!(actions, vec![warning(3, StopReason::WindowEnd)]);
    let actions = enforcer.step(
        &observe(allowed(minutes(2), StopReason::WindowEnd), &[10]),
        TICK,
    );
    assert_eq!(actions, NOTHING);
}

#[test]
fn no_warning_while_nothing_runs_but_it_comes_on_launch() {
    let mut enforcer = Enforcer::default();
    assert_eq!(
        enforcer.step(
            &observe(allowed(minutes(4), StopReason::QuotaEnd), &[]),
            TICK
        ),
        NOTHING
    );
    let actions = enforcer.step(
        &observe(allowed(minutes(4), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    assert_eq!(actions, vec![warning(4, StopReason::QuotaEnd)]);
}

#[test]
fn bonus_rearms_warnings() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    assert_eq!(
        enforcer.step(
            &observe(allowed(minutes(20), StopReason::QuotaEnd), &[10]),
            TICK
        ),
        NOTHING
    );
    let actions = enforcer.step(
        &observe(allowed(minutes(10), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    assert_eq!(actions, vec![warning(10, StopReason::QuotaEnd)]);
}

#[test]
fn running_game_is_closed_softly_then_forced_after_grace() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );

    let actions = enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    assert_eq!(
        actions,
        vec![
            terminate(10, Termination::Graceful),
            notify(Notice::Closing {
                group: GAMES,
                reason: QUOTA
            })
        ]
    );

    for _ in 0..5 {
        assert_eq!(
            enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK),
            NOTHING
        );
    }
    let actions = enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    assert_eq!(actions, vec![terminate(10, Termination::Forced)]);
}

#[test]
fn game_that_quits_during_grace_is_left_alone() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    assert_eq!(
        enforcer.step(&observe(Availability::Denied(QUOTA), &[]), GRACE_PERIOD),
        NOTHING
    );
}

#[test]
fn launch_while_denied_is_terminated_at_once() {
    let mut enforcer = Enforcer::default();
    let reason = DenyReason::OutsideWindow;
    let actions = enforcer.step(&observe(Availability::Denied(reason), &[20]), TICK);
    assert_eq!(
        actions,
        vec![
            terminate(20, Termination::Forced),
            notify(Notice::LaunchDenied {
                group: GAMES,
                reason
            })
        ]
    );
}

#[test]
fn launch_during_grace_is_terminated_at_once() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    let actions = enforcer.step(&observe(Availability::Denied(QUOTA), &[10, 11]), TICK);
    assert_eq!(
        actions,
        vec![
            terminate(11, Termination::Forced),
            notify(Notice::LaunchDenied {
                group: GAMES,
                reason: QUOTA
            })
        ]
    );
}

#[test]
fn survivor_of_forced_termination_is_terminated_again() {
    let mut enforcer = Enforcer::default();
    enforcer.step(&observe(Availability::Denied(QUOTA), &[20]), TICK);
    let actions = enforcer.step(&observe(Availability::Denied(QUOTA), &[20]), TICK);
    assert_eq!(actions, vec![terminate(20, Termination::Forced)]);
}

#[test]
fn permission_during_grace_cancels_forced_termination() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    let actions = enforcer.step(
        &observe(allowed(minutes(15), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    assert_eq!(
        actions,
        vec![notify(Notice::CloseCancelled { group: GAMES })]
    );
    assert_eq!(
        enforcer.step(
            &observe(allowed(minutes(15), StopReason::QuotaEnd), &[10]),
            GRACE_PERIOD
        ),
        NOTHING
    );
}

#[test]
fn sleep_during_grace_forces_termination_on_wake() {
    let mut enforcer = Enforcer::default();
    enforcer.step(
        &observe(allowed(minutes(1), StopReason::QuotaEnd), &[10]),
        TICK,
    );
    enforcer.step(&observe(Availability::Denied(QUOTA), &[10]), TICK);
    let actions = enforcer.step(
        &observe(Availability::Denied(QUOTA), &[10]),
        Duration::from_secs(3600),
    );
    assert_eq!(actions, vec![terminate(10, Termination::Forced)]);
}

#[test]
fn permission_after_a_forced_termination_announces_nothing() {
    let mut enforcer = Enforcer::default();
    enforcer.step(&observe(Availability::Denied(QUOTA), &[20]), TICK);
    let actions = enforcer.step(
        &observe(allowed(minutes(15), StopReason::QuotaEnd), &[20]),
        TICK,
    );
    assert_eq!(actions, NOTHING);
}
