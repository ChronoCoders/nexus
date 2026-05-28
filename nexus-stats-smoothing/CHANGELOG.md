# Changelog

All notable changes to nexus-stats-smoothing are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/),
with the project-specific allowance that a minor bump may carry small,
narrowly-scoped breaking changes when external blast radius is
contained.

## [Unreleased]

### Removed

- `HoltF32`, `HoltF32Builder` — use `HoltF64` (f32 is a precision footgun for accumulating smoothers)
- `SpringF32` — use `SpringF64`
- `Kalman1dF32`, `Kalman1dF32Builder` — use `Kalman1dF64`
- `KamaF32`, `KamaF32Builder` — use `KamaF64`
- `WindowedMedianF32` — use `WindowedMedianF64`
- `WindowedMedianI32` — use `WindowedMedianI64`

### Changed

- `WindowedMedianI64::modified_z_score` now returns `Option<f64>` (was `Option<i64>`) to include the 0.6745 scale factor
- `Kalman1dF64Builder::build` now rejects NaN/infinite process and measurement noise

## [1.2.3] — 2026-05-26

## [1.2.1] and earlier

Earlier history is not documented in this CHANGELOG. See git history
and GitHub release notes for details.
