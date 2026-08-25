// In-page performance harness for the parts that only exist in the browser:
// the wasm boundary, the GL uploads, and the draw path. Native benches cannot
// see any of it.
//
// From the console, or the browser tools:
//   await perfHarness.run()            // all phases, JSON result
//   await perfHarness.run({rounds: 5})
//
// Same scoring as the native gate — every phase is bracketed by a control
// measured immediately before and after, and the best ratio wins. Browser
// wall-clock swings several-fold between sessions; a ratio to an untouched
// control does not. `controlDrift` in the result reports how far the control
// itself moved during the run: if that exceeds ~10%, distrust the numbers and
// re-run with fewer tabs open.

import { median, summarize } from './perf-stats.mjs';

export const DEFAULT_ROUNDS = 9;
export const TARGET_SEGMENT_MS = 15;

/** Time `iters` calls, optionally flushing once inside the timed region. */
export function timeIters(fn, iters, now, flush) {
  const t0 = now();
  for (let i = 0; i < iters; i++) fn();
  if (flush) flush();
  return (now() - t0) / iters;
}

/** Pick an iteration count that makes one segment last ~targetMs. */
export function calibrate(fn, warmup, now, flush, targetMs = TARGET_SEGMENT_MS) {
  for (let i = 0; i < warmup; i++) fn();
  const one = timeIters(fn, Math.max(1, warmup), now, flush);
  if (!(one > 0)) return 10_000;
  return Math.min(1_000_000, Math.max(1, Math.ceil(targetMs / one)));
}

/**
 * Bracket the phase between two control measurements and keep the best ratio.
 * Timing a phase and its control far apart lets frequency drift and GC leak
 * straight into the ratio.
 */
export function measurePhase({ run, flush, control, controlIters, rounds, iters, now }) {
  let bestMs = Infinity;
  let bestRatio = Infinity;
  const controlSamples = [];
  for (let r = 0; r < rounds; r++) {
    const before = timeIters(control, controlIters, now);
    const ms = timeIters(run, iters, now, flush);
    const after = timeIters(control, controlIters, now);
    controlSamples.push(before, after);
    bestMs = Math.min(bestMs, ms);
    bestRatio = Math.min(bestRatio, ms / ((before + after) / 2));
  }
  return { ms: bestMs, ratio: bestRatio, controlSamples };
}

/** Relative spread of the control across the whole run. >0.1 means distrust. */
export function controlDrift(samples) {
  const mid = median(samples);
  return (Math.max(...samples) - Math.min(...samples)) / mid;
}

/**
 * A CPU control that no optimization phase touches, mirroring the native
 * `control_memcpy`: a fixed-size typed-array copy.
 */
export function makeControl(length = 300_000) {
  const src = new Float32Array(length).fill(1);
  const dst = new Float32Array(length);
  return () => dst.set(src);
}

export function measureAll(phases, opts) {
  const {
    rounds = DEFAULT_ROUNDS,
    now = () => performance.now(),
    control = makeControl(),
  } = opts ?? {};

  const controlIters = calibrate(control, 20, now);
  const cases = {};
  const allControlSamples = [];
  for (const phase of phases) {
    if (phase.skip) continue;
    const iters = calibrate(phase.run, phase.warmup ?? 3, now, phase.flush);
    const result = measurePhase({
      run: phase.run,
      flush: phase.flush,
      control,
      controlIters,
      rounds,
      iters,
      now,
    });
    allControlSamples.push(...result.controlSamples);
    cases[phase.name] = {
      ms: Number(result.ms.toFixed(6)),
      ratio: Number(result.ratio.toFixed(6)),
      iters,
    };
  }
  return {
    schema: 1,
    cases,
    controlDrift: Number(controlDrift(allControlSamples).toFixed(4)),
    rounds,
  };
}

/**
 * Build the phase list from the seams glue.js exposes. GL phases flush with
 * `gl.finish()` once per timed segment — per call it would dominate the
 * measurement, and without it we would only be timing command submission.
 */
export function buildPhases(hooks) {
  const { flock, gl, uploadDots, buildTrails, buildLines, uploadLines, draw, refreshUniforms } = hooks;
  const finish = () => gl.finish();
  return [
    { name: 'wasm_step', run: () => flock.step(), warmup: 10 },
    { name: 'wasm_capture_trail', run: () => flock.capture_trail_frame(), warmup: 10 },
    { name: 'wasm_update_camera', run: () => flock.update_camera(gl.canvas.width, gl.canvas.height), warmup: 10 },
    { name: 'wasm_uniforms', run: () => refreshUniforms(), warmup: 50 },
    { name: 'gl_upload_dots', run: uploadDots, flush: finish, warmup: 5 },
    { name: 'wasm_build_trails', run: buildTrails, flush: finish, warmup: 3 },
    { name: 'js_build_lines', run: buildLines, warmup: 5 },
    { name: 'gl_upload_lines', run: () => uploadLines(buildLines()), flush: finish, warmup: 3 },
    { name: 'gl_draw', run: draw, flush: finish, warmup: 3 },
  ];
}

/**
 * Publish a run into the DOM when the page is opened with `?perf`. Tooling that
 * can navigate and read a page but cannot evaluate JavaScript — and CI — can
 * then drive the harness without a console.
 */
export async function autoRunFromQuery(harness, { search, document: doc, opts } = {}) {
  const query = search ?? globalThis.location?.search ?? '';
  if (!/[?&]perf(=|&|$)/.test(query)) return null;
  const target = doc ?? globalThis.document;
  if (!target) return null;
  const node = target.createElement('pre');
  node.id = 'perf-output';
  node.textContent = 'running…';
  target.body.append(node);
  try {
    const report = await harness.run(opts);
    node.textContent = JSON.stringify(report, null, 2);
    return report;
  } catch (error) {
    node.textContent = `perf harness failed: ${error?.message ?? error}`;
    throw error;
  }
}

/**
 * Install `window.perfHarness`. Kept out of glue.js so the timing logic stays
 * unit-testable in Node with an injected clock and stub hooks.
 */
export function installPerfHarness(hooks) {
  const harness = {
    async run(opts) {
      const wasRunning = hooks.isRunning();
      // The page's own rAF loop would otherwise compete with every sample.
      hooks.setRunning(false);
      await new Promise(resolve => requestAnimationFrame(() => resolve()));
      try {
        const report = measureAll(buildPhases(hooks), opts);
        report.n = hooks.flock.n();
        report.dim = hooks.flock.dim();
        report.trails = hooks.trailsEnabled();
        if (report.controlDrift > 0.1) {
          console.warn(
            `control drifted ${(report.controlDrift * 100).toFixed(0)}% during the run — ` +
              'distrust these numbers and re-run with a single quiet tab',
          );
        }
        return report;
      } finally {
        hooks.setRunning(wasRunning);
      }
    },
    summarize,
  };
  globalThis.perfHarness = harness;
  autoRunFromQuery(harness).catch(error => console.error(error));
  return harness;
}
