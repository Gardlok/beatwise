use std::time::Duration;

use super::{Monitor, StopReason};
use crate::{
    HealthImpact, HealthPolicy, HealthState, HealthVerdict, LearnedTiming, TaskConfig, Timing,
};

#[test]
fn empty_monitor_has_an_empty_summary() {
    let monitor = Monitor::new();
    let summary = monitor.summary_at(HealthPolicy::readiness(), 1);

    assert_eq!(summary.observed_after, Duration::ZERO);
    assert_eq!(summary.verdict, HealthVerdict::Empty);
    assert_eq!(summary.counts.total(), 0);
    assert_eq!(summary.considered_tasks, 0);
    assert_eq!(summary.ignored_tasks, 0);
    assert!(!summary.is_nominal());
}

#[test]
fn summary_counts_every_stable_health_state() {
    let monitor = Monitor::new();

    let starting = monitor
        .register(TaskConfig::new(
            "starting",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid starting task");

    let learned = LearnedTiming::default()
        .with_minimum_samples(3)
        .with_startup_grace(Duration::from_secs(1))
        .with_minimum_grace(Duration::from_micros(100));
    let learning = monitor
        .register(TaskConfig::new("learning", Timing::Learned(learned)))
        .expect("valid learning task");

    let healthy = monitor
        .register(TaskConfig::new(
            "healthy",
            Timing::fixed(Duration::from_millis(100)),
        ))
        .expect("valid healthy task");

    let late = monitor
        .register(TaskConfig::new(
            "late",
            Timing::fixed(Duration::from_millis(5)),
        ))
        .expect("valid late task");

    let stopped = monitor
        .register(TaskConfig::new(
            "stopped",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid stopped task");

    learning
        .beat_at(learning.entry.created_tick + 1_000)
        .expect("learning heartbeat accepted");
    healthy
        .beat_at(healthy.entry.created_tick + 1_000)
        .expect("healthy heartbeat accepted");
    late.beat_at(late.entry.created_tick + 1_000)
        .expect("late heartbeat accepted");
    stopped.mark_stopped_at(StopReason::Explicit, stopped.entry.created_tick + 1_000);

    let now = [
        starting.entry.created_tick,
        learning.entry.created_tick,
        healthy.entry.created_tick,
        late.entry.created_tick,
        stopped.entry.created_tick,
    ]
    .into_iter()
    .max()
    .expect("task ticks exist")
        + 20_000;

    let summary = monitor.summary_at(HealthPolicy::readiness(), now);

    assert_eq!(summary.counts.starting, 1);
    assert_eq!(summary.counts.learning, 1);
    assert_eq!(summary.counts.healthy, 1);
    assert_eq!(summary.counts.late, 1);
    assert_eq!(summary.counts.stopped, 1);
    assert_eq!(summary.counts.total(), 5);
    assert_eq!(summary.considered_tasks, 5);
    assert_eq!(summary.ignored_tasks, 0);
    assert_eq!(summary.verdict, HealthVerdict::Unhealthy);
}

#[test]
fn readiness_moves_from_degraded_to_nominal_to_unhealthy() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "readiness-worker",
            Timing::fixed(Duration::from_millis(10)),
        ))
        .expect("valid registration");
    let start = heartbeat.entry.created_tick;
    let policy = HealthPolicy::readiness();

    let starting = monitor.summary_at(policy, start);
    assert_eq!(starting.verdict, HealthVerdict::Degraded);

    heartbeat
        .beat_at(start + 1_000)
        .expect("heartbeat accepted");
    let healthy = monitor.summary_at(policy, start + 2_000);
    assert_eq!(healthy.verdict, HealthVerdict::Nominal);
    assert!(healthy.is_nominal());

    let late = monitor.summary_at(policy, start + 20_000);
    assert_eq!(late.verdict, HealthVerdict::Unhealthy);
}

#[test]
fn liveness_ignores_stopped_tasks() {
    let monitor = Monitor::new();
    let stopped = monitor
        .register(TaskConfig::new(
            "completed-worker",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid stopped task");
    stopped.mark_stopped_at(StopReason::Explicit, stopped.entry.created_tick + 1_000);

    let stopped_only =
        monitor.summary_at(HealthPolicy::liveness(), stopped.entry.created_tick + 2_000);
    assert_eq!(stopped_only.verdict, HealthVerdict::Empty);
    assert_eq!(stopped_only.considered_tasks, 0);
    assert_eq!(stopped_only.ignored_tasks, 1);

    let healthy = monitor
        .register(TaskConfig::new(
            "live-worker",
            Timing::fixed(Duration::from_millis(10)),
        ))
        .expect("valid healthy task");
    let healthy_start = healthy.entry.created_tick;
    healthy
        .beat_at(healthy_start + 1_000)
        .expect("heartbeat accepted");

    let mixed = monitor.summary_at(HealthPolicy::liveness(), healthy_start + 2_000);
    assert_eq!(mixed.verdict, HealthVerdict::Nominal);
    assert_eq!(mixed.considered_tasks, 1);
    assert_eq!(mixed.ignored_tasks, 1);
}

#[test]
fn strict_policy_rejects_a_learning_task() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(3)
        .with_startup_grace(Duration::from_secs(1));
    let heartbeat = monitor
        .register(TaskConfig::new("strict-learner", Timing::Learned(learned)))
        .expect("valid learned task");
    let start = heartbeat.entry.created_tick;
    heartbeat
        .beat_at(start + 1_000)
        .expect("heartbeat accepted");

    let summary = monitor.summary_at(HealthPolicy::strict(), start + 2_000);
    assert_eq!(summary.counts.learning, 1);
    assert_eq!(summary.verdict, HealthVerdict::Unhealthy);
}

#[test]
fn custom_policy_can_degrade_instead_of_fail_a_late_task() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "custom-policy-worker",
            Timing::fixed(Duration::from_millis(5)),
        ))
        .expect("valid registration");
    let start = heartbeat.entry.created_tick;
    heartbeat
        .beat_at(start + 1_000)
        .expect("heartbeat accepted");

    let policy = HealthPolicy::liveness().with_impact(HealthState::Late, HealthImpact::Degraded);
    assert_eq!(policy.impact(HealthState::Late), HealthImpact::Degraded);

    let summary = monitor.summary_at(policy, start + 20_000);
    assert_eq!(summary.counts.late, 1);
    assert_eq!(summary.verdict, HealthVerdict::Degraded);
}

#[test]
fn report_orders_task_snapshots_and_matches_its_summary() {
    let monitor = Monitor::new();
    let first = monitor
        .register(TaskConfig::new(
            "first",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid first task");
    let second = monitor
        .register(TaskConfig::new(
            "second",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid second task");
    let third = monitor
        .register(TaskConfig::new(
            "third",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid third task");
    let now = first
        .entry
        .created_tick
        .max(second.entry.created_tick)
        .max(third.entry.created_tick);

    let report = monitor.report_at(HealthPolicy::readiness(), now);

    assert_eq!(report.tasks.len(), 3);
    assert_eq!(report.tasks[0].id, first.id());
    assert_eq!(report.tasks[1].id, second.id());
    assert_eq!(report.tasks[2].id, third.id());
    assert_eq!(report.summary.counts.starting, 3);
    assert_eq!(report.summary.counts.total(), report.tasks.len());
    assert_eq!(report.summary.verdict, HealthVerdict::Degraded);
    assert!(!report.is_nominal());
}
