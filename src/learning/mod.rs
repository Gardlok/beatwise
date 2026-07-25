use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use crate::{
    config::{
        LearnedTiming, LearningModel, MAX_PATTERN_CYCLES, MAX_PATTERN_PERIOD, MAX_ROBUST_WINDOW,
        PatternConfig,
    },
    status::Confidence,
};

include!("frequency.rs");
include!("pattern.rs");
include!("state.rs");
include!("statistics.rs");
