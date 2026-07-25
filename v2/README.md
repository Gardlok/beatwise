# Thumper v2

This directory contains the modern Thumper core. It remains separate from the
original crate while the new API and timing models mature.

## Current scope

- Runtime-neutral, standard-library-only core.
- No internal background threads.
- No heartbeat event queue.
- Fixed-interval health monitoring.
- Learned timing with three bounded models:
  - constant-memory EWMA;
  - a fixed-capacity median/MAD robust window;
  - bounded repeating-pattern discovery with phase-aware deadlines.
- Confidence reporting for learned baselines.
- Explicit baseline retraining without replacing the heartbeat handle.
- Rejection accounting for observations outside a trusted trained range.
- Explicit `Starting`, `Learning`, `Healthy`, `Late`, and `Stopped` states.
- Stopped tasks remain observable until explicitly purged.
- Opaque, monotonically allocated task IDs that are never reused.
- Deterministic unit tests that do not sleep.

The fixed-timing heartbeat path performs only atomic operations and a monotonic
clock read. Learned timing adds one small per-task mutex. EWMA retains only its
current aggregate values. The optional robust model allocates one fixed-size
buffer only when selected and retains at most 31 interval samples.

Repeating-pattern discovery is also opt-in. Pattern tasks allocate one bounded
64-interval buffer while learning. The detector considers cycles of two through
eight phases, uses per-phase medians and median absolute deviations, rejects
constant-frequency false positives through a configurable contrast threshold,
and discards the learning buffer as an authority once a cycle is established.
No timing model retains unbounded history.

## Learned frequency

EWMA remains the default and has the smallest per-task state:

```rust
use std::time::Duration;
use thumper_v2::{
    Adaptation, LearnedTiming, Monitor, TaskConfig, Timing,
};

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

For workloads that occasionally produce extreme interval outliers, select the
bounded robust model:

```rust
use std::time::Duration;
use thumper_v2::{
    Adaptation, LearnedTiming, LearningModel, Monitor, TaskConfig, Timing,
};

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

Robust window capacities must be odd numbers from 5 through 31.

## Repeating patterns

A process may be healthy while deliberately alternating between short and long
work phases. The repeating-pattern model learns that cycle instead of reducing
it to one misleading average:

```rust
use std::time::Duration;
use thumper_v2::{
    Adaptation, LearnedTiming, LearningModel, Monitor, PatternConfig,
    TaskConfig, Timing,
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

`maximum_period` must be from 2 through 8, and `minimum_cycles` must be from 3
through 8. Candidate cycles are selected from a bounded 64-interval window. A
trained pattern reports its phase intervals, deviations, next expected phase,
and phase-specific deadline through `TimingStatus::PatternLearned`.

An outlier after training remains a valid liveness signal, advances the cycle
phase, and increments `rejected_samples`, but it does not overwrite the trusted
phase baseline. `Monitor::retrain` clears either a single-frequency or pattern
baseline while preserving the task ID, heartbeat count, lifecycle, and handle.
The first interval after reset is ignored so reset latency cannot contaminate the
new baseline.

## Validation

From the repository root:

```bash
cargo fmt --manifest-path v2/Cargo.toml --all --check
cargo clippy --manifest-path v2/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v2/Cargo.toml
cargo doc --manifest-path v2/Cargo.toml --no-deps
```
