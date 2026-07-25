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
                let skip_next_interval = entry.last_tick.load(Ordering::Acquire) != NO_TICK;
                lock_learning(state).reset_for_retraining(config.model, skip_next_interval);
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
