# Changelog

All notable changes to Beatwise are documented in this file.

The project follows Semantic Versioning. Versions before 1.0 may make breaking
API changes in a minor release.

## [Unreleased]

### Documentation

- Added installation and fixed-timing quick-start guidance.
- Added timing-model selection, lifecycle, ownership, and responsibility-boundary documentation.
- Expanded monitor and heartbeat API rustdoc.
- Added a runnable standard-library worker-loop example.
- Added contribution and security policies.
- Converted the 0.2.0 release-readiness document into a completed release record.

## [0.2.0] - 2026-07-25

### Added

- Runtime-neutral heartbeat monitoring with no background worker or event queue.
- Fixed timing with configurable grace periods.
- Learned timing using EWMA, bounded robust-window, and bounded repeating-pattern models.
- Confidence and rejected-sample reporting for learned timing.
- Explicit retraining while preserving task identity and heartbeat handles.
- Stable `Starting`, `Learning`, `Healthy`, `Late`, and `Stopped` health states.
- Independent pull-driven transition cursors.
- Policy-driven readiness, liveness, and strict aggregate health reports.
- External public-API tests, README-backed doctests, and runnable examples.
- Rust 2024 support with Rust 1.85 as the declared MSRV.

### Changed

- Replaced the legacy Thumper 0.1.x DJ/deck/output API with the modern monitoring core.
- Promoted the qualified implementation from the temporary `v2/` package to the repository root.
- Renamed the modern project, package, library crate, documentation identity, and Rust import path to `beatwise`.

### Removed

- Legacy DJ, deck, beat, track, record, output, tuning, benchmark, and example code.
- All runtime dependencies.

### Release status

Version 0.2.0 was published to crates.io on 2026-07-25 from commit
`ff80153a11b66d38c300efabde522cfdb0f5da5a` and tagged as `v0.2.0`.
The published registry checksum is
`a9c6792308d79ef698d9f1c87729492190e9b514393abaa8514e2a61131e92c0`.
