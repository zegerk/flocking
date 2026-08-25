# Changelog

All notable changes from version 0.1.12 onward are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases before 0.1.12 predate the maintained changelog.

## [Unreleased]

## [0.2.2] - 2026-08-25

### Added

- Add a reusable performance test suite: `npm run test:perf` gates the native benchmarks against `perf-baseline.json`, and `npm run perf:bless` re-records it. Cases are scored as a ratio to an untouched control and each tolerance is measured from repeated runs rather than guessed, so the thresholds survive different machines. `tests/parity.rs` now asserts the fixed-seed simulation hashes under `cargo test`, and `perf-harness.mjs` times the wasm boundary, GL uploads and draw in the page — call `perfHarness.run()` from the console, or load the page with `?perf=1` to have it run automatically and publish the report into the document.

### Changed

- Performance: `analyse()` skips the traversal entirely when the friend graph has not changed since the last call, which removes a full O(n) pointer-chasing pass that the legend was running several times a second; the on-cycle count is accumulated during that traversal instead of rescanning every dot; the colour-mode branch is hoisted out of the per-dot colour loop; trail history is no longer recorded while trails are switched off; line geometry is skipped when neither links nor the floor are drawn; and the wasm memory object and steps-per-frame slider are cached instead of re-read every frame. Simulation semantics are bit-identical (`cargo test` parity hashes).
- Performance: the simulation step and the camera's centroid-fit pass are specialized on the dimensions the UI offers, so the inner per-dimension loops unroll instead of staying rolled around a runtime `dim` — 11–20% off the step at 2D–8D. Above 131,072 dots the fit samples a stride rather than every dot, which cuts its cost by 78% at the maximum population; the sampled bounding box stays inside the true one and the camera smooths the result anyway.
- Performance: trail geometry carries the previous sample as an offset into the history ring instead of copying it through a fixed 24-float buffer, which removed a per-sample zeroing and a 96-byte copy that ran at every dimension. Trails are now drawn indexed, so each interior point is stored, uploaded and transformed once instead of twice. Together these cut a full-depth trail rebuild by 65%, and roughly halve both the per-frame upload and the trail vertex-shader work.
- Performance: the projection shader is compiled in two variants instead of branching on a `uTouring` uniform. Because that branch contains dynamic indexing into the local position array, a single shader forced the position into indexable scratch memory for every vertex even in 3D; the non-tour variant now keeps it in registers and skips uploading the 576 grand-tour floats each frame.

### Fixed

- Fix a crash when leaving a high dimension while the adaptive trail quality was changing. The trail vertex and colour scratch buffers were resized under a single size check on the vertex buffer, so dropping from 24D to 3D — which shrinks the per-vertex stride eightfold while raising the trail budget — left the colour buffer too short and panicked mid-frame.

### Notes

- Two further step rewrites were implemented, benchmarked natively, and reverted: caching the friend/enemy gaps from the distance pass into hoisted scratch won 4% on the unit law but lost 24% on the proportional one (the second gather hits L1 and beats the stack round-trip), and specializing the step on 24 dimensions was ~8% slower than leaving that loop rolled.

## [0.2.1] - 2026-08-25

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

[Unreleased]: https://github.com/zegerk/flocking/compare/v0.2.2...HEAD
[0.1.12]: https://github.com/zegerk/flocking/tree/v0.1.12
[0.2.0]: https://github.com/zegerk/flocking/compare/v0.1.12...v0.2.0
[0.2.1]: https://github.com/zegerk/flocking/compare/v0.2.0...v0.2.1
[0.2.2]: https://github.com/zegerk/flocking/compare/v0.2.1...v0.2.2
