// Asserts the native bench has not regressed against perf-baseline.json.
// Run with: npm run test:perf   (needs a release build; see perf-baseline.mjs)

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  MAX_TOLERANCE,
  MIN_TOLERANCE,
  assertUsable,
  clampTolerance,
  compare,
  drift,
  formatRow,
  loadBaseline,
  median,
  ratios,
  runBench,
  summarize,
} from './perf-baseline.mjs';

test('ratios read the interleaved measurement and drop the controls', () => {
  const report = {
    profile: 'release',
    cases: [
      { name: 'control_alu', control: '', ms: 2, ratio: 0 },
      { name: 'hot', control: 'control_alu', ms: 6, ratio: 3 },
    ],
  };
  assert.deepEqual(ratios(report), { hot: 3 });
});

test('ratios accept a case whose work is now skipped entirely', () => {
  const report = {
    cases: [{ name: 'eliminated', control: 'control_memcpy', ms: 0, ratio: 0 }],
  };
  assert.deepEqual(ratios(report), { eliminated: 0 });
});

test('ratios reject a case that reported no usable ratio', () => {
  assert.throws(
    () => ratios({ cases: [{ name: 'hot', control: 'control_alu', ms: 1, ratio: NaN }] }),
    /reported ratio NaN/,
  );
  assert.throws(
    () => ratios({ cases: [{ name: 'hot', control: 'control_alu', ms: 1, ratio: -1 }] }),
    /reported ratio -1/,
  );
});

test('median picks the middle sample', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([5]), 5);
});

test('summarize derives the band from the observed spread', () => {
  // ~10% spread * 1.5 margin is under the floor, so the floor wins.
  const tight = summarize([1.0, 1.05, 1.1]);
  assert.equal(tight.ratio, 1.05);
  assert.equal(tight.tolerance, MIN_TOLERANCE);

  // ~33% spread * 1.5 margin clears the floor and is used as measured.
  const noisy = summarize([1.0, 1.2, 1.4]);
  assert.ok(noisy.tolerance > MIN_TOLERANCE);
  assert.equal(noisy.samples, 3);
});

test('tolerance is clamped to a usable range', () => {
  assert.equal(clampTolerance(0.01), MIN_TOLERANCE);
  assert.equal(clampTolerance(10), MAX_TOLERANCE);
  assert.equal(clampTolerance(0.3), 0.3);
});

test('drift is signed: positive is slower', () => {
  assert.ok(Math.abs(drift(1.2, 1.0) - 0.2) < 1e-9);
  assert.ok(Math.abs(drift(0.8, 1.0) + 0.2) < 1e-9);
  assert.equal(drift(1.0, 1.0), 0);
});

test('drift treats a regression away from zero as unbounded', () => {
  assert.equal(drift(0, 0), 0);
  assert.equal(drift(0.5, 0), Infinity);
});

test('summarize gives an eliminated case the floor band, not NaN', () => {
  const gone = summarize([0, 0, 0]);
  assert.equal(gone.ratio, 0);
  assert.equal(gone.tolerance, MIN_TOLERANCE);
});

test('compare separates regressions, improvements, missing and added cases', () => {
  const band = { tolerance: 0.15 };
  const result = compare(
    { slower: 2.0, faster: 0.5, steady: 1.0, brandnew: 1.0 },
    {
      slower: { ratio: 1.0, ...band },
      faster: { ratio: 1.0, ...band },
      steady: { ratio: 1.0, ...band },
      gone: { ratio: 1.0, ...band },
    },
  );
  assert.deepEqual(result.regressions.map(r => r.name), ['slower']);
  assert.deepEqual(result.improvements.map(r => r.name), ['faster']);
  assert.deepEqual(result.missing, ['gone']);
  assert.deepEqual(result.added, ['brandnew']);
});

test('compare honours the per-case band recorded in the baseline', () => {
  const wide = compare({ noisy: 1.4 }, { noisy: { ratio: 1.0, tolerance: 0.5 } });
  assert.deepEqual(wide.regressions, []);
  const narrow = compare({ noisy: 1.4 }, { noisy: { ratio: 1.0, tolerance: 0.15 } });
  assert.deepEqual(narrow.regressions.map(r => r.name), ['noisy']);
});

test('a debug-profile or stale-schema report is rejected before any threshold', () => {
  assert.throws(() => assertUsable({ profile: 'debug', schema: 2 }), /debug build/);
  assert.throws(() => assertUsable({ profile: 'release', schema: 1 }), /schema 1/);
});

test('native benchmarks stay within the measured band of the baseline', { timeout: 900_000 }, () => {
  const baseline = loadBaseline();
  const current = ratios(runBench());
  const { regressions, improvements, missing, added } = compare(current, baseline.cases);

  if (improvements.length) {
    console.log('Faster than baseline — re-bless deliberately with npm run perf:bless:');
    for (const row of improvements) console.log(formatRow(row));
  }
  if (added.length) console.log(`New cases not yet in the baseline: ${added.join(', ')}`);

  assert.deepEqual(missing, [], 'baseline cases disappeared from the bench');
  assert.equal(
    regressions.length,
    0,
    `\nPerformance regressions:\n${regressions.map(formatRow).join('\n')}\n`,
  );
});
