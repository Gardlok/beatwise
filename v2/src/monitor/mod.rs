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
    status::{Health, StopReason, TaskId, TaskStatus, TimingStatus},
};

include!("registry.rs");
include!("heartbeat.rs");
include!("classification.rs");

#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod tests;
