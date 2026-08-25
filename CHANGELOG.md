# Changelog

All notable changes from version 0.1.12 onward are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases before 0.1.12 predate the maintained changelog.

## [Unreleased]

### Changed

- Performance: spread is measured on the readout cadence instead of every frame; the GL upload reads sim.pos in place (no per-frame repack copy); analyse() reuses its in-degree/component-size scratch instead of allocating per call; JS caches typed-array views on the wasm buffer, graph versioning now changes only on re-pick so friend/enemy views are not rebuilt every step, line colours are parsed once instead of per frame, line uniforms upload once per frame, and vertex attributes rebind only on dimension change. Simulation semantics are bit-identical (fixed-seed parity harness, examples/parity.rs).
- Build with WebAssembly simd128 — measured 15–20% faster simulation step in wasm (A/B via examples/bench.rs + in-browser min-of-rounds timing).

### Notes

- A gap-caching / buffer-swap rewrite of Sim::step was implemented, benchmarked natively, and reverted: the original two-pass structure is ~20–80% faster because the copy-back clamp is a trivially vectorized streaming pass while the compute loop is gather-bound.

## [0.2.0] - 2026-08-25

### Added

- Add strict SemVer synchronization, guarded local release tooling, generated build metadata, and a visible runtime version.

### Fixed

- Make fullscreen-scale camera zoom consistent across every dimension and support two-finger pinch zoom on mobile canvases.

## [0.1.12] - 2026-08-25

### Added

- Add dynamic simulation storage and generic WebGL projection for 2D, 3D, 4D, 5D, 8D, and 24D spaces.
- Add D8 and D24 controls with full grand-tour projection and slicing.
- Make trail and geometry memory budgets dimension-aware.

[Unreleased]: https://github.com/zegerk/flocking/compare/v0.2.0...HEAD
[0.1.12]: https://github.com/zegerk/flocking/tree/v0.1.12
[0.2.0]: https://github.com/zegerk/flocking/compare/v0.1.12...v0.2.0
