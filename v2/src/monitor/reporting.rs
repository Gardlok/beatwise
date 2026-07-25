impl Monitor {
    /// Returns a compact policy-driven aggregate health summary.
    #[must_use]
    pub fn summary(&self, policy: HealthPolicy) -> HealthSummary {
        let now = self.inner.now_tick();
        self.summary_at(policy, now)
    }

    /// Returns an aggregate health summary and the task snapshots behind it.
    ///
    /// Task snapshots are ordered by task ID and are captured against the same
    /// monotonic observation tick used for the aggregate summary.
    #[must_use]
    pub fn report(&self, policy: HealthPolicy) -> HealthReport {
        let now = self.inner.now_tick();
        self.report_at(policy, now)
    }

    fn summary_at(&self, policy: HealthPolicy, now: u64) -> HealthSummary {
        let tasks = self.inner.read_tasks();
        let mut counts = HealthCounts::default();

        for entry in tasks.values() {
            counts.observe(status_for(entry, now).health.state());
        }

        HealthSummary::from_counts(policy, tick_duration(now, 1), counts)
    }

    fn report_at(&self, policy: HealthPolicy, now: u64) -> HealthReport {
        let tasks = self.inner.read_tasks();
        let mut statuses = tasks
            .values()
            .map(|entry| status_for(entry, now))
            .collect::<Vec<_>>();
        drop(tasks);

        statuses.sort_unstable_by_key(|status| status.id);
        HealthReport::from_statuses(policy, tick_duration(now, 1), statuses)
    }
}
