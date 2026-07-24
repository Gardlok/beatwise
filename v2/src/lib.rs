//! A lightweight heartbeat-based task liveness monitor.
//!
//! Thumper v2 keeps heartbeat collection inexpensive and runtime-neutral. It
//! supports both explicitly configured intervals and bounded learned-frequency
//! models without spawning threads or retaining an unbounded event history.

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{
        Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const RUNNING: u8 = 0;
const STOPPED: u8 = 1;

const STOP_EXPLICIT: u8 = 1;
const STOP_LAST_HANDLE_DROPPED: u8 = 2;

const NO_TICK: u64 = 0;

/// An opaque identifier for a monitored task.
///
/// IDs are allocated monotonically and are not reused for the lifetime of a
/// monitor, preventing stale heartbeat handles from targeting newer tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    /// Returns the numeric representation of this identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Controls how a learned model changes after its training phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Adaptation {
    /// Keep the learned baseline fixed after the minimum sample count is met.
    FrozenAfterTraining,

    /// Accept healthy observations with a conservative adaptation rate.
    #[default]
    Slow,

    /// Accept healthy observations with a faster adaptation rate.
    Continuous,
}

impl Adaptation {
    fn alpha(self) -> f64 {
        match self {
            Self::FrozenAfterTraining => 0.0,
            Self::Slow => 0.05,
            Self::Continuous => 0.20,
        }
    }
}

/// Fixed timing configuration for a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedTiming {
    interval: Duration,
    grace: Duration,
}

impl FixedTiming {
    /// Creates fixed timing with no additional grace period.
    #[must_use]
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            grace: Duration::ZERO,
        }
    }

    /// Adds time beyond the expected interval before the task becomes late.
    #[must_use]
    pub const fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// Returns the expected heartbeat interval.
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// Returns the configured grace period.
    #[must_use]
    pub const fn grace(self) -> Duration {
        self.grace
    }

    fn deadline(self) -> Duration {
        self.interval.saturating_add(self.grace)
    }
}

/// Learned-frequency configuration for a task.
///
/// The current model uses an exponentially weighted moving mean and absolute
/// deviation. It consumes constant memory and rejects observations that are
/// already outside the trusted healthy deadline after training.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LearnedTiming {
    minimum_samples: u32,
    startup_grace: Duration,
    minimum_grace: Duration,
    sensitivity: f64,
    adaptation: Adaptation,
}

impl Default for LearnedTiming {
    fn default() -> Self {
        Self {
            minimum_samples: 8,
            startup_grace: Duration::from_secs(30),
            minimum_grace: Duration::from_millis(10),
            sensitivity: 4.0,
            adaptation: Adaptation::Slow,
        }
    }
}

impl LearnedTiming {
    /// Sets the number of accepted interval samples required for training.
    #[must_use]
    pub const fn with_minimum_samples(mut self, minimum_samples: u32) -> Self {
        self.minimum_samples = minimum_samples;
        self
    }

    /// Sets the maximum initial silence allowed before any baseline exists.
    #[must_use]
    pub const fn with_startup_grace(mut self, startup_grace: Duration) -> Self {
        self.startup_grace = startup_grace;
        self
    }

    /// Sets the smallest grace period used around the learned interval.
    #[must_use]
    pub const fn with_minimum_grace(mut self, minimum_grace: Duration) -> Self {
        self.minimum_grace = minimum_grace;
        self
    }

    /// Sets how many learned deviations are accepted beyond the mean.
    #[must_use]
    pub const fn with_sensitivity(mut self, sensitivity: f64) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Sets how the trusted model changes after training.
    #[must_use]
    pub const fn with_adaptation(mut self, adaptation: Adaptation) -> Self {
        self.adaptation = adaptation;
        self
    }

    /// Returns the minimum number of interval samples required for training.
    #[must_use]
    pub const fn minimum_samples(self) -> u32 {
        self.minimum_samples
    }

    /// Returns the configured adaptation policy.
    #[must_use]
    pub const fn adaptation(self) -> Adaptation {
        self.adaptation
    }
}

/// Timing policy used to determine a task's health.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timing {
    /// Use a caller-provided expected interval.
    Fixed(FixedTiming),

    /// Learn the expected interval from healthy heartbeat observations.
    Learned(LearnedTiming),
}

impl Timing {
    /// Creates a fixed timing policy with no additional grace period.
    #[must_use]
    pub const fn fixed(interval: Duration) -> Self {
        Self::Fixed(FixedTiming::new(interval))
    }

    /// Creates the default learned-frequency policy.
    #[must_use]
    pub fn learned() -> Self {
        Self::Learned(LearnedTiming::default())
    }
}

/// Registration configuration for a monitored task.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskConfig {
    name: Arc<str>,
    timing: Timing,
    startup_grace: Option<Duration>,
}

impl TaskConfig {
    /// Creates a task configuration.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>, timing: Timing) -> Self {
        Self {
            name: name.into(),
            timing,
            startup_grace: None,
        }
    }

    /// Overrides how long a task may remain silent before its first beat.
    #[must_use]
    pub const fn with_startup_grace(mut self, startup_grace: Duration) -> Self {
        self.startup_grace = Some(startup_grace);
        self
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.name.trim().is_empty() {
            return Err(ConfigError::EmptyName);
        }

        if self
            .startup_grace
            .is_some_and(|duration| duration.is_zero())
        {
            return Err(ConfigError::ZeroStartupGrace);
        }

        match self.timing {
            Timing::Fixed(fixed) => {
                if fixed.interval.is_zero() {
                    return Err(ConfigError::ZeroInterval);
                }
            }
            Timing::Learned(learned) => {
                if learned.minimum_samples < 2 {
                    return Err(ConfigError::TooFewLearningSamples);
                }
                if learned.startup_grace.is_zero() {
                    return Err(ConfigError::ZeroStartupGrace);
                }
                if !learned.sensitivity.is_finite() || learned.sensitivity <= 0.0 {
                    return Err(ConfigError::InvalidSensitivity);
                }
            }
        }

        Ok(())
    }

    fn effective_startup_grace(&self) -> Duration {
        self.startup_grace.unwrap_or_else(|| match self.timing {
            Timing::Fixed(fixed) => fixed.deadline(),
            Timing::Learned(learned) => learned.startup_grace,
        })
    }
}

/// A task configuration error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    EmptyName,
    ZeroInterval,
    ZeroStartupGrace,
    TooFewLearningSamples,
    InvalidSensitivity,
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

/// Returned when a heartbeat is sent after a task has stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoppedError;

impl fmt::Display for StoppedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task has stopped")
    }
}

impl Error for StoppedError {}

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
        samples: u32,
        required: u32,
        estimated_interval: Option<Duration>,
        estimated_deviation: Option<Duration>,
    },
    Learned {
        samples: u32,
        interval: Duration,
        deviation: Duration,
        deadline: Duration,
        adaptation: Adaptation,
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

#[derive(Clone, Copy, Debug)]
struct LearningState {
    samples: u32,
    mean_micros: f64,
    deviation_micros: f64,
    trained: bool,
}

impl Default for LearningState {
    fn default() -> Self {
        Self {
            samples: 0,
            mean_micros: 0.0,
            deviation_micros: 0.0,
            trained: false,
        }
    }
}

impl LearningState {
    fn observe(&mut self, interval: Duration, config: LearnedTiming) {
        let observed = interval.as_micros() as f64;
        if observed <= 0.0 || !observed.is_finite() {
            return;
        }

        if self.samples == 0 {
            self.samples = 1;
            self.mean_micros = observed;
            self.deviation_micros = 0.0;
            self.trained = self.samples >= config.minimum_samples;
            return;
        }

        if self.trained {
            let deadline = self.deadline_micros(config);
            if observed > deadline {
                return;
            }

            let alpha = config.adaptation.alpha();
            if alpha == 0.0 {
                self.samples = self.samples.saturating_add(1);
                return;
            }

            self.update(observed, alpha);
            self.samples = self.samples.saturating_add(1);
            return;
        }

        let training_alpha = 2.0 / (f64::from(config.minimum_samples) + 1.0);
        let accepted = self.clamp_training_observation(observed, config);
        self.update(accepted, training_alpha);
        self.samples = self.samples.saturating_add(1);
        self.trained = self.samples >= config.minimum_samples;
    }

    fn update(&mut self, observed: f64, alpha: f64) {
        let delta = observed - self.mean_micros;
        self.mean_micros += alpha * delta;
        self.deviation_micros += alpha * (delta.abs() - self.deviation_micros);
        self.deviation_micros = self.deviation_micros.max(0.0);
    }

    fn clamp_training_observation(self, observed: f64, config: LearnedTiming) -> f64 {
        if self.samples < 3 || self.deviation_micros <= 0.0 {
            return observed;
        }

        let allowance = self.allowance_micros(config);
        let lower = (self.mean_micros - allowance).max(1.0);
        let upper = self.mean_micros + allowance;
        observed.clamp(lower, upper)
    }

    fn allowance_micros(self, config: LearnedTiming) -> f64 {
        let minimum = config.minimum_grace.as_micros() as f64;
        minimum.max(config.sensitivity * self.deviation_micros)
    }

    fn deadline_micros(self, config: LearnedTiming) -> f64 {
        self.mean_micros + self.allowance_micros(config)
    }

    fn estimated_interval(self) -> Option<Duration> {
        (self.samples > 0).then(|| duration_from_micros(self.mean_micros))
    }

    fn estimated_deviation(self) -> Option<Duration> {
        (self.samples > 0).then(|| duration_from_micros(self.deviation_micros))
    }

    fn deadline(self, config: LearnedTiming) -> Option<Duration> {
        (self.samples > 0).then(|| duration_from_micros(self.deadline_micros(config)))
    }
}

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
                state: Mutex::new(LearningState::default()),
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
        self.entry.heartbeat_count.fetch_add(1, Ordering::Relaxed);

        if previous != NO_TICK && tick > previous {
            if let TimingState::Learned { config, state } = &self.entry.timing {
                let mut state = lock_learning(state);
                state.observe(tick_duration(tick, previous), *config);
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
        self.entry.active_handles.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Arc::clone(&self.inner),
            entry: Arc::clone(&self.entry),
        }
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        let previous = self.entry.active_handles.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "heartbeat handle count underflow");

        if previous == 1 {
            self.mark_stopped_at(StopReason::LastHandleDropped, self.inner.now_tick());
        }
    }
}

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
            let status = learned_timing_status(*config, *state);

            if state.trained {
                let interval = state
                    .estimated_interval()
                    .expect("trained learned timing must have an interval");
                let deadline = state
                    .deadline(*config)
                    .expect("trained learned timing must have a deadline");
                return (classify_health(silent_for, deadline, interval), status);
            }

            let provisional_deadline = state.deadline(*config).unwrap_or(entry.startup_grace);
            if silent_for > provisional_deadline {
                let interval = state.estimated_interval().unwrap_or(provisional_deadline);
                return (
                    classify_health(silent_for, provisional_deadline, interval),
                    status,
                );
            }

            (
                Health::Learning {
                    samples: state.samples,
                    required: config.minimum_samples,
                    silent_for,
                    estimated_interval: state.estimated_interval(),
                },
                status,
            )
        }
    }
}

fn classify_health(silent_for: Duration, deadline: Duration, interval: Duration) -> Health {
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
            learned_timing_status(*config, *lock_learning(state))
        }
    }
}

fn learned_timing_status(config: LearnedTiming, state: LearningState) -> TimingStatus {
    if state.trained {
        TimingStatus::Learned {
            samples: state.samples,
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
        }
    } else {
        TimingStatus::Learning {
            samples: state.samples,
            required: config.minimum_samples,
            estimated_interval: state.estimated_interval(),
            estimated_deviation: state.estimated_deviation(),
        }
    }
}

fn lock_learning(state: &Mutex<LearningState>) -> MutexGuard<'_, LearningState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn tick_duration(later: u64, earlier: u64) -> Duration {
    Duration::from_micros(later.saturating_sub(earlier))
}

fn interval_count(elapsed: Duration, interval: Duration) -> u64 {
    if interval.is_zero() {
        return 0;
    }

    (elapsed.as_nanos() / interval.as_nanos()).min(u128::from(u64::MAX)) as u64
}

fn duration_from_micros(micros: f64) -> Duration {
    if !micros.is_finite() || micros <= 0.0 {
        return Duration::ZERO;
    }

    Duration::from_micros(micros.round().min(u64::MAX as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_timing_becomes_healthy_then_late() {
        let monitor = Monitor::new();
        let heartbeat = monitor
            .register(TaskConfig::new(
                "fixed-worker",
                Timing::Fixed(
                    FixedTiming::new(Duration::from_millis(10))
                        .with_grace(Duration::from_millis(2)),
                ),
            ))
            .expect("valid fixed registration");

        let start = heartbeat.entry.created_tick;
        heartbeat
            .beat_at(start + 1_000)
            .expect("first heartbeat accepted");

        let healthy = monitor
            .status_at(heartbeat.id(), start + 11_000)
            .expect("status exists");
        assert!(matches!(healthy.health, Health::Healthy { .. }));

        let late = monitor
            .status_at(heartbeat.id(), start + 14_000)
            .expect("status exists");
        assert!(matches!(late.health, Health::Late { .. }));
    }

    #[test]
    fn learned_timing_trains_without_retaining_history() {
        let monitor = Monitor::new();
        let learned = LearnedTiming::default()
            .with_minimum_samples(3)
            .with_startup_grace(Duration::from_secs(1))
            .with_minimum_grace(Duration::from_micros(200))
            .with_sensitivity(4.0)
            .with_adaptation(Adaptation::FrozenAfterTraining);
        let heartbeat = monitor
            .register(TaskConfig::new("learner", Timing::Learned(learned)))
            .expect("valid learned registration");

        let start = heartbeat.entry.created_tick;
        for offset in [1_000, 2_000, 3_000, 4_000] {
            heartbeat
                .beat_at(start + offset)
                .expect("training heartbeat accepted");
        }

        let healthy = monitor
            .status_at(heartbeat.id(), start + 4_500)
            .expect("status exists");
        assert!(matches!(healthy.health, Health::Healthy { .. }));
        assert!(matches!(healthy.timing, TimingStatus::Learned { .. }));

        let late = monitor
            .status_at(heartbeat.id(), start + 6_000)
            .expect("status exists");
        assert!(matches!(late.health, Health::Late { .. }));
    }

    #[test]
    fn final_handle_drop_preserves_stopped_record() {
        let monitor = Monitor::new();
        let heartbeat = monitor
            .register(TaskConfig::new(
                "drop-test",
                Timing::fixed(Duration::from_secs(1)),
            ))
            .expect("valid registration");
        let id = heartbeat.id();
        let clone = heartbeat.clone();

        drop(heartbeat);
        let still_running = monitor.status(id).expect("record retained");
        assert!(!matches!(still_running.health, Health::Stopped { .. }));

        drop(clone);
        let stopped = monitor.status(id).expect("stopped record retained");
        assert!(matches!(
            stopped.health,
            Health::Stopped {
                reason: StopReason::LastHandleDropped,
                ..
            }
        ));
    }

    #[test]
    fn stopped_records_are_removed_only_by_explicit_purge() {
        let monitor = Monitor::new();
        let heartbeat = monitor
            .register(TaskConfig::new(
                "purge-test",
                Timing::fixed(Duration::from_secs(1)),
            ))
            .expect("valid registration");
        let id = heartbeat.id();

        heartbeat.stop();
        assert!(monitor.status(id).is_some());
        assert_eq!(monitor.purge_stopped(), 1);
        assert!(monitor.status(id).is_none());
    }

    #[test]
    fn invalid_learning_configuration_is_rejected() {
        let monitor = Monitor::new();
        let learned = LearnedTiming::default().with_minimum_samples(1);
        let error = monitor
            .register(TaskConfig::new("invalid", Timing::Learned(learned)))
            .err()
            .expect("registration must fail");

        assert_eq!(
            error,
            RegisterError::InvalidConfig(ConfigError::TooFewLearningSamples)
        );
    }
}
