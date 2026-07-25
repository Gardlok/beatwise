# Beatwise

[crates.io](https://crates.io/crates/beatwise) · [API documentation](https://docs.rs/beatwise) · [repository](https://github.com/Gardlok/beatwise)

Beatwise is a small Rust library for watching long-running work.

A task calls `beat()` whenever it makes progress. Beatwise tracks that rhythm and
reports whether the task is `Starting`, `Learning`, `Healthy`, `Late`, or `Stopped`.

It works well for background workers, sync loops, schedulers, queue consumers,
device pollers, backups, peer maintenance, and other jobs that should keep moving.

Beatwise can watch a known interval, learn a task's normal pace, or learn a
repeating cycle. It uses only the standard library, starts no threads, and does
not require an async runtime.

## Installation

```bash
cargo add beatwise
```

Beatwise supports Rust 1.85 and newer.

## Quick start

Register a task, keep the returned heartbeat handle with the work, and call
`beat()` whenever meaningful progress completes:

```rust
use std::time::Duration;
use beatwise::{Monitor, TaskConfig, Timing};

let monitor = Monitor::new();
let heartbeat = monitor.register(TaskConfig::new(
    "worker",
    Timing::fixed(Duration::from_secs(5)),
))?;

heartbeat.beat()?;

let status = monitor
    .status(heartbeat.id())
    .expect("registered task remains retained");
println!("{}: {:?}", status.name, status.health);

# Ok::<(), Box<dyn std::error::Error>>(())
```

A heartbeat should mean that work advanced, not merely that a process or thread
still exists. Beatwise reports health when the caller asks; it does not wake,
restart, cancel, retry, or schedule the monitored work.

## What it provides

- Fixed deadlines for predictable work.
- Learned timing for jobs whose pace changes.
- A robust model for occasional outliers.
- Repeating-pattern detection for multi-phase work.
- Independent transition cursors and aggregate health reports.
- Bounded memory with no event queue or unbounded history.

All timing stays inside the caller's process. Fixed heartbeats use atomics and a
monotonic clock read. Learned tasks add one small mutex. Stopped tasks remain
visible until explicitly purged.

## Choosing a timing model

| Model | Best for | Main tradeoff |
| --- | --- | --- |
| Fixed | Pollers, schedulers, and jobs with a known expected interval | The caller must choose the interval and grace period |
| EWMA | Workloads whose normal pace changes gradually | Accepted observations continuously influence the baseline |
| Robust window | Bursty work with occasional extreme pauses or outliers | Keeps a small bounded sample window and requires an odd capacity |
| Repeating pattern | Stable multi-phase work such as short/long processing cycles | Requires several complete cycles before the pattern is trusted |

Use fixed timing when the expected interval is part of the task's contract. Use
learned timing when the workload itself is the best source of that expectation.

## Lifecycle and ownership

A fixed task begins in `Starting` and becomes `Healthy` after its first beat. A
learned task begins in `Learning` and remains there until enough accepted
intervals establish a baseline. Running tasks then move between `Healthy` and
`Late` as their silence crosses the current deadline.

```text
fixed:    Starting ── first beat ──> Healthy <──> Late
learned:  Learning ── trained ─────> Healthy <──> Late

running task ── explicit stop or final heartbeat drop ──> Stopped
Stopped ── Monitor::purge_stopped() ────────────────────> removed
```

`Heartbeat` clones refer to the same task. An explicit `stop()` is task-wide, so
other clones can no longer beat. Dropping the final clone also marks the task as
stopped. A stopped record remains observable until `Monitor::purge_stopped()`
removes it, and a stopped handle cannot revive a purged task.

`Monitor` is cheap to clone and may be shared between observers. Heartbeat clones
are safe to move between threads. For learned timing, remember that every call to
`beat()` is treated as the next interval in one logical progress stream; unrelated
concurrent producers should usually be registered as separate tasks.

## Learn a task's normal pace

EWMA is the default learned model. It uses constant memory and adapts gradually as
the task changes:

```rust
use std::time::Duration;
use beatwise::{Adaptation, LearnedTiming, Monitor, TaskConfig, Timing};

let monitor = Monitor::new();
let timing = LearnedTiming::default()
    .with_minimum_samples(8)
    .with_minimum_grace(Duration::from_millis(25))
    .with_sensitivity(4.0)
    .with_adaptation(Adaptation::Slow);

let heartbeat = monitor.register(TaskConfig::new(
    "database-sync",
    Timing::Learned(timing),
))?;

heartbeat.beat()?;

for status in monitor.snapshot() {
    println!("{}: {:?}", status.name, status.timing);
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

For jobs with occasional long pauses or extreme outliers, use the bounded robust
window:

```rust
use std::time::Duration;
use beatwise::{Adaptation, LearnedTiming, LearningModel, Monitor, TaskConfig, Timing};

let monitor = Monitor::new();
let timing = LearnedTiming::default()
    .with_minimum_samples(9)
    .with_minimum_grace(Duration::from_millis(25))
    .with_adaptation(Adaptation::Slow)
    .with_model(LearningModel::robust_window(9));
let heartbeat = monitor.register(TaskConfig::new(
    "bursty-worker",
    Timing::Learned(timing),
))?;

let task_id = heartbeat.id();
monitor.retrain(task_id)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

Robust windows use odd capacities from 5 through 31. `Monitor::retrain` clears the
learned baseline without replacing the task ID or heartbeat handle.

## Learn repeating work

Some healthy jobs alternate between short and long phases. The repeating-pattern
model learns that cycle instead of forcing it into one average:

```rust
use std::time::Duration;
use beatwise::{
    Adaptation, LearnedTiming, LearningModel, Monitor, PatternConfig, TaskConfig, Timing,
};

let monitor = Monitor::new();
let pattern = PatternConfig::new(6, 3)
    .with_tolerance_percent(10)
    .with_minimum_contrast_percent(20);
let timing = LearnedTiming::default()
    .with_minimum_samples(9)
    .with_startup_grace(Duration::from_secs(30))
    .with_minimum_grace(Duration::from_millis(25))
    .with_sensitivity(4.0)
    .with_adaptation(Adaptation::Slow)
    .with_model(LearningModel::RepeatingPattern(pattern));
let heartbeat = monitor.register(TaskConfig::new(
    "phased-worker",
    Timing::Learned(timing),
))?;

heartbeat.beat()?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

Patterns can have two through eight phases and require three through eight cycles.
Beatwise learns from at most 64 intervals. An outlier still counts as a heartbeat
and advances the phase, but it does not replace the trusted baseline.

## Watch state changes

A transition cursor reports meaningful state changes without callbacks or a shared
event queue:

```rust
use std::time::Duration;
use beatwise::{HealthState, Monitor, TaskConfig, Timing};

let monitor = Monitor::new();
let heartbeat = monitor.register(TaskConfig::new(
    "worker",
    Timing::fixed(Duration::from_secs(1)),
))?;
let mut transitions = monitor.transition_cursor();

for transition in transitions.poll() {
    assert_eq!(transition.previous, None);
    assert_eq!(transition.current, HealthState::Starting);
}

heartbeat.beat()?;

for transition in transitions.poll() {
    println!(
        "{}: {:?} -> {:?}",
        transition.status.name,
        transition.previous,
        transition.current,
    );
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

Each cursor is independent. `poll()` returns only state changes, ordered by task
ID. A transition's observation time is when the cursor polled the monitor, not an
exact historical timestamp for the instant the state changed. Beatwise does not
run callbacks or own an executor.

## Build health endpoints

Use a summary for a lightweight health check or a full report when you need task
details:

```rust
use std::time::Duration;
use beatwise::{HealthPolicy, HealthVerdict, Monitor, TaskConfig, Timing};

let monitor = Monitor::new();
let heartbeat = monitor.register(TaskConfig::new(
    "api-worker",
    Timing::fixed(Duration::from_secs(1)),
))?;

let readiness = monitor.summary(HealthPolicy::readiness());
assert_eq!(readiness.verdict, HealthVerdict::Degraded);
assert_eq!(readiness.counts.starting, 1);

heartbeat.beat()?;

let liveness = monitor.report(HealthPolicy::liveness());
println!(
    "verdict={:?}, considered={}, ignored={}",
    liveness.summary.verdict,
    liveness.summary.considered_tasks,
    liveness.summary.ignored_tasks,
);
for task in liveness.tasks.iter() {
    println!("{}: {:?}", task.name, task.health);
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

`readiness()` treats starting and learning tasks as degraded. `liveness()` allows
those states, fails late tasks, and ignores stopped records. `strict()` accepts
only healthy tasks. Custom policies can change how any state affects the verdict.

## Examples

The repository includes runnable examples for fixed timing, learned timing,
aggregate health reports, and a standard-library worker thread:

```bash
cargo run --example fixed_monitor
cargo run --example learned_monitor
cargo run --example health_report
cargo run --example worker_loop
```

## Version note

Beatwise 0.2.0 replaces the old Thumper 0.1.x DJ, deck, and output API. The old
implementation remains available in Git history.

## Validation

From the repository root:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo package --list
cargo package
```
