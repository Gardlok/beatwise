//! A lightweight heartbeat-based task liveness monitor.
//!
//! Thumper v2 keeps heartbeat collection inexpensive and runtime-neutral. It
//! supports fixed intervals, bounded learned-frequency models, bounded
//! repeating-pattern discovery, and pull-driven health transitions without
//! spawning threads or retaining an unbounded event history.

#![forbid(unsafe_code)]

mod config;
mod learning;
mod monitor;
mod status;

pub use config::{
    Adaptation, ConfigError, FixedTiming, LearnedTiming, LearningModel, PatternConfig,
    RegisterError, RetrainError, TaskConfig, Timing,
};
pub use monitor::{HealthTransition, Heartbeat, Monitor, StoppedError, TransitionCursor};
pub use status::{Confidence, Health, HealthState, StopReason, TaskId, TaskStatus, TimingStatus};
