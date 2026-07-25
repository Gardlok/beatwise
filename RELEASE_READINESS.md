# Beatwise 0.2.0 release readiness

This document defines the gate between the completed release-candidate audit and
any publication of Beatwise 0.2.0.

## Package identity

- Product name: **Beatwise**
- Canonical repository: `Gardlok/beatwise`
- crates.io package name: `beatwise`
- Library crate and Rust import path: `beatwise`
- Version: `0.2.0`
- Rust edition: 2024
- Minimum supported Rust version: 1.85
- Runtime dependencies: none
- Publication target: crates.io only; no upload is authorized by this document

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
grep -Fq 'publish = ["crates-io"]' Cargo.toml &&
cargo tree --depth 0 &&
cargo fmt --all --check &&
cargo clippy --all-targets -- -D warnings &&
cargo test &&
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps &&
cargo +1.85.0 check --all-targets &&
cargo +1.85.0 test &&
cargo package --list &&
cargo package &&
cargo publish --dry-run --registry crates-io &&
cargo run --example fixed_monitor &&
cargo run --example learned_monitor &&
cargo run --example health_report

git status
```

Expected results:

- package repository and homepage metadata point to `Gardlok/beatwise`;
- publication is restricted to the `crates-io` registry;
- `cargo tree --depth 0` shows `beatwise v0.2.0` and no dependencies;
- 30 internal unit tests pass;
- 4 external integration tests pass;
- 5 README-backed doctests pass;
- Rust 1.85.0 builds all targets and passes the test suite;
- package listing contains 33 files after Cargo adds generated metadata;
- the extracted package archive compiles successfully;
- `cargo publish --dry-run --registry crates-io` completes without uploading;
- all three examples run successfully;
- the worktree remains clean and synchronized.

## Registry preflight

The crates.io data-access policy requires API clients to send a descriptive
`User-Agent`. A generic client-only user agent can be rejected with `403
Forbidden`, which is an access-policy response rather than evidence that a crate
name is occupied.

Perform this check immediately before any authorized publish operation:

```bash
response=/tmp/beatwise-crate.json
status="$(curl -sS \
  --user-agent 'beatwise-release-audit/0.2.0 (https://github.com/Gardlok/beatwise)' \
  --header 'Accept: application/json' \
  --output "$response" \
  --write-out '%{http_code}' \
  https://crates.io/api/v1/crates/beatwise)"

case "$status" in
  404)
    echo "PASS: beatwise is not currently published on crates.io"
    ;;
  200)
    echo "FAIL: beatwise is already present on crates.io" >&2
    cat "$response" >&2
    exit 1
    ;;
  *)
    echo "ERROR: crates.io preflight returned HTTP $status" >&2
    cat "$response" >&2
    exit 1
    ;;
esac
```

Only `404` passes this preflight. `200` means the name is already present. Any
other response is inconclusive and must be resolved before publication; it must
not be treated as either availability or ownership.

Do not attempt to publish over an existing package or assume ownership from a
similar repository name.

## Publication enablement

The candidate manifest must contain:

```toml
publish = ["crates-io"]
```

This restricts publication to crates.io and allows Cargo's package and publish
dry-run checks to exercise the real release path. It does **not** authorize an
upload.

Do not run `cargo publish` without `--dry-run`, create a Git tag, or create a
GitHub release until the enablement PR is merged, the exact merged commit passes
the full gate again, and publication receives separate explicit authorization.

## Authorized release sequence

1. Merge the publication-enablement PR only after local qualification and the
   crates.io dry run pass.
2. Switch to `main`, pull the exact merged commit, and confirm a clean worktree.
3. Recheck that the exact `beatwise` registry name still returns `404`.
4. Repeat the full local qualification, package inspection, and publish dry run.
5. Record the final candidate commit and package archive checksum.
6. Obtain explicit authorization for the irreversible crates.io upload.
7. Authenticate Cargo as needed and run `cargo publish --registry crates-io` from
   the exact reviewed commit.
8. Verify the published crates.io metadata and docs.rs build.
9. Tag the published commit as `v0.2.0` and create the GitHub release from
   `CHANGELOG.md`.

Publishing is permanent for a given crate version. Never publish from a dirty
worktree, an unmerged branch, or a commit that differs from the reviewed package.
