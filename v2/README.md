# Thumper v2 foundation

This directory contains the first implementation slice for a modern Thumper core.
It is intentionally separate from the original crate so the design can mature
without destabilizing the existing implementation.

## Current scope

- Runtime-neutral, standard-library-only core.
- No internal background threads.
- No heartbeat event queue.
- Fixed-interval health monitoring.
- Learned-frequency monitoring using constant-memory EWMA state.
- Explicit `Starting`, `Learning`, `Healthy`, `Late`, and `Stopped` states.
- Stopped tasks remain observable until explicitly purged.
- Opaque, monotonically allocated task IDs that are never reused.
- Deterministic unit tests that do not sleep.

The fixed-timing heartbeat path performs only atomic operations and a monotonic
clock read. Learned timing adds one small per-task mutex around its bounded
statistical model; it does not allocate or retain an unbounded sample history.

Repeating multi-phase pattern discovery is deliberately not included in this
foundation. The public timing model leaves room to add it after learned-frequency
behavior and lifecycle semantics are proven.

## Example

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
    println!("{}: {:?}", status.name, status.health);
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

## Validation

From the repository root:

```bash
cargo fmt --manifest-path v2/Cargo.toml --all --check
cargo clippy --manifest-path v2/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v2/Cargo.toml
cargo doc --manifest-path v2/Cargo.toml --no-deps
```
