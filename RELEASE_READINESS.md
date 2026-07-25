# Beatwise 0.2.0 release record

Beatwise 0.2.0 was qualified, published, tagged, and released on 2026-07-25.
This document preserves the exact release identity and the checks used for that
publication. It is a historical record, not authorization to publish a future
version.

## Published identity

- Product name: **Beatwise**
- Canonical repository: `Gardlok/beatwise`
- crates.io package name: `beatwise`
- Library crate and Rust import path: `beatwise`
- Version: `0.2.0`
- Rust edition: 2024
- Minimum supported Rust version: 1.85
- Runtime dependencies: none
- Source commit: `ff80153a11b66d38c300efabde522cfdb0f5da5a`
- Git tag: `v0.2.0`
- Annotated tag object: `712a7210717e020293b69c32962b4d3a66a0c705`
- Published archive SHA-256: `a9c6792308d79ef698d9f1c87729492190e9b514393abaa8514e2a61131e92c0`
- Registry publication time: `2026-07-25T19:44:56Z`
- GitHub release publication time: `2026-07-25T20:36:26Z`

The crates.io index recorded the same archive checksum, no dependencies,
`rust_version = "1.85"`, and `yanked = false`. docs.rs completed the 0.2.0 build.
The peeled `v0.2.0` tag resolves to the exact source commit above.

## Qualification evidence

The exact merged source commit passed:

- `cargo tree --depth 0` with no dependencies;
- `cargo fmt --all --check`;
- `cargo clippy --all-targets -- -D warnings`;
- 30 internal unit tests;
- 4 external public-API tests;
- 5 README-backed doctests;
- rustdoc with warnings denied;
- Rust 1.85.0 checks for all targets and the same 39 tests;
- package listing and extracted-package compilation;
- `cargo publish --dry-run --registry crates-io`;
- all three release examples;
- clean and synchronized worktree verification.

The release archive contained 33 files and measured 120.2 KiB uncompressed and
25.0 KiB compressed. After a clean rebuild of `target/package`, both `gzip -t`
and `tar -tzf` returned status zero and the archive produced the published
checksum above.

## Publication evidence

The final registry preflight confirmed the exact `beatwise` name was unoccupied.
The authorized command:

```bash
cargo publish --registry crates-io
```

completed successfully and crates.io reported Beatwise 0.2.0 as published. The
registry checksum was then compared with the qualified local archive and matched
exactly.

The annotated `v0.2.0` tag was pushed and verified with its peeled commit:

```text
712a7210717e020293b69c32962b4d3a66a0c705  refs/tags/v0.2.0
ff80153a11b66d38c300efabde522cfdb0f5da5a  refs/tags/v0.2.0^{}
```

The GitHub release `Beatwise 0.2.0` was created as the latest release, not a draft
and not a prerelease.

## Checklist for a future release

Every future version requires a new, version-specific qualification. Do not reuse
this release's commit, archive checksum, registry preflight, or test counts.

1. Update the version and changelog deliberately.
2. Confirm package, repository, documentation, license, edition, MSRV, and
   publication metadata.
3. Run formatting, Clippy, stable tests, rustdoc with warnings denied, exact-MSRV
   checks, and all examples.
4. Inspect `cargo package --list`, build the package, verify the extracted crate,
   and test the gzip/tar archive integrity.
5. Run `cargo publish --dry-run --registry crates-io`.
6. Record the exact clean merged commit and archive SHA-256.
7. Obtain separate explicit authorization for the irreversible upload.
8. Publish from that exact commit and immediately verify crates.io metadata,
   registry checksum, yanked state, and docs.rs.
9. Tag the exact published commit and create the GitHub release from the
   changelog.

Publishing is permanent for a given crate version. Never publish from a dirty
worktree, an unmerged branch, or a commit that differs from the reviewed package.
