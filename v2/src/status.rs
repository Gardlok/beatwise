use std::{
    sync::Arc,
    time::Duration,
};

use crate::config::{Adaptation, LearningModel};

/// An opaque identifier for a monitored task.
///
/// IDs are allocated monotonically and are not reused for the lifetime of a
/// monitor, preventing stale heartbeat handles from targeting newer tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub(crate) u64);

impl TaskId {
    /// Returns the numeric representation of this identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Confidence in a learned timing baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// There are not yet enough accepted samples to trust the model.
    Insufficient,

    /// A baseline exists, but it has limited evidence or substantial variation.
    Low,

    /// The baseline has a useful amount of stable evidence.
    Medium,

    /// The baseline has substantial stable evidence.
    High,
}

/// Describes why a task entered the stopped state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Explicit,
    LastHandleDropped,
}

/// Current health of a monitored task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Health {
    Starting {
        elapsed: Duration,
        startup_grace: Duration,
    },
    Learning {
        samples: u32,
        required: u32,
        silent_for: Duration,
        estimated_interval: Option<Duration>,
    },
    Healthy {
        silent_for: Duration,
        deadline: Duration,
    },
    Late {
        silent_for: Duration,
        deadline: Duration,
        overdue_by: Duration,
        missed_intervals: u64,
    },
    Stopped {
        stopped_for: Duration,
        reason: StopReason,
    },
}

/// Current public view of a task's timing policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimingStatus {
    Fixed {
        interval: Duration,
        grace: Duration,
    },
    Learning {
        model: LearningModel,
        samples: u32,
        rejected_samples: u32,
        required: u32,
        estimated_interval: Option<Duration>,
        estimated_deviation: Option<Duration>,
        confidence: Confidence,
    },
    Learned {
        model: LearningModel,
        samples: u32,
        rejected_samples: u32,
        interval: Duration,
        deviation: Duration,
        deadline: Duration,
        adaptation: Adaptation,
        confidence: Confidence,
    },
}

/// Compact snapshot of one monitored task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskStatus {
    pub id: TaskId,
    pub name: Arc<str>,
    pub health: Health,
    pub timing: TimingStatus,
    pub heartbeat_count: u64,
}
