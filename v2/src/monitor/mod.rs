use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{
        Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    config::{FixedTiming, LearnedTiming, RegisterError, RetrainError, TaskConfig, Timing},
    learning::{LearningState, lock_learning},
    status::{Health, HealthState, StopReason, TaskId, TaskStatus, TimingStatus},
};

include!("registry.rs");
include!("heartbeat.rs");
include!("classification.rs");
include!("transitions.rs");

#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transition_tests;
