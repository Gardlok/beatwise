use std::time::Duration;

use super::{Health, Heartbeat, Monitor, TimingStatus};
use crate::{
    Adaptation, ConfigError, LearnedTiming, LearningModel, PatternConfig, RegisterError,
    TaskConfig, Timing,
};

fn train_three_phase_pattern() -> (Monitor, Heartbeat, u64) {
    let monitor = Monitor::new();
    let pattern = PatternConfig::new(4, 3)
        .with_tolerance_percent(10)
        .with_minimum_contrast_percent(20);
    let learned = LearnedTiming::default()
        .with_minimum_samples(9)
        .with_startup_grace(Duration::from_secs(1))
        .with_minimum_grace(Duration::from_micros(100))
        .with_adaptation(Adaptation::FrozenAfterTraining)
        .with_model(LearningModel::RepeatingPattern(pattern));
    let heartbeat = monitor
        .register(TaskConfig::new("pattern-worker", Timing::Learned(learned)))
        .expect("valid pattern registration");
    let start = heartbeat.entry.created_tick;

    for offset in [
        1_000, 2_000, 3_000, 11_000, 12_000, 13_000, 21_000, 22_000, 23_000, 31_000,
    ] {
        heartbeat
            .beat_at(start + offset)
            .expect("pattern heartbeat accepted");
    }

    (monitor, heartbeat, start)
}

#[test]
fn repeating_pattern_learns_three_phase_cycle() {
    let (monitor, heartbeat, start) = train_three_phase_pattern();
    let status = monitor
        .status_at(heartbeat.id(), start + 31_500)
        .expect("status exists");

    assert!(matches!(status.health, Health::Healthy { .. }));
    assert!(matches!(
        status.timing,
        TimingStatus::PatternLearned {
            period: 3,
            next_phase: 0,
            ref intervals,
            expected_interval,
            ..
        } if intervals.as_ref()
            == [
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(8),
            ]
            && expected_interval == Duration::from_millis(1)
    ));
}

#[test]
fn pattern_deadline_follows_the_next_phase() {
    let (monitor, heartbeat, start) = train_three_phase_pattern();

    let short_phase_healthy = monitor
        .status_at(heartbeat.id(), start + 32_050)
        .expect("status exists");
    assert!(matches!(short_phase_healthy.health, Health::Healthy { .. }));

    let short_phase_late = monitor
        .status_at(heartbeat.id(), start + 32_200)
        .expect("status exists");
    assert!(matches!(short_phase_late.health, Health::Late { .. }));

    heartbeat
        .beat_at(start + 32_000)
        .expect("first short phase accepted");
    heartbeat
        .beat_at(start + 33_000)
        .expect("second short phase accepted");

    let long_phase_healthy = monitor
        .status_at(heartbeat.id(), start + 40_500)
        .expect("status exists");
    assert!(matches!(long_phase_healthy.health, Health::Healthy { .. }));

    let long_phase_late = monitor
        .status_at(heartbeat.id(), start + 41_200)
        .expect("status exists");
    assert!(matches!(long_phase_late.health, Health::Late { .. }));
}

#[test]
fn constant_frequency_is_not_mislabeled_as_a_pattern() {
    let monitor = Monitor::new();
    let pattern = PatternConfig::new(4, 3).with_minimum_contrast_percent(20);
    let learned = LearnedTiming::default()
        .with_minimum_samples(8)
        .with_startup_grace(Duration::from_secs(1))
        .with_model(LearningModel::RepeatingPattern(pattern));
    let heartbeat = monitor
        .register(TaskConfig::new("constant-worker", Timing::Learned(learned)))
        .expect("valid pattern registration");
    let start = heartbeat.entry.created_tick;

    for offset in (1_u64..=21).map(|value| value * 1_000) {
        heartbeat
            .beat_at(start + offset)
            .expect("constant heartbeat accepted");
    }

    let status = monitor
        .status_at(heartbeat.id(), start + 21_500)
        .expect("status exists");
    assert!(matches!(
        status.timing,
        TimingStatus::PatternLearning {
            candidate_period: None,
            ..
        }
    ));
}

#[test]
fn pattern_outlier_is_rejected_and_phase_advances() {
    let (monitor, heartbeat, start) = train_three_phase_pattern();

    heartbeat
        .beat_at(start + 81_000)
        .expect("outlier remains a liveness signal");
    let after_outlier = monitor
        .status_at(heartbeat.id(), start + 81_100)
        .expect("status exists");
    assert!(matches!(
        after_outlier.timing,
        TimingStatus::PatternLearned {
            rejected_samples: 1,
            next_phase: 1,
            expected_interval,
            ..
        } if expected_interval == Duration::from_millis(1)
    ));

    heartbeat
        .beat_at(start + 82_000)
        .expect("next phase heartbeat accepted");
    let long_phase = monitor
        .status_at(heartbeat.id(), start + 82_100)
        .expect("status exists");
    assert!(matches!(
        long_phase.timing,
        TimingStatus::PatternLearned {
            rejected_samples: 1,
            next_phase: 2,
            expected_interval,
            ..
        } if expected_interval == Duration::from_millis(8)
    ));
}

#[test]
fn retraining_discards_a_pattern_cycle() {
    let (monitor, heartbeat, start) = train_three_phase_pattern();
    monitor
        .retrain(heartbeat.id())
        .expect("pattern task can retrain");

    let status = monitor
        .status_at(heartbeat.id(), start + 31_500)
        .expect("status exists");
    assert!(matches!(
        status.timing,
        TimingStatus::PatternLearning {
            samples: 0,
            candidate_period: None,
            ..
        }
    ));
}

#[test]
fn invalid_pattern_configuration_is_rejected() {
    let monitor = Monitor::new();
    let learned = LearnedTiming::default()
        .with_model(LearningModel::RepeatingPattern(PatternConfig::new(1, 3)));
    let error = monitor
        .register(TaskConfig::new("invalid-pattern", Timing::Learned(learned)))
        .err()
        .expect("registration must fail");

    assert_eq!(
        error,
        RegisterError::InvalidConfig(ConfigError::InvalidPatternMaximumPeriod)
    );
}
