# Beatwise 0.2.0 identity qualification

This document records the acceptance boundary after promoting the modern
heartbeat-monitoring core to the repository root and adopting the Beatwise
package identity.

## Qualified boundary

- Product, package, library crate, and Rust import path: `beatwise` 0.2.0
- Current repository location: `Gardlok/thumper`
- Rust edition: 2024
- Minimum supported Rust version: 1.85
- Runtime dependencies: none
- Publication: disabled during release qualification
- Authoritative package location: repository root
- Legacy Thumper 0.1.x implementation: removed from the active tree
- Former `v2/` staging package: removed after promotion

The 0.2.0 version communicates that the public API is intentionally incompatible
with the legacy DJ/deck/output model. The old implementation remains recoverable
through Git history, but it is not duplicated or packaged.

## Required validation

Run from the repository root:

```bash
test ! -d v2 &&
! grep -R \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude='*.md' \
  -nE 'thumper_v2|thumper-monitor|use thumper::' . &&
cargo tree --depth 0 &&
cargo fmt --all --check &&
cargo clippy --all-targets -- -D warnings &&
cargo test &&
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps &&
cargo +1.85.0 check --all-targets &&
cargo +1.85.0 test &&
cargo package --list &&
cargo package --allow-dirty &&
cargo run --example fixed_monitor &&
cargo run --example learned_monitor &&
cargo run --example health_report

git status
```

The test suite is expected to include:

- 30 internal unit tests;
- 4 external integration tests under `tests/public_api.rs`;
- 5 README-backed doctests.

The package listing is expected to contain 33 files after Cargo adds generated
package metadata. The package verification step must build the extracted crate
archive rather than only the repository working tree.

## Identity invariants

- `cargo metadata` resolves one root package named `beatwise`.
- The library target and Rust import path are `beatwise`.
- No `v2/` package remains.
- No legacy runtime dependencies remain in `Cargo.toml`.
- No legacy DJ, deck, output, tuning, benchmark, or example modules remain in
  the active repository tree.
- Public examples, README doctests, and integration tests import `beatwise`.
- The README is the crate-level rustdoc source.
- Publication remains disabled until a later explicit release task.

Passing this gate completes the package-identity qualification. It does not
publish, tag, rename the GitHub repository, create a release, or enable automated
publication.
