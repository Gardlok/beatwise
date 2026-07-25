# Beatwise 0.2.0 release readiness

This document defines the gate between the completed root migration and any
future publication of Beatwise 0.2.0.

## Package identity

- Product name: **Beatwise**
- Canonical repository: `Gardlok/beatwise`
- crates.io package name: `beatwise`
- Library crate and Rust import path: `beatwise`
- Version: `0.2.0`
- Rust edition: 2024
- Minimum supported Rust version: 1.85
- Runtime dependencies: none
- Publication: disabled until a separate explicitly authorized release task

The former Thumper identity is retained only in repository history. The active
repository, package metadata, documentation, examples, and Rust imports use one
consistent Beatwise identity.

Registry availability is time-sensitive. Immediately before publication, repeat
the exact-name check rather than relying on this document or an older search.

## Local qualification

Run from a clean checkout of the candidate commit:

```bash
grep -q 'repository = "https://github.com/Gardlok/beatwise"' Cargo.toml &&
grep -q 'homepage = "https://github.com/Gardlok/beatwise"' Cargo.toml &&
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

Expected results:

- package repository and homepage metadata point to `Gardlok/beatwise`;
- `cargo tree --depth 0` shows `beatwise v0.2.0` and no dependencies;
- 30 internal unit tests pass;
- 4 external integration tests pass;
- 5 README-backed doctests pass;
- Rust 1.85.0 builds all targets and passes the test suite;
- package listing contains 33 files after Cargo adds generated metadata;
- the extracted package archive compiles successfully;
- all three examples run successfully;
- the worktree remains clean and synchronized.

## Registry preflight

Perform this check immediately before any authorized publish operation:

```bash
status="$(curl -sS \
  -o /tmp/beatwise-crate.json \
  -w '%{http_code}' \
  https://crates.io/api/v1/crates/beatwise)"

test "$status" = 404
```

A response other than `404` means the package identity must be reviewed again.
Do not attempt to publish over an existing package or assume ownership from a
similar repository name.

## Publication safety latch

The candidate manifest must continue to contain:

```toml
publish = false
```

Enabling publication, authenticating Cargo, uploading the crate, creating a Git
tag, or creating a GitHub release are all outside this phase. Those actions
require separate explicit authorization after the full gate above passes.

## Future authorized release sequence

Once publication is explicitly authorized:

1. Reconfirm the exact candidate commit and clean worktree.
2. Recheck the `beatwise` registry name.
3. Review the generated package contents and extracted build one final time.
4. Change the publication setting in a focused release PR.
5. Merge only after local qualification passes again.
6. Publish the exact merged commit.
7. Verify crates.io and docs.rs metadata.
8. Tag the published commit and create release notes from `CHANGELOG.md`.

Publishing is permanent for a given crate version. Never publish from a dirty
worktree, an unmerged branch, or a commit that differs from the reviewed package.
