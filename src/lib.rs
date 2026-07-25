#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

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
