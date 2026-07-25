use std::time::Duration;

use beatwise::{
    Health, HealthPolicy, HealthState, HealthVerdict, LearnedTiming, LearningModel, Monitor,
    TaskConfig, Timing, TimingStatus,
};

#[test]
fn fixed_monitor_is_usable_from_outside_the_crate() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "external-fixed",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid fixed registration");
    let id = heartbeat.id();

    let initial = monitor.status(id).expect("registered task is retained");
    assert_eq!(initial.id, id);
    assert_eq!(initial.name.as_ref(), "external-fixed");
    assert_eq!(initial.heartbeat_count, 0);
    assert!(matches!(initial.health, Health::Starting { .. }));

    heartbeat.beat().expect("running task accepts a heartbeat");

    let running = monitor.status(id).expect("task remains retained");
    assert_eq!(running.heartbeat_count, 1);
    assert!(matches!(running.health, Health::Healthy { .. }));
}

#[test]
fn learned_configuration_is_constructible_through_public_types() {
    let monitor = Monitor::new();
    let timing = LearnedTiming::default()
        .with_minimum_samples(5)
        .with_model(LearningModel::robust_window(5));
    let heartbeat = monitor
        .register(TaskConfig::new("external-learner", Timing::Learned(timing)))
        .expect("valid learned registration");

    heartbeat.beat().expect("first learning heartbeat accepted");

    let status = monitor
        .status(heartbeat.id())
        .expect("learned task remains retained");
    assert_eq!(status.health.state(), HealthState::Learning);
    assert!(matches!(
        status.timing,
        TimingStatus::Learning {
            model: LearningModel::RobustWindow { capacity: 5 },
            required: 5,
            ..
        }
    ));
}

#[test]
fn transitions_and_reports_compose_through_the_public_api() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "external-report",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid fixed registration");
    let mut transitions = monitor.transition_cursor();

    let initial = transitions.poll();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].previous, None);
    assert_eq!(initial[0].current, HealthState::Starting);

    heartbeat.beat().expect("heartbeat accepted");

    let changed = transitions.poll();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].previous, Some(HealthState::Starting));
    assert_eq!(changed[0].current, HealthState::Healthy);

    let report = monitor.report(HealthPolicy::liveness());
    assert_eq!(report.summary.verdict, HealthVerdict::Nominal);
    assert_eq!(report.summary.counts.healthy, 1);
    assert_eq!(report.tasks.len(), 1);
    assert!(report.is_nominal());
}

#[test]
fn stopped_records_remain_observable_until_purged() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "external-stop",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid fixed registration");
    let id = heartbeat.id();

    heartbeat.stop();

    let stopped = monitor.status(id).expect("stopped record is retained");
    assert_eq!(stopped.health.state(), HealthState::Stopped);
    assert_eq!(monitor.purge_stopped(), 1);
    assert!(monitor.status(id).is_none());
}
