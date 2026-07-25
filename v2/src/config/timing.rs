/// Fixed timing configuration for a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedTiming {
    pub(crate) interval: Duration,
    pub(crate) grace: Duration,
}

impl FixedTiming {
    /// Creates fixed timing with no additional grace period.
    #[must_use]
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            grace: Duration::ZERO,
        }
    }

    /// Adds time beyond the expected interval before the task becomes late.
    #[must_use]
    pub const fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// Returns the expected heartbeat interval.
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// Returns the configured grace period.
    #[must_use]
    pub const fn grace(self) -> Duration {
        self.grace
    }

    pub(crate) fn deadline(self) -> Duration {
        self.interval.saturating_add(self.grace)
    }
}

/// Learned timing configuration for a task.
///
/// The default model uses an exponentially weighted moving mean and absolute
/// deviation. Callers may instead select a bounded robust window or bounded
/// repeating-pattern discovery. All trained models reject observations outside
/// their trusted range while still treating those beats as liveness signals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LearnedTiming {
    pub(crate) minimum_samples: u32,
    pub(crate) startup_grace: Duration,
    pub(crate) minimum_grace: Duration,
    pub(crate) sensitivity: f64,
    pub(crate) adaptation: Adaptation,
    pub(crate) model: LearningModel,
}

impl Default for LearnedTiming {
    fn default() -> Self {
        Self {
            minimum_samples: 8,
            startup_grace: Duration::from_secs(30),
            minimum_grace: Duration::from_millis(10),
            sensitivity: 4.0,
            adaptation: Adaptation::Slow,
            model: LearningModel::Ewma,
        }
    }
}

impl LearnedTiming {
    /// Sets the number of accepted interval samples required for training.
    #[must_use]
    pub const fn with_minimum_samples(mut self, minimum_samples: u32) -> Self {
        self.minimum_samples = minimum_samples;
        self
    }

    /// Sets the maximum initial silence allowed before any baseline exists.
    #[must_use]
    pub const fn with_startup_grace(mut self, startup_grace: Duration) -> Self {
        self.startup_grace = startup_grace;
        self
    }

    /// Sets the smallest grace period used around the learned interval.
    #[must_use]
    pub const fn with_minimum_grace(mut self, minimum_grace: Duration) -> Self {
        self.minimum_grace = minimum_grace;
        self
    }

    /// Sets how many learned deviations are accepted around the center.
    #[must_use]
    pub const fn with_sensitivity(mut self, sensitivity: f64) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Sets how the trusted model changes after training.
    #[must_use]
    pub const fn with_adaptation(mut self, adaptation: Adaptation) -> Self {
        self.adaptation = adaptation;
        self
    }

    /// Selects the statistical model used for learning.
    #[must_use]
    pub const fn with_model(mut self, model: LearningModel) -> Self {
        self.model = model;
        self
    }

    /// Returns the minimum number of interval samples required for training.
    #[must_use]
    pub const fn minimum_samples(self) -> u32 {
        self.minimum_samples
    }

    /// Returns the configured adaptation policy.
    #[must_use]
    pub const fn adaptation(self) -> Adaptation {
        self.adaptation
    }

    /// Returns the configured learning model.
    #[must_use]
    pub const fn model(self) -> LearningModel {
        self.model
    }
}

/// Timing policy used to determine a task's health.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timing {
    /// Use a caller-provided expected interval.
    Fixed(FixedTiming),

    /// Learn expected timing from healthy heartbeat observations.
    Learned(LearnedTiming),
}

impl Timing {
    /// Creates a fixed timing policy with no additional grace period.
    #[must_use]
    pub const fn fixed(interval: Duration) -> Self {
        Self::Fixed(FixedTiming::new(interval))
    }

    /// Creates the default learned-frequency policy.
    #[must_use]
    pub fn learned() -> Self {
        Self::Learned(LearnedTiming::default())
    }
}

/// Registration configuration for a monitored task.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskConfig {
    pub(crate) name: Arc<str>,
    pub(crate) timing: Timing,
    startup_grace: Option<Duration>,
}

impl TaskConfig {
    /// Creates a task configuration.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>, timing: Timing) -> Self {
        Self {
            name: name.into(),
            timing,
            startup_grace: None,
        }
    }

    /// Overrides how long a task may remain silent before its first beat.
    #[must_use]
    pub const fn with_startup_grace(mut self, startup_grace: Duration) -> Self {
        self.startup_grace = Some(startup_grace);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.name.trim().is_empty() {
            return Err(ConfigError::EmptyName);
        }

        if self
            .startup_grace
            .is_some_and(|duration| duration.is_zero())
        {
            return Err(ConfigError::ZeroStartupGrace);
        }

        match self.timing {
            Timing::Fixed(fixed) => {
                if fixed.interval.is_zero() {
                    return Err(ConfigError::ZeroInterval);
                }
            }
            Timing::Learned(learned) => {
                if learned.minimum_samples < 2 {
                    return Err(ConfigError::TooFewLearningSamples);
                }
                if learned.startup_grace.is_zero() {
                    return Err(ConfigError::ZeroStartupGrace);
                }
                if !learned.sensitivity.is_finite() || learned.sensitivity <= 0.0 {
                    return Err(ConfigError::InvalidSensitivity);
                }

                match learned.model {
                    LearningModel::Ewma => {}
                    LearningModel::RobustWindow { capacity } => {
                        let capacity = usize::from(capacity);
                        if !(5..=MAX_ROBUST_WINDOW).contains(&capacity) || capacity & 1 == 0 {
                            return Err(ConfigError::InvalidRobustWindowCapacity);
                        }
                    }
                    LearningModel::RepeatingPattern(pattern) => {
                        let maximum_period = usize::from(pattern.maximum_period);
                        let minimum_cycles = usize::from(pattern.minimum_cycles);
                        if !(2..=MAX_PATTERN_PERIOD).contains(&maximum_period) {
                            return Err(ConfigError::InvalidPatternMaximumPeriod);
                        }
                        if !(3..=MAX_PATTERN_CYCLES).contains(&minimum_cycles) {
                            return Err(ConfigError::InvalidPatternMinimumCycles);
                        }
                        if !(1..=50).contains(&pattern.tolerance_percent) {
                            return Err(ConfigError::InvalidPatternTolerance);
                        }
                        if !(1..=100).contains(&pattern.minimum_contrast_percent) {
                            return Err(ConfigError::InvalidPatternContrast);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn effective_startup_grace(&self) -> Duration {
        self.startup_grace.unwrap_or_else(|| match self.timing {
            Timing::Fixed(fixed) => fixed.deadline(),
            Timing::Learned(learned) => learned.startup_grace,
        })
    }
}
