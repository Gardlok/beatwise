use std::time::Duration;

use super::{Health, Monitor, StopReason, TimingStatus};
use crate::{
    Adaptation, Confidence, ConfigError, FixedTiming, LearnedTiming, LearningModel, RegisterError,
    RetrainError, TaskConfig, Timing,
};

#[test]
fn fixed_timing_becomes_healthy_then_late() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "fixed-worker",
            Timing::Fixed(
                FixedTiming::new(Duration::from_millis(10)).with_grace(Duration::from_millis(2)),
            ),
        ))
        .expect("valid fixed registration");

    let start = heartbeat.entry.created_tick;
    heartbeat
        .beat_at(start + 1_000)
        .expect("first heartbeat accepted");

    let healthy = monitor
        .status_at(heartbeat.id(), start + 11_000)
        .expect("status exists");
    assert!(matches!(healthy.health, Health::Healthy { .. }));

    let late = monitor
        .status_at(heartbeat.id(), start + 14_000)
        .expect("status exists");
    assert!(matches!(late.health, Health::Late { .. }));
}

#[test]
fn learned_timing_trains_without_retaining_history() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(3)
        .with_startup_grace(Duration::from_secs(1))
        .with_minimum_grace(Duration::from_micros(200))
        .with_sensitivity(4.0)
        .with_adaptation(Adaptation::FrozenAfterTraining);
    let heartbeat = monitor
        .register(TaskConfig::new("learner", Timing::Learned(learned)))
        .expect("valid learned registration");

    let start = heartbeat.entry.created_tick;
    for offset in [1_000, 2_000, 3_000, 4_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("training heartbeat accepted");
    }

    let healthy = monitor
        .status_at(heartbeat.id(), start + 4_500)
        .expect("status exists");
    assert!(matches!(healthy.health, Health::Healthy { .. }));
    assert!(matches!(healthy.timing, TimingStatus::Learned { .. }));

    let late = monitor
        .status_at(heartbeat.id(), start + 6_000)
        .expect("status exists");
    assert!(matches!(late.health, Health::Late { .. }));
}

#[test]
fn final_handle_drop_preserves_stopped_record() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "drop-test",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let id = heartbeat.id();
    let clone = heartbeat.clone();

    drop(heartbeat);
    let still_running = monitor.status(id).expect("record retained");
    assert!(!matches!(still_running.health, Health::Stopped { .. }));

    drop(clone);
    let stopped = monitor.status(id).expect("stopped record retained");
    assert!(matches!(
        stopped.health,
        Health::Stopped {
            reason: StopReason::LastHandleDropped,
            ..
        }
    ));
}

#[test]
fn stopped_records_are_removed_only_by_explicit_purge() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "purge-test",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");
    let id = heartbeat.id();

    heartbeat.stop();
    assert!(monitor.status(id).is_some());
    assert_eq!(monitor.purge_stopped(), 1);
    assert!(monitor.status(id).is_none());
}

#[test]
fn invalid_learning_configuration_is_rejected() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default().with_minimum_samples(1);
    let error = monitor
        .register(TaskConfig::new("invalid", Timing::Learned(learned)))
        .err()
        .expect("registration must fail");

    assert_eq!(
        error,
        RegisterError::InvalidConfig(ConfigError::TooFewLearningSamples)
    );
}

#[test]
fn robust_window_resists_a_training_outlier() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(5)
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::FrozenAfterTraining)
        .with_model(LearningModel::robust_window(5));
    let heartbeat = monitor
        .register(TaskConfig::new("robust-learner", Timing::Learned(learned)))
        .expect("valid robust registration");

    let start = heartbeat.entry.created_tick;
    for offset in [1_000, 2_000, 3_000, 53_000, 54_000, 55_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("training heartbeat accepted");
    }

    let status = monitor
        .status_at(heartbeat.id(), start + 55_500)
        .expect("status exists");

    assert!(matches!(status.health, Health::Healthy { .. }));
    assert!(matches!(
        status.timing,
        TimingStatus::Learned {
            model: LearningModel::RobustWindow { capacity: 5 },
            interval,
            ..
        } if interval == Duration::from_millis(1)
    ));
}

#[test]
fn robust_window_rejects_an_outlier_after_training() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(5)
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::Continuous)
        .with_model(LearningModel::robust_window(5));
    let heartbeat = monitor
        .register(TaskConfig::new(
            "robust-rejection",
            Timing::Learned(learned),
        ))
        .expect("valid robust registration");

    let start = heartbeat.entry.created_tick;
    for offset in [1_000, 2_000, 3_000, 4_000, 5_000, 6_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("training heartbeat accepted");
    }

    heartbeat
        .beat_at(start + 56_000)
        .expect("late heartbeat remains a valid liveness signal");

    let status = monitor
        .status_at(heartbeat.id(), start + 56_500)
        .expect("status exists");

    assert!(matches!(
        status.timing,
        TimingStatus::Learned {
            rejected_samples: 1,
            interval,
            ..
        } if interval == Duration::from_millis(1)
    ));
}

#[test]
fn confidence_increases_with_stable_evidence() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(2)
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::Continuous)
        .with_model(LearningModel::robust_window(5));
    let heartbeat = monitor
        .register(TaskConfig::new("confidence", Timing::Learned(learned)))
        .expect("valid learned registration");

    let start = heartbeat.entry.created_tick;
    for offset in [
        1_000, 2_000, 3_000, 4_000, 5_000, 6_000, 7_000, 8_000, 9_000,
    ] {
        heartbeat
            .beat_at(start + offset)
            .expect("stable heartbeat accepted");
    }

    let status = monitor
        .status_at(heartbeat.id(), start + 9_500)
        .expect("status exists");

    assert!(matches!(
        status.timing,
        TimingStatus::Learned {
            confidence: Confidence::High,
            ..
        }
    ));
}

#[test]
fn retraining_discards_the_old_baseline() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_minimum_samples(3)
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::FrozenAfterTraining);
    let heartbeat = monitor
        .register(TaskConfig::new("retrain", Timing::Learned(learned)))
        .expect("valid learned registration");
    let id = heartbeat.id();
    let start = heartbeat.entry.created_tick;

    for offset in [1_000, 2_000, 3_000, 4_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("initial training heartbeat accepted");
    }
    assert!(matches!(
        monitor
            .status_at(id, start + 4_500)
            .expect("status exists")
            .timing,
        TimingStatus::Learned { .. }
    ));

    monitor.retrain(id).expect("learned task can retrain");
    assert!(matches!(
        monitor
            .status_at(id, start + 4_500)
            .expect("status exists")
            .timing,
        TimingStatus::Learning { samples: 0, .. }
    ));

    // The first interval after reset is discarded. The following samples
    // establish a new two-millisecond baseline.
    for offset in [5_000, 7_000, 9_000, 11_000] {
        heartbeat
            .beat_at(start + offset)
            .expect("retraining heartbeat accepted");
    }

    assert!(matches!(
        monitor
            .status_at(id, start + 11_500)
            .expect("status exists")
            .timing,
        TimingStatus::Learned { interval, .. }
            if interval == Duration::from_millis(2)
    ));
}

#[test]
fn fixed_timing_cannot_be_retrained() {
    let monitor = Monitor::new();
    let heartbeat = monitor
        .register(TaskConfig::new(
            "fixed-retrain",
            Timing::fixed(Duration::from_secs(1)),
        ))
        .expect("valid registration");

    assert_eq!(
        monitor.retrain(heartbeat.id()),
        Err(RetrainError::FixedTiming)
    );
}

#[test]
fn invalid_robust_window_capacity_is_rejected() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default().with_model(LearningModel::robust_window(4));
    let error = monitor
        .register(TaskConfig::new("invalid-window", Timing::Learned(learned)))
        .err()
        .expect("registration must fail");

    assert_eq!(
        error,
        RegisterError::InvalidConfig(ConfigError::InvalidRobustWindowCapacity)
    );
}
