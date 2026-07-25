#[derive(Clone, Copy, Debug)]
struct PatternCandidate {
    period: u8,
    centers: [f64; MAX_PATTERN_PERIOD],
    deviations: [f64; MAX_PATTERN_PERIOD],
    residual_ratio: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatternObservation {
    Accepted,
    Rejected,
}

#[derive(Debug)]
struct PatternState {
    config: PatternConfig,
    values: [u64; MAX_PATTERN_SAMPLES],
    len: u8,
    cursor: u8,
    total_samples: u32,
    period: u8,
    next_phase: u8,
    centers: [f64; MAX_PATTERN_PERIOD],
    deviations: [f64; MAX_PATTERN_PERIOD],
    best_candidate: Option<u8>,
    fit_residual_ratio: f64,
}

impl PatternState {
    fn new(config: PatternConfig) -> Self {
        Self {
            config,
            values: [0; MAX_PATTERN_SAMPLES],
            len: 0,
            cursor: 0,
            total_samples: 0,
            period: 0,
            next_phase: 0,
            centers: [0.0; MAX_PATTERN_PERIOD],
            deviations: [0.0; MAX_PATTERN_PERIOD],
            best_candidate: None,
            fit_residual_ratio: f64::INFINITY,
        }
    }

    fn observe(&mut self, observed: f64, timing: LearnedTiming) -> PatternObservation {
        if self.is_trained() {
            return self.observe_trained(observed, timing);
        }

        self.push(observed.round().min(u64::MAX as f64) as u64);
        let candidate = self.best_pattern(timing.minimum_samples);
        self.best_candidate = candidate.map(|candidate| candidate.period);
        if let Some(candidate) = candidate {
            let tolerance = f64::from(self.config.tolerance_percent) / 100.0;
            if candidate.residual_ratio <= tolerance {
                self.train(candidate);
            }
        }

        PatternObservation::Accepted
    }

    fn observe_trained(&mut self, observed: f64, timing: LearnedTiming) -> PatternObservation {
        let phase = usize::from(self.next_phase);
        let center = self.centers[phase];
        let deviation = self.deviations[phase];
        let allowance = (timing.minimum_grace.as_micros() as f64)
            .max(timing.sensitivity * deviation);
        let lower = (center - allowance).max(1.0);
        let upper = center + allowance;
        let accepted = (lower..=upper).contains(&observed);

        if accepted {
            let alpha = timing.adaptation.alpha();
            if alpha > 0.0 {
                let delta = observed - center;
                self.centers[phase] += alpha * delta;
                self.deviations[phase] += alpha * (delta.abs() - deviation);
                self.deviations[phase] = self.deviations[phase].max(0.0);
            }
        }

        self.next_phase = (self.next_phase + 1) % self.period;
        if accepted {
            PatternObservation::Accepted
        } else {
            PatternObservation::Rejected
        }
    }

    fn push(&mut self, observed_micros: u64) {
        let cursor = usize::from(self.cursor);
        self.values[cursor] = observed_micros;
        self.cursor = ((cursor + 1) % MAX_PATTERN_SAMPLES) as u8;
        self.len = self.len.saturating_add(1).min(MAX_PATTERN_SAMPLES as u8);
        self.total_samples = self.total_samples.saturating_add(1);
    }

    fn logical_value(&self, index: usize) -> u64 {
        let len = usize::from(self.len);
        let start = (usize::from(self.cursor) + MAX_PATTERN_SAMPLES - len) % MAX_PATTERN_SAMPLES;
        self.values[(start + index) % MAX_PATTERN_SAMPLES]
    }

    fn best_pattern(&self, minimum_samples: u32) -> Option<PatternCandidate> {
        let mut best = None;
        let maximum_period = usize::from(self.config.maximum_period);
        let minimum_cycles = usize::from(self.config.minimum_cycles);
        let minimum_contrast = f64::from(self.config.minimum_contrast_percent) / 100.0;

        for period in 2..=maximum_period {
            let required = minimum_samples.max((period * minimum_cycles) as u32);
            if self.total_samples < required || usize::from(self.len) < period * minimum_cycles {
                continue;
            }

            let candidate = self.analyze_period(period);
            let active_centers = &candidate.centers[..period];
            let mean = active_centers.iter().sum::<f64>() / period as f64;
            let minimum = active_centers.iter().copied().fold(f64::INFINITY, f64::min);
            let maximum = active_centers.iter().copied().fold(0.0, f64::max);
            let contrast = (maximum - minimum) / mean.max(1.0);
            if contrast < minimum_contrast {
                continue;
            }

            let replace = match best {
                None => true,
                Some(current) => {
                    candidate.residual_ratio < current.residual_ratio
                        || ((candidate.residual_ratio - current.residual_ratio).abs()
                            <= f64::EPSILON
                            && candidate.period < current.period)
                }
            };
            if replace {
                best = Some(candidate);
            }
        }

        best
    }

    fn analyze_period(&self, period: usize) -> PatternCandidate {
        let len = usize::from(self.len);
        let usable_len = (len / period) * period;
        let start_offset = len - usable_len;
        let oldest_global = self.total_samples as usize - len;
        let mut phase_values = [[0_u64; MAX_PATTERN_SAMPLES]; MAX_PATTERN_PERIOD];
        let mut phase_counts = [0_usize; MAX_PATTERN_PERIOD];

        for logical_index in start_offset..len {
            let global_index = oldest_global + logical_index;
            let phase = global_index % period;
            let count = phase_counts[phase];
            phase_values[phase][count] = self.logical_value(logical_index);
            phase_counts[phase] += 1;
        }

        let mut centers = [0.0; MAX_PATTERN_PERIOD];
        let mut deviations = [0.0; MAX_PATTERN_PERIOD];
        for phase in 0..period {
            let count = phase_counts[phase];
            centers[phase] = median(&mut phase_values[phase][..count]);

            let mut absolute_deviations = [0_u64; MAX_PATTERN_SAMPLES];
            let rounded_center = centers[phase].round() as u64;
            for (target, value) in absolute_deviations
                .iter_mut()
                .zip(phase_values[phase].iter())
                .take(count)
            {
                *target = value.abs_diff(rounded_center);
            }
            deviations[phase] = median(&mut absolute_deviations[..count]) * 1.4826;
        }

        let mut residuals = [0.0_f64; MAX_PATTERN_SAMPLES];
        let mut residual_count = 0;
        for logical_index in start_offset..len {
            let global_index = oldest_global + logical_index;
            let phase = global_index % period;
            let value = self.logical_value(logical_index) as f64;
            residuals[residual_count] = (value - centers[phase]).abs() / centers[phase].max(1.0);
            residual_count += 1;
        }

        PatternCandidate {
            period: period as u8,
            centers,
            deviations,
            residual_ratio: median_f64(&mut residuals[..residual_count]),
        }
    }

    fn train(&mut self, candidate: PatternCandidate) {
        self.period = candidate.period;
        self.next_phase = (self.total_samples % u32::from(candidate.period)) as u8;
        self.centers = candidate.centers;
        self.deviations = candidate.deviations;
        self.fit_residual_ratio = candidate.residual_ratio;
    }

    fn is_trained(&self) -> bool {
        self.period > 0
    }

    fn expected_center(&self) -> Option<f64> {
        self.is_trained()
            .then_some(self.centers[usize::from(self.next_phase)])
    }

    fn expected_deviation(&self) -> Option<f64> {
        self.is_trained()
            .then_some(self.deviations[usize::from(self.next_phase)])
    }

    fn intervals(&self) -> Option<Vec<Duration>> {
        self.is_trained().then(|| {
            self.centers[..usize::from(self.period)]
                .iter()
                .copied()
                .map(duration_from_micros)
                .collect()
        })
    }

    fn phase_deviations(&self) -> Option<Vec<Duration>> {
        self.is_trained().then(|| {
            self.deviations[..usize::from(self.period)]
                .iter()
                .copied()
                .map(duration_from_micros)
                .collect()
        })
    }
}
