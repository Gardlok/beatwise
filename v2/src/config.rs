use std::{error::Error, fmt, sync::Arc, time::Duration};

pub(crate) const MAX_ROBUST_WINDOW: usize = 31;

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

/// Statistical model used to learn a task's heartbeat frequency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LearningModel {
    /// Constant-memory exponentially weighted moving statistics.
    #[default]
    Ewma,

    /// A bounded median and median-absolute-deviation window.
    ///
    /// The capacity must be an odd number from 5 through 31.
    RobustWindow { capacity: u8 },
}

impl LearningModel {
    /// Creates a bounded robust window model.
    #[must_use]
    pub const fn robust_window(capacity: u8) -> Self {
        Self::RobustWindow { capacity }
    }
}

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

/// Learned-frequency configuration for a task.
///
/// The default model uses an exponentially weighted moving mean and absolute
/// deviation. Callers may instead select a bounded robust window that uses the
/// median and scaled median absolute deviation. Both models reject observations
/// outside the trusted healthy range after training.
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

    /// Learn the expected interval from healthy heartbeat observations.
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
                if let LearningModel::RobustWindow { capacity } = learned.model {
                    let capacity = usize::from(capacity);
                    if !(5..=MAX_ROBUST_WINDOW).contains(&capacity) || capacity & 1 == 0 {
                        return Err(ConfigError::InvalidRobustWindowCapacity);
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

/// A task configuration error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    EmptyName,
    ZeroInterval,
    ZeroStartupGrace,
    TooFewLearningSamples,
    InvalidSensitivity,
    InvalidRobustWindowCapacity,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("task name cannot be empty"),
            Self::ZeroInterval => formatter.write_str("fixed interval cannot be zero"),
            Self::ZeroStartupGrace => formatter.write_str("startup grace cannot be zero"),
            Self::TooFewLearningSamples => {
                formatter.write_str("learned timing requires at least two interval samples")
            }
            Self::InvalidSensitivity => {
                formatter.write_str("learned timing sensitivity must be finite and positive")
            }
            Self::InvalidRobustWindowCapacity => formatter.write_str(
                "robust learning window capacity must be an odd number from 5 through 31",
            ),
        }
    }
}

impl Error for ConfigError {}

/// A task registration error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    InvalidConfig(ConfigError),
    IdSpaceExhausted,
}

impl fmt::Display for RegisterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(error) => write!(formatter, "invalid task configuration: {error}"),
            Self::IdSpaceExhausted => formatter.write_str("task ID space exhausted"),
        }
    }
}

impl Error for RegisterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidConfig(error) => Some(error),
            Self::IdSpaceExhausted => None,
        }
    }
}

impl From<ConfigError> for RegisterError {
    fn from(error: ConfigError) -> Self {
        Self::InvalidConfig(error)
    }
}

/// Returned when a learned task cannot be retrained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetrainError {
    /// No retained task exists for the supplied identifier.
    UnknownTask,

    /// Fixed timing has no learned baseline to reset.
    FixedTiming,

    /// Stopped tasks cannot re-enter learning.
    Stopped,
}

impl fmt::Display for RetrainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTask => formatter.write_str("task is not registered"),
            Self::FixedTiming => formatter.write_str("fixed timing cannot be retrained"),
            Self::Stopped => formatter.write_str("stopped task cannot be retrained"),
        }
    }
}

impl Error for RetrainError {}
