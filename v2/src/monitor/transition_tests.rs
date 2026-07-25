use std::time::Duration;

use super::Monitor;
use crate::{
    Adaptation, FixedTiming, HealthState, LearnedTiming, TaskConfig, Timing, TimingStatus,
};

#[test]
fn first_poll_reports_retained_tasks_in_id_order() {
    let monitor = Monitor::new();
    let first = monitor
        .register(TaskConfig::new(
            "first",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let second = monitor
        .register(TaskConfig::new(
            "second",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let now = first.entry.created_tick.max(second.entry.created_tick);
    let mut cursor = monitor.transition_cursor();

    let transitions = cursor.poll_at(now);

    assert_eq!(transitions.len(), 2);
    assert_eq!(transitions[0].sequence, 1);
    assert_eq!(transitions[0].status.id, first.id());
    assert_eq!(transitions[0].previous, None);
    assert_eq!(transitions[0].current, HealthState::Starting);
    assert_eq!(transitions[1].sequence, 2);
    assert_eq!(transitions[1].status.id, second.id());
    assert_eq!(transitions[1].previous, None);
    assert_eq!(transitions[1].current, HealthState::Starting);
    assert_eq!(cursor.tracked_len(), 2);

    assert!(cursor.poll_at(now + 500).is_empty());
}

#[test]
fn cursor_reports_healthy_late_and_recovery_transitions() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "fixed-worker",
            Timing::Fixed(
                FixedTiming::new(Duration::from_millis(10)).with_grace(Duration::from_millis(2)),
            ),
        ))
        .expect("valid registration");
    let start = heartbeat.entry.created_tick;
    let mut cursor = monitor.transition_cursor();

    assert_eq!(cursor.poll_at(start)[0].current, HealthState::Starting);

    heartbeat
        .beat_at(start + 1_000)
        .expect("first heartbeat accepted");
    let healthy = cursor.poll_at(start + 11_000);
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0].previous, Some(HealthState::Starting));
    assert_eq!(healthy[0].current, HealthState::Healthy);

    let late = cursor.poll_at(start + 14_000);
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].previous, Some(HealthState::Healthy));
    assert_eq!(late[0].current, HealthState::Late);

    heartbeat
        .beat_at(start + 15_000)
        .expect("recovery heartbeat accepted");
    let recovered = cursor.poll_at(start + 15_500);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].previous, Some(HealthState::Late));
    assert_eq!(recovered[0].current, HealthState::Healthy);
}

#[test]
fn independent_cursors_observe_the_same_changes() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "shared-observation",
            Timing::fixed(Duration::from_millis(10)),
        ))
        .expect("valid registration");
    let start = heartbeat.entry.created_tick;
    let mut first = monitor.transition_cursor();
    let mut second = monitor.transition_cursor();

    assert_eq!(first.poll_at(start)[0].current, HealthState::Starting);
    assert_eq!(second.poll_at(start)[0].current, HealthState::Starting);

    heartbeat
        .beat_at(start + 1_000)
        .expect("heartbeat accepted");
    assert_eq!(first.poll_at(start + 2_000)[0].current, HealthState::Healthy);
    assert_eq!(second.poll_at(start + 2_000)[0].current, HealthState::Healthy);
}

#[test]
fn retraining_is_reported_as_a_return_to_learning() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(2)
        .with_startup_grace(Duration::from_secs(1))
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::FrozenAfterTraining);
    let heartbeat = monitor
        .register(TaskConfig::new("learner", Timing::Learned(learned)))
        .expect("valid learned registration");
    let start = heartbeat.entry.created_tick;
    let mut cursor = monitor.transition_cursor();

    cursor.poll_at(start);
    for offset in [1_000, 2_000, 3_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("training heartbeat accepted");
    }

    let trained = cursor.poll_at(start + 3_500);
    assert_eq!(trained[0].current, HealthState::Healthy);
    assert!(matches!(trained[0].status.timing, TimingStatus::Learned { .. }));

    monitor
        .retrain(heartbeat.id())
        .expect("learned task can retrain");
    let retraining = cursor.poll_at(start + 3_500);
    assert_eq!(retraining.len(), 1);
    assert_eq!(retraining[0].previous, Some(HealthState::Healthy));
    assert_eq!(retraining[0].current, HealthState::Learning);
    assert!(matches!(
        retraining[0].status.timing,
        TimingStatus::Learning { samples: 0, .. }
    ));
}

#[test]
fn stopped_tasks_are_reported_then_pruned_after_purge() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "stopping-worker",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let id = heartbeat.id();
    let start = heartbeat.entry.created_tick;
    let mut cursor = monitor.transition_cursor();

    cursor.poll_at(start);
    heartbeat.stop();

    let stopped = cursor.poll_at(start + 1_000);
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].previous, Some(HealthState::Starting));
    assert_eq!(stopped[0].current, HealthState::Stopped);
    assert_eq!(cursor.tracked_len(), 1);

    assert_eq!(monitor.purge_stopped(), 1);
    assert!(cursor.poll_at(start + 2_000).is_empty());
    assert!(cursor.is_empty());
    assert!(cursor.poll_task(id).is_none());
}

#[test]
fn reset_replays_current_state_as_a_first_observation() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "reset-worker",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let start = heartbeat.entry.created_tick;
    let mut cursor = monitor.transition_cursor();

    let initial = cursor
        .poll_task_at(heartbeat.id(), start)
        .expect("first observation");
    assert_eq!(initial.sequence, 1);
    assert_eq!(initial.previous, None);
    assert!(cursor.poll_task_at(heartbeat.id(), start + 100).is_none());

    cursor.reset();
    let replayed = cursor
        .poll_task_at(heartbeat.id(), start + 200)
        .expect("state replayed after reset");
    assert_eq!(replayed.sequence, 1);
    assert_eq!(replayed.previous, None);
    assert_eq!(replayed.current, HealthState::Starting);
}
