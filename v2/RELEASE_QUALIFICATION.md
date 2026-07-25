# Thumper v2 release qualification

This document defines the gate that must pass before the modern crate replaces
the legacy root `thumper` package.

## Boundary under test

- Package: `thumper-v2` 0.1.0
- Library crate: `thumper_v2`
- Rust edition: 2024
- Minimum supported Rust version: 1.85
- Runtime dependencies: none
- Publication: disabled during qualification
- Legacy root crate: unchanged

The package manifest explicitly limits distributable contents to source files,
integration tests, examples, the README, and the MIT license.

## Required validation

Run from the repository root:

```bash
cargo fmt --manifest-path v2/Cargo.toml --all --check &&
cargo clippy --manifest-path v2/Cargo.toml --all-targets -- -D warnings &&
cargo test --manifest-path v2/Cargo.toml &&
RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path v2/Cargo.toml --no-deps &&
cargo package --manifest-path v2/Cargo.toml --list &&
cargo package --manifest-path v2/Cargo.toml --allow-dirty

git status
```

The test suite is expected to include:

- 30 internal unit tests;
- 4 external integration tests under `tests/public_api.rs`;
- 5 README-backed doctests.

The package verification step must build the generated crate archive rather
than only the repository working tree.

## Runnable examples

```bash
cargo run --manifest-path v2/Cargo.toml --example fixed_monitor
cargo run --manifest-path v2/Cargo.toml --example learned_monitor
cargo run --manifest-path v2/Cargo.toml --example health_report
```

The examples intentionally require no async runtime, background worker, sleep,
or external service.

## Phase 7 entry criteria

The root migration may begin only after all required validation passes from a
clean, synchronized branch. Phase 7 will then replace the legacy package
boundary with the qualified modern crate, update the root README and package
metadata, and preserve the legacy implementation only if there is an explicit
reason to retain it.
