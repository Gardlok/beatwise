/// A task configuration error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    EmptyName,
    ZeroInterval,
    ZeroStartupGrace,
    TooFewLearningSamples,
    InvalidSensitivity,
    InvalidRobustWindowCapacity,
    InvalidPatternMaximumPeriod,
    InvalidPatternMinimumCycles,
    InvalidPatternTolerance,
    InvalidPatternContrast,
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
            Self::InvalidPatternMaximumPeriod => {
                formatter.write_str("pattern maximum period must be from 2 through 8")
            }
            Self::InvalidPatternMinimumCycles => {
                formatter.write_str("pattern minimum cycles must be from 3 through 8")
            }
            Self::InvalidPatternTolerance => {
                formatter.write_str("pattern tolerance must be from 1 through 50 percent")
            }
            Self::InvalidPatternContrast => formatter
                .write_str("pattern minimum contrast must be from 1 through 100 percent"),
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
