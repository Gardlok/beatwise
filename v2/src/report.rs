use std::{sync::Arc, time::Duration};

use crate::status::{HealthState, TaskStatus};

const HEALTH_STATES: [HealthState; 5] = [
    HealthState::Starting,
    HealthState::Learning,
    HealthState::Healthy,
    HealthState::Late,
    HealthState::Stopped,
];

/// Effect one task state has on an aggregate health verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HealthImpact {
    /// Exclude matching tasks from the aggregate verdict.
    Ignore,

    /// Treat matching tasks as operating normally.
    Nominal,

    /// Treat matching tasks as usable but not fully ready.
    Degraded,

    /// Treat matching tasks as unhealthy.
    Unhealthy,
}

/// Aggregate health verdict for the tasks considered by a [`HealthPolicy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HealthVerdict {
    /// No retained task was considered by the policy.
    Empty,

    /// Every considered task had nominal impact.
    Nominal,

    /// At least one considered task was degraded and none were unhealthy.
    Degraded,

    /// At least one considered task was unhealthy.
    Unhealthy,
}

impl HealthVerdict {
    /// Returns whether this verdict is fully nominal.
    #[must_use]
    pub const fn is_nominal(self) -> bool {
        matches!(self, Self::Nominal)
    }
}

/// Explicit mapping from task states to aggregate health impacts.
///
/// The built-in policies are conveniences rather than hidden monitor behavior.
/// Callers may override any state with [`HealthPolicy::with_impact`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HealthPolicy {
    starting: HealthImpact,
    learning: HealthImpact,
    healthy: HealthImpact,
    late: HealthImpact,
    stopped: HealthImpact,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self::readiness()
    }
}

impl HealthPolicy {
    /// A readiness-oriented policy.
    ///
    /// Starting and learning tasks are degraded, healthy tasks are nominal,
    /// and late or stopped retained tasks are unhealthy.
    #[must_use]
    pub const fn readiness() -> Self {
        Self {
            starting: HealthImpact::Degraded,
            learning: HealthImpact::Degraded,
            healthy: HealthImpact::Nominal,
            late: HealthImpact::Unhealthy,
            stopped: HealthImpact::Unhealthy,
        }
    }

    /// A liveness-oriented policy.
    ///
    /// Starting, learning, and healthy tasks are nominal. Late tasks are
    /// unhealthy. Stopped retained tasks are ignored because they are no longer
    /// expected to produce heartbeats.
    #[must_use]
    pub const fn liveness() -> Self {
        Self {
            starting: HealthImpact::Nominal,
            learning: HealthImpact::Nominal,
            healthy: HealthImpact::Nominal,
            late: HealthImpact::Unhealthy,
            stopped: HealthImpact::Ignore,
        }
    }

    /// A strict policy that accepts only healthy tasks as nominal.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            starting: HealthImpact::Unhealthy,
            learning: HealthImpact::Unhealthy,
            healthy: HealthImpact::Nominal,
            late: HealthImpact::Unhealthy,
            stopped: HealthImpact::Unhealthy,
        }
    }

    /// Overrides the impact assigned to one stable task state.
    #[must_use]
    pub const fn with_impact(mut self, state: HealthState, impact: HealthImpact) -> Self {
        match state {
            HealthState::Starting => self.starting = impact,
            HealthState::Learning => self.learning = impact,
            HealthState::Healthy => self.healthy = impact,
            HealthState::Late => self.late = impact,
            HealthState::Stopped => self.stopped = impact,
        }
        self
    }

    /// Returns the impact assigned to a stable task state.
    #[must_use]
    pub const fn impact(self, state: HealthState) -> HealthImpact {
        match state {
            HealthState::Starting => self.starting,
            HealthState::Learning => self.learning,
            HealthState::Healthy => self.healthy,
            HealthState::Late => self.late,
            HealthState::Stopped => self.stopped,
        }
    }

    fn evaluate(self, counts: HealthCounts) -> (HealthVerdict, usize, usize) {
        let mut verdict = HealthVerdict::Empty;
        let mut considered_tasks = 0;
        let mut ignored_tasks = 0;

        for state in HEALTH_STATES {
            let count = counts.count(state);
            if count == 0 {
                continue;
            }

            match self.impact(state) {
                HealthImpact::Ignore => ignored_tasks += count,
                HealthImpact::Nominal => {
                    considered_tasks += count;
                    if verdict == HealthVerdict::Empty {
                        verdict = HealthVerdict::Nominal;
                    }
                }
                HealthImpact::Degraded => {
                    considered_tasks += count;
                    if verdict != HealthVerdict::Unhealthy {
                        verdict = HealthVerdict::Degraded;
                    }
                }
                HealthImpact::Unhealthy => {
                    considered_tasks += count;
                    verdict = HealthVerdict::Unhealthy;
                }
            }
        }

        (verdict, considered_tasks, ignored_tasks)
    }
}

/// Exact retained-task counts grouped by stable health state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HealthCounts {
    pub starting: usize,
    pub learning: usize,
    pub healthy: usize,
    pub late: usize,
    pub stopped: usize,
}

impl HealthCounts {
    /// Returns the total number of retained task records represented.
    #[must_use]
    pub const fn total(self) -> usize {
        self.starting + self.learning + self.healthy + self.late + self.stopped
    }

    /// Returns the count for one stable health state.
    #[must_use]
    pub const fn count(self, state: HealthState) -> usize {
        match state {
            HealthState::Starting => self.starting,
            HealthState::Learning => self.learning,
            HealthState::Healthy => self.healthy,
            HealthState::Late => self.late,
            HealthState::Stopped => self.stopped,
        }
    }

    pub(crate) fn observe(&mut self, state: HealthState) {
        match state {
            HealthState::Starting => self.starting += 1,
            HealthState::Learning => self.learning += 1,
            HealthState::Healthy => self.healthy += 1,
            HealthState::Late => self.late += 1,
            HealthState::Stopped => self.stopped += 1,
        }
    }
}

/// Compact aggregate observation of a monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthSummary {
    /// Time since the monitor epoch when this summary was observed.
    pub observed_after: Duration,

    /// Policy used to calculate the verdict.
    pub policy: HealthPolicy,

    /// Aggregate result under the supplied policy.
    pub verdict: HealthVerdict,

    /// Exact counts of every retained task state, including ignored states.
    pub counts: HealthCounts,

    /// Number of retained tasks included in the verdict.
    pub considered_tasks: usize,

    /// Number of retained tasks excluded from the verdict by the policy.
    pub ignored_tasks: usize,
}

impl HealthSummary {
    pub(crate) fn from_counts(
        policy: HealthPolicy,
        observed_after: Duration,
        counts: HealthCounts,
    ) -> Self {
        let (verdict, considered_tasks, ignored_tasks) = policy.evaluate(counts);
        Self {
            observed_after,
            policy,
            verdict,
            counts,
            considered_tasks,
            ignored_tasks,
        }
    }

    /// Returns whether the aggregate verdict is fully nominal.
    #[must_use]
    pub const fn is_nominal(self) -> bool {
        self.verdict.is_nominal()
    }
}

/// Aggregate summary plus the task snapshots used to build it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReport {
    /// Compact aggregate result for this observation.
    pub summary: HealthSummary,

    /// Task snapshots captured at the same observation time, ordered by task ID.
    pub tasks: Arc<[TaskStatus]>,
}

impl HealthReport {
    pub(crate) fn from_statuses(
        policy: HealthPolicy,
        observed_after: Duration,
        statuses: Vec<TaskStatus>,
    ) -> Self {
        let mut counts = HealthCounts::default();
        for status in &statuses {
            counts.observe(status.health.state());
        }

        Self {
            summary: HealthSummary::from_counts(policy, observed_after, counts),
            tasks: Arc::from(statuses),
        }
    }

    /// Returns whether the aggregate verdict is fully nominal.
    #[must_use]
    pub const fn is_nominal(&self) -> bool {
        self.summary.is_nominal()
    }
}
