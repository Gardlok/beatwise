pub(crate) const MAX_ROBUST_WINDOW: usize = 31;
pub(crate) const MAX_PATTERN_PERIOD: usize = 8;
pub(crate) const MAX_PATTERN_CYCLES: usize = 8;

/// Controls how a learned model changes after its training phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Adaptation {
    /// Keep the learned baseline fixed after the minimum sample count is met.
    FrozenAfterTraining,

    /// Accept healthy observations with a conservative adaptation rate.
    #[default]
    Slow,

    /// Accept healthy observations with a faster adaptation rate.
    Continuous,
}

impl Adaptation {
    pub(crate) fn alpha(self) -> f64 {
        match self {
            Self::FrozenAfterTraining => 0.0,
            Self::Slow => 0.05,
            Self::Continuous => 0.20,
        }
    }

    pub(crate) fn robust_update_stride(self) -> Option<u32> {
        match self {
            Self::FrozenAfterTraining => None,
            Self::Slow => Some(4),
            Self::Continuous => Some(1),
        }
    }
}

/// Configuration for bounded repeating-pattern discovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternConfig {
    pub(crate) maximum_period: u8,
    pub(crate) minimum_cycles: u8,
    pub(crate) tolerance_percent: u8,
    pub(crate) minimum_contrast_percent: u8,
}

impl PatternConfig {
    /// Creates a repeating-pattern configuration.
    ///
    /// `maximum_period` is the largest number of distinct heartbeat intervals
    /// that may form one cycle. `minimum_cycles` controls how many repetitions
    /// must be observed before a candidate cycle can become trusted.
    #[must_use]
    pub const fn new(maximum_period: u8, minimum_cycles: u8) -> Self {
        Self {
            maximum_period,
            minimum_cycles,
            tolerance_percent: 10,
            minimum_contrast_percent: 20,
        }
    }

    /// Sets the maximum median relative residual accepted for a candidate cycle.
    #[must_use]
    pub const fn with_tolerance_percent(mut self, tolerance_percent: u8) -> Self {
        self.tolerance_percent = tolerance_percent;
        self
    }

    /// Sets the minimum relative spread required between learned phases.
    ///
    /// This prevents a constant-frequency process from being mislabeled as a
    /// repeating multi-phase pattern.
    #[must_use]
    pub const fn with_minimum_contrast_percent(
        mut self,
        minimum_contrast_percent: u8,
    ) -> Self {
        self.minimum_contrast_percent = minimum_contrast_percent;
        self
    }

    /// Returns the largest cycle length considered by the detector.
    #[must_use]
    pub const fn maximum_period(self) -> u8 {
        self.maximum_period
    }

    /// Returns the minimum number of complete cycles required for training.
    #[must_use]
    pub const fn minimum_cycles(self) -> u8 {
        self.minimum_cycles
    }

    /// Returns the candidate-fit tolerance percentage.
    #[must_use]
    pub const fn tolerance_percent(self) -> u8 {
        self.tolerance_percent
    }

    /// Returns the minimum phase-contrast percentage.
    #[must_use]
    pub const fn minimum_contrast_percent(self) -> u8 {
        self.minimum_contrast_percent
    }
}

/// Statistical model used to learn a task's heartbeat timing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LearningModel {
    /// Constant-memory exponentially weighted moving statistics.
    #[default]
    Ewma,

    /// A bounded median and median-absolute-deviation window.
    ///
    /// The capacity must be an odd number from 5 through 31.
    RobustWindow { capacity: u8 },

    /// Discover a bounded repeating sequence of heartbeat intervals.
    RepeatingPattern(PatternConfig),
}

impl LearningModel {
    /// Creates a bounded robust window model.
    #[must_use]
    pub const fn robust_window(capacity: u8) -> Self {
        Self::RobustWindow { capacity }
    }

    /// Creates a bounded repeating-pattern model with default fit thresholds.
    #[must_use]
    pub const fn repeating_pattern(maximum_period: u8, minimum_cycles: u8) -> Self {
        Self::RepeatingPattern(PatternConfig::new(maximum_period, minimum_cycles))
    }
}
