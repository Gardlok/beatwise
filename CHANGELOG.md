# Changelog

All notable changes to Beatwise are documented in this file.

The project follows Semantic Versioning. Versions before 1.0 may make breaking
API changes in a minor release.

## [Unreleased]

No unreleased changes.

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

Version 0.2.0 has not been published by this change. The manifest retains
`publish = false` until a separate, explicitly authorized release task.
