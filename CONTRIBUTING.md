# Contributing to Beatwise

Thank you for helping improve Beatwise.

Beatwise is intentionally small: it observes task progress without owning a
runtime, scheduler, retry loop, background worker, or durable operation record.
Contributions should preserve that boundary unless a design discussion reaches a
clear alternative first.

## Before opening a change

For defects, include the smallest reproducible example you can. For API or model
changes, open an issue or discussion before investing in a large implementation.
Documentation, tests, examples, and narrowly scoped fixes are welcome directly.

Keep pull requests focused. Avoid mixing formatting, renaming, unrelated cleanup,
and behavior changes in one review.

## Development requirements

- Stable Rust for normal development.
- Rust 1.85.0 for minimum-supported-version validation.
- No additional runtime dependencies without explicit design justification.

Install the MSRV toolchain when needed:

```bash
rustup toolchain install 1.85.0
```

## Project principles

Changes should preserve these properties:

- runtime-neutral operation;
- no background thread, executor, callback worker, or event queue;
- bounded memory for learned timing and transition observation;
- no `unsafe` code;
- monotonic in-process timing;
- observer semantics rather than task-control authority;
- stopped records remain visible until explicitly purged;
- public behavior is documented and covered by tests.

A heartbeat should represent meaningful progress. Beatwise should report health;
it should not silently restart, cancel, retry, schedule, or replay the work it
observes.

## Validation

Run the stable-toolchain checks from the repository root:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Then verify the declared MSRV:

```bash
cargo +1.85.0 check --all-targets
cargo +1.85.0 test
```

For changes that affect packaging, examples, metadata, or documentation included
in the crate, also run:

```bash
cargo package --list
cargo package

cargo run --example fixed_monitor
cargo run --example learned_monitor
cargo run --example health_report
cargo run --example worker_loop
```

Do not run a non-dry-run `cargo publish` as part of contribution validation.
Publishing, tagging, and GitHub release creation are maintainer-controlled
operations.

## Tests and documentation

- Add unit tests beside internal behavior when practical.
- Add or update `tests/public_api.rs` when public callers need coverage.
- Keep README examples compilable; the README is also the crate-level rustdoc.
- Document errors, lifecycle effects, ownership, and concurrency behavior for new
  public APIs.
- Prefer runnable examples for integration patterns that would make inline
  rustdoc too large.

## Pull requests

A good pull request explains:

- what changed;
- why the change belongs in Beatwise;
- any public or behavioral impact;
- the validation commands that passed;
- remaining tradeoffs or follow-up work.

Breaking changes before 1.0 require a minor-version release and should not be
introduced accidentally through an otherwise unrelated change.
