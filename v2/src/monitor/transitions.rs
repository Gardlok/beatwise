/// One meaningful health-state change observed by a [`TransitionCursor`].
///
/// The first observation of a retained task has `previous == None`. Later
/// observations are emitted only when the stable [`HealthState`] changes;
/// changing elapsed durations and timing details do not create duplicate
/// transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthTransition {
    /// Monotonic sequence allocated by this cursor, starting at one.
    pub sequence: u64,

    /// Time since the monitor epoch when this transition was observed.
    ///
    /// This is the polling observation time, not a claim that the transition
    /// occurred at that exact instant between polls.
    pub observed_after: Duration,

    /// State previously observed by this cursor, or `None` for first sight.
    pub previous: Option<HealthState>,

    /// Newly observed stable health state.
    pub current: HealthState,

    /// Full task status captured by the same poll.
    pub status: TaskStatus,
}

/// Independent, bounded cursor for observing task health transitions.
///
/// A cursor stores one compact [`HealthState`] per retained task it has seen.
/// It does not retain transition history, does not drain a shared queue, and
/// does not spawn a worker. Multiple cursors may observe the same monitor
/// independently.
pub struct TransitionCursor {
    inner: Arc<Inner>,
    states: HashMap<TaskId, HealthState>,
    next_sequence: u64,
}

impl Monitor {
    /// Creates an independent transition cursor for this monitor.
    #[must_use]
    pub fn transition_cursor(&self) -> TransitionCursor {
        TransitionCursor {
            inner: Arc::clone(&self.inner),
            states: HashMap::new(),
            next_sequence: 1,
        }
    }
}

impl TransitionCursor {
    /// Observes all retained tasks and returns meaningful state changes.
    ///
    /// Results are ordered by task ID. The first poll reports the current state
    /// of every retained task with no previous state. Later polls report only
    /// `Starting`, `Learning`, `Healthy`, `Late`, or `Stopped` changes.
    #[must_use]
    pub fn poll(&mut self) -> Vec<HealthTransition> {
        self.poll_at(self.inner.now_tick())
    }

    /// Observes one retained task and returns its next meaningful state change.
    ///
    /// Unknown or purged task IDs return `None` and remove any state retained
    /// for that ID by this cursor.
    #[must_use]
    pub fn poll_task(&mut self, id: TaskId) -> Option<HealthTransition> {
        self.poll_task_at(id, self.inner.now_tick())
    }

    /// Returns the number of task-state tags retained by this cursor.
    #[must_use]
    pub fn tracked_len(&self) -> usize {
        self.states.len()
    }

    /// Returns whether this cursor currently retains no task-state tags.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Forgets all observations and restarts transition sequence numbering.
    ///
    /// The next poll reports every retained task as a first observation.
    pub fn reset(&mut self) {
        self.states.clear();
        self.next_sequence = 1;
    }

    fn poll_at(&mut self, now: u64) -> Vec<HealthTransition> {
        let tasks = self.inner.read_tasks();
        let mut statuses = tasks
            .values()
            .map(|entry| status_for(entry, now))
            .collect::<Vec<_>>();
        drop(tasks);

        statuses.sort_unstable_by_key(|status| status.id);
        self.states.retain(|id, _| {
            statuses
                .binary_search_by_key(id, |status| status.id)
                .is_ok()
        });

        let observed_after = tick_duration(now, 1);
        statuses
            .into_iter()
            .filter_map(|status| self.observe(status, observed_after))
            .collect()
    }

    fn poll_task_at(&mut self, id: TaskId, now: u64) -> Option<HealthTransition> {
        let status = {
            let tasks = self.inner.read_tasks();
            tasks.get(&id).map(|entry| status_for(entry, now))
        };

        let Some(status) = status else {
            self.states.remove(&id);
            return None;
        };

        self.observe(status, tick_duration(now, 1))
    }

    fn observe(
        &mut self,
        status: TaskStatus,
        observed_after: Duration,
    ) -> Option<HealthTransition> {
        let current = status.health.state();
        let previous = self.states.insert(status.id, current);
        if previous == Some(current) {
            return None;
        }

        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);

        Some(HealthTransition {
            sequence,
            observed_after,
            previous,
            current,
            status,
        })
    }
}
