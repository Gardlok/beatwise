use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
    time::{Duration, Instant},
};

use crate::{
    config::{
        FixedTiming, LearnedTiming, RegisterError, RetrainError, TaskConfig, Timing,
    },
    learning::{lock_learning, LearningState},
    status::{Health, StopReason, TaskId, TaskStatus, TimingStatus},
};

const RUNNING: u8 = 0;
const STOPPED: u8 = 1;

const STOP_EXPLICIT: u8 = 1;
const STOP_LAST_HANDLE_DROPPED: u8 = 2;

const NO_TICK: u64 = 0;

enum TimingState {
    Fixed(FixedTiming),
    Learned {
        config: LearnedTiming,
        state: Mutex<LearningState>,
    },
}

struct TaskEntry {
    id: TaskId,
    name: Arc<str>,
    created_tick: u64,
    startup_grace: Duration,
    timing: TimingState,

    last_tick: AtomicU64,
    stopped_tick: AtomicU64,
    heartbeat_count: AtomicU64,
    active_handles: AtomicUsize,
    lifecycle: AtomicU8,
    stop_reason: AtomicU8,
}

struct Inner {
    epoch: Instant,
    next_id: AtomicU64,
    tasks: RwLock<HashMap<TaskId, Arc<TaskEntry>>>,
}

impl Inner {
    fn now_tick(&self) -> u64 {
        let elapsed_micros = self.epoch.elapsed().as_micros();
        let bounded = elapsed_micros.min(u128::from(u64::MAX - 1)) as u64;
        bounded + 1
    }

    fn read_tasks(&self) -> RwLockReadGuard<'_, HashMap<TaskId, Arc<TaskEntry>>> {
        self.tasks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write_tasks(&self) -> RwLockWriteGuard<'_, HashMap<TaskId, Arc<TaskEntry>>> {
        self.tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Registry and observation point for monitored tasks.
///
/// `Monitor` is cheap to clone and does not own a background thread.
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    /// Creates an empty monitor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                epoch: Instant::now(),
                next_id: AtomicU64::new(1),
                tasks: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Registers a task and returns its heartbeat handle.
    pub fn register(&self, config: TaskConfig) -> Result<Heartbeat, RegisterError> {
        config.validate()?;

        let raw_id = self
            .inner
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| RegisterError::IdSpaceExhausted)?;

        let id = TaskId(raw_id);
        let created_tick = self.inner.now_tick();
        let startup_grace = config.effective_startup_grace();
        let timing = match config.timing {
            Timing::Fixed(fixed) => TimingState::Fixed(fixed),
            Timing::Learned(learned) => TimingState::Learned {
                config: learned,
                state: Mutex::new(LearningState::new(learned.model)),
            },
        };

        let entry = Arc::new(TaskEntry {
            id,
            name: config.name,
            created_tick,
            startup_grace,
            timing,
            last_tick: AtomicU64::new(NO_TICK),
            stopped_tick: AtomicU64::new(NO_TICK),
            heartbeat_count: AtomicU64::new(0),
            active_handles: AtomicUsize::new(1),
            lifecycle: AtomicU8::new(RUNNING),
            stop_reason: AtomicU8::new(0),
        });

        self.inner.write_tasks().insert(id, Arc::clone(&entry));

        Ok(Heartbeat {
            inner: Arc::clone(&self.inner),
            entry,
        })
    }

    /// Returns a compact status snapshot for one task.
    #[must_use]
    pub fn status(&self, id: TaskId) -> Option<TaskStatus> {
        self.status_at(id, self.inner.now_tick())
    }

    /// Returns compact status snapshots for all retained tasks.
    #[must_use]
    pub fn snapshot(&self) -> Vec<TaskStatus> {
        let now = self.inner.now_tick();
        self.inner
            .read_tasks()
            .values()
            .map(|entry| status_for(entry, now))
            .collect()
    }

    /// Returns the number of retained task records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.read_tasks().len()
    }

    /// Returns whether no task records are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Discards a learned baseline and returns the task to its learning phase.
    ///
    /// The next interval is intentionally ignored so the time between the
    /// reset request and the next heartbeat cannot contaminate the new model.
    pub fn retrain(&self, id: TaskId) -> Result<(), RetrainError> {
        let entry = self
            .inner
            .read_tasks()
            .get(&id)
            .cloned()
            .ok_or(RetrainError::UnknownTask)?;

        if entry.lifecycle.load(Ordering::Acquire) != RUNNING {
            return Err(RetrainError::Stopped);
        }

        match &entry.timing {
            TimingState::Fixed(_) => Err(RetrainError::FixedTiming),
            TimingState::Learned { config, state } => {
                let skip_next_interval =
                    entry.last_tick.load(Ordering::Acquire) != NO_TICK;
                lock_learning(state)
                    .reset_for_retraining(config.model, skip_next_interval);
                Ok(())
            }
        }
    }

    /// Removes all stopped records and returns the number removed.
    pub fn purge_stopped(&self) -> usize {
        let mut tasks = self.inner.write_tasks();
        let previous_len = tasks.len();
        tasks.retain(|_, entry| entry.lifecycle.load(Ordering::Acquire) != STOPPED);
        previous_len - tasks.len()
    }

    fn status_at(&self, id: TaskId, now: u64) -> Option<TaskStatus> {
        let tasks = self.inner.read_tasks();
        tasks.get(&id).map(|entry| status_for(entry, now))
    }
}

/// Handle used by a monitored task to report progress.
///
/// Clones refer to the same task. The record becomes stopped only after an
/// explicit stop or after the final handle is dropped.
pub struct Heartbeat {
    inner: Arc<Inner>,
    entry: Arc<TaskEntry>,
}

impl Heartbeat {
    /// Returns this heartbeat's task identifier.
    #[must_use]
    pub fn id(&self) -> TaskId {
        self.entry.id
    }

    /// Records a heartbeat at the current monotonic time.
    pub fn beat(&self) -> Result<(), StoppedError> {
        self.beat_at(self.inner.now_tick())
    }

    /// Explicitly marks the task as stopped.
    pub fn stop(self) {
        self.mark_stopped_at(StopReason::Explicit, self.inner.now_tick());
    }

    fn beat_at(&self, tick: u64) -> Result<(), StoppedError> {
        if self.entry.lifecycle.load(Ordering::Acquire) != RUNNING {
            return Err(StoppedError);
        }

        let previous = self.entry.last_tick.swap(tick, Ordering::AcqRel);
        self.entry
            .heartbeat_count
            .fetch_add(1, Ordering::Relaxed);

        if previous != NO_TICK && tick > previous {
            if let TimingState::Learned { config, state } = &self.entry.timing {
                lock_learning(state)
                    .observe(tick_duration(tick, previous), *config);
            }
        }

        Ok(())
    }

    fn mark_stopped_at(&self, reason: StopReason, tick: u64) {
        if self
            .entry
            .stopped_tick
            .compare_exchange(NO_TICK, tick, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.entry.stop_reason.store(
                match reason {
                    StopReason::Explicit => STOP_EXPLICIT,
                    StopReason::LastHandleDropped => STOP_LAST_HANDLE_DROPPED,
                },
                Ordering::Relaxed,
            );
            self.entry.lifecycle.store(STOPPED, Ordering::Release);
        }
    }
}

impl Clone for Heartbeat {
    fn clone(&self) -> Self {
        self.entry
            .active_handles
            .fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Arc::clone(&self.inner),
            entry: Arc::clone(&self.entry),
        }
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        let previous = self
            .entry
            .active_handles
            .fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "heartbeat handle count underflow");

        if previous == 1 {
            self.mark_stopped_at(
                StopReason::LastHandleDropped,
                self.inner.now_tick(),
            );
        }
    }
}

/// Returned when a heartbeat is sent after a task has stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoppedError;

impl fmt::Display for StoppedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task has stopped")
    }
}

impl Error for StoppedError {}

fn status_for(entry: &TaskEntry, now: u64) -> TaskStatus {
    let heartbeat_count = entry.heartbeat_count.load(Ordering::Acquire);
    let (health, timing) = if entry.lifecycle.load(Ordering::Acquire) == STOPPED {
        let stopped_tick = entry.stopped_tick.load(Ordering::Acquire);
        let reason = match entry.stop_reason.load(Ordering::Acquire) {
            STOP_EXPLICIT => StopReason::Explicit,
            _ => StopReason::LastHandleDropped,
        };

        (
            Health::Stopped {
                stopped_for: tick_duration(now, stopped_tick),
                reason,
            },
            timing_status(&entry.timing),
        )
    } else {
        running_status(entry, now)
    };

    TaskStatus {
        id: entry.id,
        name: Arc::clone(&entry.name),
        health,
        timing,
        heartbeat_count,
    }
}

fn running_status(entry: &TaskEntry, now: u64) -> (Health, TimingStatus) {
    let last_tick = entry.last_tick.load(Ordering::Acquire);

    if last_tick == NO_TICK {
        let elapsed = tick_duration(now, entry.created_tick);
        let health = if elapsed <= entry.startup_grace {
            Health::Starting {
                elapsed,
                startup_grace: entry.startup_grace,
            }
        } else {
            Health::Late {
                silent_for: elapsed,
                deadline: entry.startup_grace,
                overdue_by: elapsed.saturating_sub(entry.startup_grace),
                missed_intervals: 0,
            }
        };

        return (health, timing_status(&entry.timing));
    }

    let silent_for = tick_duration(now, last_tick);

    match &entry.timing {
        TimingState::Fixed(fixed) => {
            let deadline = fixed.deadline();
            let health = classify_health(silent_for, deadline, fixed.interval);
            (
                health,
                TimingStatus::Fixed {
                    interval: fixed.interval,
                    grace: fixed.grace,
                },
            )
        }
        TimingState::Learned { config, state } => {
            let state = lock_learning(state);
            let status = learned_timing_status(*config, &state);

            if state.is_trained() {
                let interval = state
                    .estimated_interval()
                    .expect("trained learned timing must have an interval");
                let deadline = state
                    .deadline(*config)
                    .expect("trained learned timing must have a deadline");
                return (
                    classify_health(silent_for, deadline, interval),
                    status,
                );
            }

            let provisional_deadline =
                state.deadline(*config).unwrap_or(entry.startup_grace);
            if silent_for > provisional_deadline {
                let interval = state
                    .estimated_interval()
                    .unwrap_or(provisional_deadline);
                return (
                    classify_health(
                        silent_for,
                        provisional_deadline,
                        interval,
                    ),
                    status,
                );
            }

            (
                Health::Learning {
                    samples: state.samples(),
                    required: config.minimum_samples,
                    silent_for,
                    estimated_interval: state.estimated_interval(),
                },
                status,
            )
        }
    }
}

fn classify_health(
    silent_for: Duration,
    deadline: Duration,
    interval: Duration,
) -> Health {
    if silent_for <= deadline {
        Health::Healthy {
            silent_for,
            deadline,
        }
    } else {
        Health::Late {
            silent_for,
            deadline,
            overdue_by: silent_for.saturating_sub(deadline),
            missed_intervals: interval_count(silent_for, interval),
        }
    }
}

fn timing_status(timing: &TimingState) -> TimingStatus {
    match timing {
        TimingState::Fixed(fixed) => TimingStatus::Fixed {
            interval: fixed.interval,
            grace: fixed.grace,
        },
        TimingState::Learned { config, state } => {
            let state = lock_learning(state);
            learned_timing_status(*config, &state)
        }
    }
}

fn learned_timing_status(
    config: LearnedTiming,
    state: &LearningState,
) -> TimingStatus {
    let confidence = state.confidence(config);

    if state.is_trained() {
        TimingStatus::Learned {
            model: state.model(),
            samples: state.samples(),
            rejected_samples: state.rejected_samples(),
            interval: state
                .estimated_interval()
                .expect("trained learned timing must have an interval"),
            deviation: state
                .estimated_deviation()
                .expect("trained learned timing must have a deviation"),
            deadline: state
                .deadline(config)
                .expect("trained learned timing must have a deadline"),
            adaptation: config.adaptation,
            confidence,
        }
    } else {
        TimingStatus::Learning {
            model: state.model(),
            samples: state.samples(),
            rejected_samples: state.rejected_samples(),
            required: config.minimum_samples,
            estimated_interval: state.estimated_interval(),
            estimated_deviation: state.estimated_deviation(),
            confidence,
        }
    }
}

fn tick_duration(later: u64, earlier: u64) -> Duration {
    Duration::from_micros(later.saturating_sub(earlier))
}

fn interval_count(elapsed: Duration, interval: Duration) -> u64 {
    if interval.is_zero() {
        return 0;
    }

    (elapsed.as_nanos() / interval.as_nanos())
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests;
