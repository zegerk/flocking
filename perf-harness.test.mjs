// Unit tests for the in-page harness. The timing core takes an injected clock
// and plain function hooks, so it is exercised headlessly here rather than only
// by hand in a browser.

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  TARGET_SEGMENT_MS,
  autoRunFromQuery,
  buildPhases,
  calibrate,
  controlDrift,
  installPerfHarness,
  makeControl,
  measureAll,
  measurePhase,
  timeIters,
} from './perf-harness.mjs';

/** Deterministic clock: only the work we declare advances it. */
function fakeClock() {
  let t = 0;
  return {
    now: () => t,
    costing: ms => () => {
      t += ms;
    },
  };
}

test('timeIters reports the per-iteration cost', () => {
  const clock = fakeClock();
  assert.equal(timeIters(clock.costing(2), 5, clock.now), 2);
});

test('timeIters counts the flush once, inside the timed region', () => {
  const clock = fakeClock();
  let flushes = 0;
  const flush = () => {
    flushes++;
    clock.costing(10)();
  };
  // 4 iterations at 1ms plus a single 10ms flush, spread over 4 iterations.
  assert.equal(timeIters(clock.costing(1), 4, clock.now, flush), 3.5);
  assert.equal(flushes, 1);
});

test('calibrate scales iterations to fill one segment', () => {
  const clock = fakeClock();
  assert.equal(calibrate(clock.costing(1), 1, clock.now), TARGET_SEGMENT_MS);
  const slow = fakeClock();
  assert.equal(calibrate(slow.costing(30), 1, slow.now), 1);
});

test('calibrate falls back when the work is too fast to time', () => {
  const clock = fakeClock();
  assert.equal(calibrate(() => {}, 1, clock.now), 10_000);
});

test('measurePhase divides by the mean of the bracketing controls', () => {
  const clock = fakeClock();
  const result = measurePhase({
    run: clock.costing(4),
    control: clock.costing(1),
    controlIters: 1,
    rounds: 3,
    iters: 1,
    now: clock.now,
  });
  assert.equal(result.ms, 4);
  assert.equal(result.ratio, 4);
  // Two control samples per round.
  assert.equal(result.controlSamples.length, 6);
});

test('controlDrift is zero for a steady control and grows with spread', () => {
  assert.equal(controlDrift([2, 2, 2]), 0);
  assert.equal(controlDrift([1, 1, 2]), 1);
});

test('makeControl copies a fixed buffer and is stable in size', () => {
  const control = makeControl(8);
  assert.doesNotThrow(control);
});

test('measureAll scores every phase and skips the ones marked skip', () => {
  const clock = fakeClock();
  const report = measureAll(
    [
      { name: 'cheap', run: clock.costing(1), warmup: 1 },
      { name: 'dear', run: clock.costing(4), warmup: 1 },
      { name: 'off', run: clock.costing(99), warmup: 1, skip: true },
    ],
    { rounds: 2, now: clock.now, control: clock.costing(1) },
  );
  assert.deepEqual(Object.keys(report.cases), ['cheap', 'dear']);
  assert.equal(report.cases.dear.ratio / report.cases.cheap.ratio, 4);
  assert.equal(report.controlDrift, 0);
  assert.equal(report.rounds, 2);
});

test('buildPhases flushes the GL phases and leaves the CPU phases alone', () => {
  const noop = () => {};
  const phases = buildPhases({
    flock: { step: noop, capture_trail_frame: noop, update_camera: noop, n: () => 1, dim: () => 3 },
    gl: { canvas: { width: 100, height: 50 }, finish: noop },
    uploadDots: noop,
    buildTrails: noop,
    buildLines: noop,
    uploadLines: noop,
    draw: noop,
    refreshUniforms: noop,
  });
  const flushed = phases.filter(p => p.flush).map(p => p.name);
  assert.deepEqual(flushed, ['gl_upload_dots', 'wasm_build_trails', 'gl_upload_lines', 'gl_draw']);
  assert.ok(phases.some(p => p.name === 'wasm_step' && !p.flush));
});

test('the harness pauses the render loop and restores it afterwards', async () => {
  const clock = fakeClock();
  const noop = () => {};
  const states = [];
  let running = true;
  const originalRaf = globalThis.requestAnimationFrame;
  globalThis.requestAnimationFrame = cb => cb(0);

  try {
    const harness = installPerfHarness({
      flock: { step: noop, capture_trail_frame: noop, update_camera: noop, n: () => 7, dim: () => 4 },
      gl: { canvas: { width: 100, height: 50 }, finish: noop },
      uploadDots: noop,
      buildTrails: noop,
      buildLines: noop,
      uploadLines: noop,
      draw: noop,
      refreshUniforms: noop,
      isRunning: () => running,
      setRunning: r => {
        running = r;
        states.push(r);
      },
      trailsEnabled: () => true,
    });
    assert.equal(globalThis.perfHarness, harness);

    const report = await harness.run({ rounds: 1, now: clock.now, control: clock.costing(1) });
    assert.equal(report.n, 7);
    assert.equal(report.dim, 4);
    assert.equal(report.trails, true);
    assert.deepEqual(states, [false, true], 'loop must be paused, then restored');
  } finally {
    globalThis.requestAnimationFrame = originalRaf;
    delete globalThis.perfHarness;
  }
});

/** Minimal stand-in for the one element and two methods autoRun touches. */
function fakeDocument() {
  const body = { appended: [], append(node) { this.appended.push(node); } };
  return { body, createElement: () => ({ id: '', textContent: '' }) };
}

test('autoRun stays out of the way unless the page asks for it', async () => {
  const harness = { run: () => assert.fail('must not run') };
  for (const search of ['', '?other=1', '?perfect=1']) {
    assert.equal(await autoRunFromQuery(harness, { search, document: fakeDocument() }), null);
  }
});

test('autoRun publishes the report into the DOM for tools that cannot eval', async () => {
  const report = { cases: { gl_draw: { ms: 1 } }, controlDrift: 0.01 };
  const doc = fakeDocument();
  const result = await autoRunFromQuery({ run: async () => report }, { search: '?perf=1', document: doc });

  assert.deepEqual(result, report);
  assert.equal(doc.body.appended.length, 1);
  const node = doc.body.appended[0];
  assert.equal(node.id, 'perf-output');
  assert.deepEqual(JSON.parse(node.textContent), report);
});

test('autoRun reports a failure into the DOM instead of leaving it blank', async () => {
  const doc = fakeDocument();
  await assert.rejects(
    autoRunFromQuery({ run: async () => { throw new Error('no webgl'); } }, { search: '?perf', document: doc }),
    /no webgl/,
  );
  assert.match(doc.body.appended[0].textContent, /perf harness failed: no webgl/);
});
