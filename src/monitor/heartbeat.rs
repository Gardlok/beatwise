/// Handle used by a monitored task to report meaningful progress.
///
/// Clones refer to the same task and may be moved between threads. An explicit
/// [`Heartbeat::stop`] is task-wide, and the record also becomes stopped after
/// the final handle is dropped. Stopped records remain observable until
/// [`Monitor::purge_stopped`] removes them.
///
/// For learned timing, every call to [`Heartbeat::beat`] is treated as the next
/// interval in one logical progress stream. Unrelated concurrent producers
/// should usually be registered as separate tasks.
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

    /// Records meaningful progress at the current monotonic time.
    ///
    /// Fixed timing uses the beat as the new liveness reference point. Learned
    /// timing also observes the interval since the previous beat after one
    /// exists. Concurrent calls are safe, but learned timing treats every call
    /// as part of the same ordered interval stream.
    ///
    /// # Errors
    ///
    /// Returns [`StoppedError`] after an explicit stop or after the task has
    /// otherwise entered the stopped lifecycle state.
    pub fn beat(&self) -> Result<(), StoppedError> {
        self.beat_at(self.inner.now_tick())
    }

    /// Explicitly marks the task as stopped.
    ///
    /// This consumes the calling handle, but the stop applies to the shared task
    /// record: any remaining clones can no longer beat and will return
    /// [`StoppedError`]. The stopped record remains available through the
    /// monitor until [`Monitor::purge_stopped`] removes it.
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
                lock_learning(state).observe(tick_duration(tick, previous), *config);
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

/// Returned when a heartbeat is sent after a task has stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoppedError;

impl fmt::Display for StoppedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task has stopped")
    }
}

impl Error for StoppedError {}
