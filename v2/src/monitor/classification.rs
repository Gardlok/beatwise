fn status_for(entry: &TaskEntry, now: u64) -> TaskStatus {
    let heartbeat_count = entry.heartbeat_count.load(Ordering::Acquire);
    let (health, timing) = if entry.lifecycle.load(Ordering::Acquire) == STOPPED {
        let stopped_tick = entry.stopped_tick.load(Ordering::Acquire);
        let reason = match entry.stop_reason.load(Ordering::Acquire) {
            STOP_EXPLICIT => StopReason::Explicit,
            _ => StopReason::LastHandleDropped,
        };

        (
            Health::Stopped {
                stopped_for: tick_duration(now, stopped_tick),
                reason,
            },
            timing_status(&entry.timing),
        )
    } else {
        running_status(entry, now)
    };

    TaskStatus {
        id: entry.id,
        name: Arc::clone(&entry.name),
        health,
        timing,
        heartbeat_count,
    }
}

fn running_status(entry: &TaskEntry, now: u64) -> (Health, TimingStatus) {
    let last_tick = entry.last_tick.load(Ordering::Acquire);

    if last_tick == NO_TICK {
        let elapsed = tick_duration(now, entry.created_tick);
        let health = if elapsed <= entry.startup_grace {
            Health::Starting {
                elapsed,
                startup_grace: entry.startup_grace,
            }
        } else {
            Health::Late {
                silent_for: elapsed,
                deadline: entry.startup_grace,
                overdue_by: elapsed.saturating_sub(entry.startup_grace),
                missed_intervals: 0,
            }
        };

        return (health, timing_status(&entry.timing));
    }

    let silent_for = tick_duration(now, last_tick);

    match &entry.timing {
        TimingState::Fixed(fixed) => {
            let deadline = fixed.deadline();
            let health = classify_health(silent_for, deadline, fixed.interval);
            (
                health,
                TimingStatus::Fixed {
                    interval: fixed.interval,
                    grace: fixed.grace,
                },
            )
        }
        TimingState::Learned { config, state } => {
            let state = lock_learning(state);
            let status = learned_timing_status(*config, &state);

            if state.is_trained() {
                let interval = state
                    .estimated_interval()
                    .expect("trained learned timing must have an interval");
                let deadline = state
                    .deadline(*config)
                    .expect("trained learned timing must have a deadline");
                return (classify_health(silent_for, deadline, interval), status);
            }

            let provisional_deadline = state.deadline(*config).unwrap_or(entry.startup_grace);
            if silent_for > provisional_deadline {
                let interval = state.estimated_interval().unwrap_or(provisional_deadline);
                return (
                    classify_health(silent_for, provisional_deadline, interval),
                    status,
                );
            }

            (
                Health::Learning {
                    samples: state.samples(),
                    required: config.minimum_samples,
                    silent_for,
                    estimated_interval: state.estimated_interval(),
                },
                status,
            )
        }
    }
}

fn classify_health(silent_for: Duration, deadline: Duration, interval: Duration) -> Health {
    if silent_for <= deadline {
        Health::Healthy {
            silent_for,
            deadline,
        }
    } else {
        Health::Late {
            silent_for,
            deadline,
            overdue_by: silent_for.saturating_sub(deadline),
            missed_intervals: interval_count(silent_for, interval),
        }
    }
}

fn timing_status(timing: &TimingState) -> TimingStatus {
    match timing {
        TimingState::Fixed(fixed) => TimingStatus::Fixed {
            interval: fixed.interval,
            grace: fixed.grace,
        },
        TimingState::Learned { config, state } => {
            let state = lock_learning(state);
            learned_timing_status(*config, &state)
        }
    }
}

fn learned_timing_status(config: LearnedTiming, state: &LearningState) -> TimingStatus {
    let confidence = state.confidence(config);

    if let Some(pattern_config) = state.pattern_config() {
        if state.is_trained() {
            return TimingStatus::PatternLearned {
                config: pattern_config,
                samples: state.samples(),
                rejected_samples: state.rejected_samples(),
                period: state
                    .pattern_period()
                    .expect("trained pattern timing must have a period"),
                next_phase: state
                    .pattern_next_phase()
                    .expect("trained pattern timing must have a next phase"),
                intervals: Arc::from(
                    state
                        .pattern_intervals()
                        .expect("trained pattern timing must have phase intervals"),
                ),
                deviations: Arc::from(
                    state
                        .pattern_deviations()
                        .expect("trained pattern timing must have phase deviations"),
                ),
                expected_interval: state
                    .estimated_interval()
                    .expect("trained pattern timing must have an expected interval"),
                deadline: state
                    .deadline(config)
                    .expect("trained pattern timing must have a deadline"),
                adaptation: config.adaptation,
                confidence,
            };
        }

        return TimingStatus::PatternLearning {
            config: pattern_config,
            samples: state.samples(),
            rejected_samples: state.rejected_samples(),
            minimum_samples: config.minimum_samples,
            candidate_period: state.pattern_candidate_period(),
            confidence,
        };
    }

    if state.is_trained() {
        TimingStatus::Learned {
            model: state.model(),
            samples: state.samples(),
            rejected_samples: state.rejected_samples(),
            interval: state
                .estimated_interval()
                .expect("trained learned timing must have an interval"),
            deviation: state
                .estimated_deviation()
                .expect("trained learned timing must have a deviation"),
            deadline: state
                .deadline(config)
                .expect("trained learned timing must have a deadline"),
            adaptation: config.adaptation,
            confidence,
        }
    } else {
        TimingStatus::Learning {
            model: state.model(),
            samples: state.samples(),
            rejected_samples: state.rejected_samples(),
            required: config.minimum_samples,
            estimated_interval: state.estimated_interval(),
            estimated_deviation: state.estimated_deviation(),
            confidence,
        }
    }
}

fn tick_duration(later: u64, earlier: u64) -> Duration {
    Duration::from_micros(later.saturating_sub(earlier))
}

fn interval_count(elapsed: Duration, interval: Duration) -> u64 {
    if interval.is_zero() {
        return 0;
    }

    (elapsed.as_nanos() / interval.as_nanos()).min(u128::from(u64::MAX)) as u64
}
