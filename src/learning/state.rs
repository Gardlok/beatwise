#[derive(Debug)]
enum EstimatorState {
    Ewma(EwmaState),
    Robust(Box<RobustWindowState>),
    Pattern(Box<PatternState>),
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
            LearningModel::RepeatingPattern(config) => {
                EstimatorState::Pattern(Box::new(PatternState::new(config)))
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

    pub(crate) fn reset_for_retraining(&mut self, model: LearningModel, skip_next_interval: bool) {
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

        let pattern_result = match &mut self.estimator {
            EstimatorState::Pattern(state) => {
                let result = state.observe(observed, config);
                Some((result, state.is_trained()))
            }
            EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        };
        if let Some((result, trained)) = pattern_result {
            match result {
                PatternObservation::Accepted => {
                    self.accepted_samples = self.accepted_samples.saturating_add(1);
                }
                PatternObservation::Rejected => {
                    self.rejected_samples = self.rejected_samples.saturating_add(1);
                }
            }
            self.trained = trained;
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
                EstimatorState::Pattern(_) => unreachable!("pattern observations return early"),
            }
        }

        self.accepted_samples = self.accepted_samples.saturating_add(1);
        self.trained = self.accepted_samples >= config.minimum_samples;
    }

    fn should_adapt(&self, config: LearnedTiming) -> bool {
        match &self.estimator {
            EstimatorState::Ewma(_) => config.adaptation.alpha() > 0.0,
            EstimatorState::Robust(_) => {
                config
                    .adaptation
                    .robust_update_stride()
                    .is_some_and(|stride| {
                        let post_training =
                            self.accepted_samples.saturating_sub(config.minimum_samples);
                        post_training.checked_rem(stride) == Some(0)
                    })
            }
            EstimatorState::Pattern(_) => false,
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
            EstimatorState::Pattern(state) => LearningModel::RepeatingPattern(state.config),
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
            EstimatorState::Ewma(state) => (self.accepted_samples > 0).then_some(state.mean_micros),
            EstimatorState::Robust(state) => state.center_micros(),
            EstimatorState::Pattern(state) => state.expected_center(),
        }
    }

    fn deviation_micros(&self) -> Option<f64> {
        match &self.estimator {
            EstimatorState::Ewma(state) => {
                (self.accepted_samples > 0).then_some(state.deviation_micros)
            }
            EstimatorState::Robust(state) => state.deviation_micros(),
            EstimatorState::Pattern(state) => state.expected_deviation(),
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

    pub(crate) fn pattern_config(&self) -> Option<PatternConfig> {
        match &self.estimator {
            EstimatorState::Pattern(state) => Some(state.config),
            EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn pattern_candidate_period(&self) -> Option<u8> {
        match &self.estimator {
            EstimatorState::Pattern(state) => state.best_candidate,
            EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn pattern_period(&self) -> Option<u8> {
        match &self.estimator {
            EstimatorState::Pattern(state) if state.is_trained() => Some(state.period),
            EstimatorState::Pattern(_) | EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn pattern_next_phase(&self) -> Option<u8> {
        match &self.estimator {
            EstimatorState::Pattern(state) if state.is_trained() => Some(state.next_phase),
            EstimatorState::Pattern(_) | EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn pattern_intervals(&self) -> Option<Vec<Duration>> {
        match &self.estimator {
            EstimatorState::Pattern(state) => state.intervals(),
            EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn pattern_deviations(&self) -> Option<Vec<Duration>> {
        match &self.estimator {
            EstimatorState::Pattern(state) => state.phase_deviations(),
            EstimatorState::Ewma(_) | EstimatorState::Robust(_) => None,
        }
    }

    pub(crate) fn confidence(&self, config: LearnedTiming) -> Confidence {
        if !self.trained {
            return Confidence::Insufficient;
        }

        if let EstimatorState::Pattern(state) = &self.estimator {
            let period = u32::from(state.period).max(1);
            let cycles = self.accepted_samples / period;
            let minimum_cycles = u32::from(state.config.minimum_cycles);
            let rejected_ratio = f64::from(self.rejected_samples)
                / f64::from(
                    self.accepted_samples
                        .saturating_add(self.rejected_samples)
                        .max(1),
                );
            let tolerance = f64::from(state.config.tolerance_percent) / 100.0;

            if cycles >= minimum_cycles.saturating_mul(3)
                && state.fit_residual_ratio <= tolerance / 2.0
                && rejected_ratio <= 0.10
            {
                return Confidence::High;
            }
            if cycles >= minimum_cycles.saturating_mul(2)
                && state.fit_residual_ratio <= tolerance
                && rejected_ratio <= 0.20
            {
                return Confidence::Medium;
            }
            return Confidence::Low;
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
