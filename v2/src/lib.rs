//! A lightweight heartbeat-based task liveness monitor.
//!
//! Thumper v2 keeps heartbeat collection inexpensive and runtime-neutral. It
//! supports fixed intervals, bounded learned-frequency models, bounded
//! repeating-pattern discovery, pull-driven health transitions, and
//! policy-driven aggregate reports without spawning threads or retaining an
//! unbounded event history.

#![forbid(unsafe_code)]

mod config;
mod learning;
mod monitor;
mod report;
mod status;

pub use config::{
    Adaptation, ConfigError, FixedTiming, LearnedTiming, LearningModel, PatternConfig,
    RegisterError, RetrainError, TaskConfig, Timing,
};
pub use monitor::{HealthTransition, Heartbeat, Monitor, StoppedError, TransitionCursor};
pub use report::{
    HealthCounts, HealthImpact, HealthPolicy, HealthReport, HealthSummary, HealthVerdict,
};
pub use status::{Confidence, Health, HealthState, StopReason, TaskId, TaskStatus, TimingStatus};
