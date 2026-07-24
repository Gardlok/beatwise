use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use crate::{
    config::{LearnedTiming, LearningModel, MAX_ROBUST_WINDOW},
    status::Confidence,
};

#[derive(Clone, Copy, Debug, Default)]
struct EwmaState {
    mean_micros: f64,
    deviation_micros: f64,
}

impl EwmaState {
    fn update(&mut self, observed: f64, alpha: f64) {
        if self.mean_micros <= 0.0 {
            self.mean_micros = observed;
            self.deviation_micros = 0.0;
            return;
        }

        let delta = observed - self.mean_micros;
        self.mean_micros += alpha * delta;
        self.deviation_micros += alpha * (delta.abs() - self.deviation_micros);
        self.deviation_micros = self.deviation_micros.max(0.0);
    }
}

#[derive(Clone, Copy, Debug)]
struct RobustWindowState {
    values: [u64; MAX_ROBUST_WINDOW],
    capacity: u8,
    len: u8,
    cursor: u8,
}

impl RobustWindowState {
    fn new(capacity: u8) -> Self {
        Self {
            values: [0; MAX_ROBUST_WINDOW],
            capacity,
            len: 0,
            cursor: 0,
        }
    }

    fn push(&mut self, observed_micros: u64) {
        let cursor = usize::from(self.cursor);
        self.values[cursor] = observed_micros;

        if self.len < self.capacity {
            self.len += 1;
        }

        self.cursor = (self.cursor + 1) % self.capacity;
    }

    fn center_micros(&self) -> Option<f64> {
        let len = usize::from(self.len);
        if len == 0 {
            return None;
        }

        let mut values = self.values;
        Some(median(&mut values[..len]))
    }

    fn deviation_micros(&self) -> Option<f64> {
        let len = usize::from(self.len);
        let center = self.center_micros()?;
        let mut deviations = [0_u64; MAX_ROBUST_WINDOW];

        for (target, value) in deviations.iter_mut().zip(self.values.iter()).take(len) {
            *target = value.abs_diff(center.round() as u64);
        }

        // 1.4826 scales MAD toward a standard-deviation-like quantity for
        // normally distributed samples while retaining robust outlier behavior.
        Some(median(&mut deviations[..len]) * 1.4826)
    }
}

#[derive(Debug)]
enum EstimatorState {
    Ewma(EwmaState),
    Robust(Box<RobustWindowState>),
}

#[derive(Debug)]
pub(crate) struct LearningState {
    accepted_samples: u32,
    rejected_samples: u32,
    trained: bool,
    skip_next_interval: bool,
    estimator: EstimatorState,
}

impl LearningState {
    pub(crate) fn new(model: LearningModel) -> Self {
        let estimator = match model {
            LearningModel::Ewma => EstimatorState::Ewma(EwmaState::default()),
            LearningModel::RobustWindow { capacity } => {
                EstimatorState::Robust(Box::new(RobustWindowState::new(capacity)))
            }
        };

        Self {
            accepted_samples: 0,
            rejected_samples: 0,
            trained: false,
            skip_next_interval: false,
            estimator,
        }
    }

    pub(crate) fn reset_for_retraining(
        &mut self,
        model: LearningModel,
        skip_next_interval: bool,
    ) {
        *self = Self::new(model);
        self.skip_next_interval = skip_next_interval;
    }

    pub(crate) fn observe(&mut self, interval: Duration, config: LearnedTiming) {
        if self.skip_next_interval {
            self.skip_next_interval = false;
            return;
        }

        let observed = interval.as_micros() as f64;
        if observed <= 0.0 || !observed.is_finite() {
            return;
        }

        if self.trained && !self.within_trusted_range(observed, config) {
            self.rejected_samples = self.rejected_samples.saturating_add(1);
            return;
        }

        let should_update = !self.trained || self.should_adapt(config);
        if should_update {
            let accepted = if self.trained {
                observed
            } else {
                self.clamp_training_observation(observed, config)
            };
            let alpha = if self.trained {
                config.adaptation.alpha()
            } else {
                2.0 / (f64::from(config.minimum_samples) + 1.0)
            };

            match &mut self.estimator {
                EstimatorState::Ewma(state) => state.update(accepted, alpha),
                EstimatorState::Robust(state) => {
                    state.push(observed.round().min(u64::MAX as f64) as u64);
                }
            }
        }

        self.accepted_samples = self.accepted_samples.saturating_add(1);
        self.trained = self.accepted_samples >= config.minimum_samples;
    }

    fn should_adapt(&self, config: LearnedTiming) -> bool {
        match &self.estimator {
            EstimatorState::Ewma(_) => config.adaptation.alpha() > 0.0,
            EstimatorState::Robust(_) => config
                .adaptation
                .robust_update_stride()
                .is_some_and(|stride| {
                    let post_training = self
                        .accepted_samples
                        .saturating_sub(config.minimum_samples);
                    post_training.checked_rem(stride) == Some(0)
                }),
        }
    }

    fn clamp_training_observation(&self, observed: f64, config: LearnedTiming) -> f64 {
        if self.accepted_samples < 3 {
            return observed;
        }

        let Some(center) = self.center_micros() else {
            return observed;
        };
        let allowance = self.allowance_micros(config);
        let lower = (center - allowance).max(1.0);
        let upper = center + allowance;
        observed.clamp(lower, upper)
    }

    fn within_trusted_range(&self, observed: f64, config: LearnedTiming) -> bool {
        let Some(center) = self.center_micros() else {
            return true;
        };
        let allowance = self.allowance_micros(config);
        let lower = (center - allowance).max(1.0);
        let upper = center + allowance;
        (lower..=upper).contains(&observed)
    }

    pub(crate) fn model(&self) -> LearningModel {
        match &self.estimator {
            EstimatorState::Ewma(_) => LearningModel::Ewma,
            EstimatorState::Robust(state) => LearningModel::RobustWindow {
                capacity: state.capacity,
            },
        }
    }

    pub(crate) fn samples(&self) -> u32 {
        self.accepted_samples
    }

    pub(crate) fn rejected_samples(&self) -> u32 {
        self.rejected_samples
    }

    pub(crate) fn is_trained(&self) -> bool {
        self.trained
    }

    fn center_micros(&self) -> Option<f64> {
        match &self.estimator {
            EstimatorState::Ewma(state) => {
                (self.accepted_samples > 0).then_some(state.mean_micros)
            }
            EstimatorState::Robust(state) => state.center_micros(),
        }
    }

    fn deviation_micros(&self) -> Option<f64> {
        match &self.estimator {
            EstimatorState::Ewma(state) => {
                (self.accepted_samples > 0).then_some(state.deviation_micros)
            }
            EstimatorState::Robust(state) => state.deviation_micros(),
        }
    }

    fn allowance_micros(&self, config: LearnedTiming) -> f64 {
        let minimum = config.minimum_grace.as_micros() as f64;
        let deviation = self.deviation_micros().unwrap_or(0.0);
        minimum.max(config.sensitivity * deviation)
    }

    fn deadline_micros(&self, config: LearnedTiming) -> Option<f64> {
        self.center_micros()
            .map(|center| center + self.allowance_micros(config))
    }

    pub(crate) fn estimated_interval(&self) -> Option<Duration> {
        self.center_micros().map(duration_from_micros)
    }

    pub(crate) fn estimated_deviation(&self) -> Option<Duration> {
        self.deviation_micros().map(duration_from_micros)
    }

    pub(crate) fn deadline(&self, config: LearnedTiming) -> Option<Duration> {
        self.deadline_micros(config).map(duration_from_micros)
    }

    pub(crate) fn confidence(&self, config: LearnedTiming) -> Confidence {
        if !self.trained {
            return Confidence::Insufficient;
        }

        let Some(center) = self.center_micros() else {
            return Confidence::Insufficient;
        };
        let relative_deviation = self.deviation_micros().unwrap_or(0.0) / center.max(1.0);
        let rejected_ratio = f64::from(self.rejected_samples)
            / f64::from(
                self.accepted_samples
                    .saturating_add(self.rejected_samples)
                    .max(1),
            );

        if self.accepted_samples >= config.minimum_samples.saturating_mul(4)
            && relative_deviation <= 0.05
            && rejected_ratio <= 0.10
        {
            Confidence::High
        } else if self.accepted_samples >= config.minimum_samples.saturating_mul(2)
            && relative_deviation <= 0.15
            && rejected_ratio <= 0.20
        {
            Confidence::Medium
        } else {
            Confidence::Low
        }
    }
}

pub(crate) fn lock_learning(
    state: &Mutex<LearningState>,
) -> MutexGuard<'_, LearningState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn median(values: &mut [u64]) -> f64 {
    values.sort_unstable();
    let midpoint = values.len() / 2;

    if values.len() & 1 == 0 {
        (values[midpoint - 1] as f64 + values[midpoint] as f64) / 2.0
    } else {
        values[midpoint] as f64
    }
}

fn duration_from_micros(micros: f64) -> Duration {
    if !micros.is_finite() || micros <= 0.0 {
        return Duration::ZERO;
    }

    Duration::from_micros(micros.round().min(u64::MAX as f64) as u64)
}
