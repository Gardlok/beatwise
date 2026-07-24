# Thumper v2

This directory contains the modern Thumper core. It remains separate from the
original crate while the new API and timing models mature.

## Current scope

- Runtime-neutral, standard-library-only core.
- No internal background threads.
- No heartbeat event queue.
- Fixed-interval health monitoring.
- Learned-frequency monitoring with two bounded models:
  - constant-memory EWMA;
  - a fixed-capacity median/MAD robust window.
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
buffer only when selected and retains at most 31 interval samples; it performs
no unbounded allocation.

Repeating multi-phase pattern discovery is deliberately not included yet. This
phase hardens single-frequency learning before introducing cycle detection.

## Learned timing

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

// Discard the trusted baseline when an intentional operating-mode change makes
// the old frequency irrelevant. The handle remains valid.
monitor.retrain(task_id)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

Robust window capacities must be odd numbers from 5 through 31. Retraining
preserves the task ID, heartbeat count, and lifecycle. The first interval after
a reset is ignored so reset latency cannot contaminate the new baseline.

## Validation

From the repository root:

```bash
cargo fmt --manifest-path v2/Cargo.toml --all --check
cargo clippy --manifest-path v2/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v2/Cargo.toml
cargo doc --manifest-path v2/Cargo.toml --no-deps
```
