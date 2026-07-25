//! A lightweight heartbeat-based task liveness monitor.
//!
//! Thumper v2 keeps heartbeat collection inexpensive and runtime-neutral. It
//! supports fixed intervals, bounded learned-frequency models, and bounded
//! repeating-pattern discovery without spawning threads or retaining an
//! unbounded event history.

#![forbid(unsafe_code)]

mod config;
mod learning;
mod monitor;
mod status;

pub use config::{
    Adaptation, ConfigError, FixedTiming, LearnedTiming, LearningModel, PatternConfig,
    RegisterError, RetrainError, TaskConfig, Timing,
};
pub use monitor::{Heartbeat, Monitor, StoppedError};
pub use status::{Confidence, Health, StopReason, TaskId, TaskStatus, TimingStatus};
