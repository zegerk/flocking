// Performance regression gate. Runs the native bench and compares each case's
// ratio-to-control against perf-baseline.json.
//
//   npm run test:perf                 assert against the baseline
//   npm run perf:bless                re-record the baseline (5 repeats)
//   npm run perf:bless -- --repeat 9  more repeats, better spread estimate
//
// Two things make this portable. Ratios rather than milliseconds: absolute
// timings swing several-fold between machines and between sessions on one
// machine, but a case's ratio to an untouched control does not. And measured
// rather than guessed tolerances: blessing repeats the whole bench and derives
// each case's band from the spread it actually observed, so a noisy case gets a
// wide band and a stable one stays tight.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { summarize } from './perf-stats.mjs';

export * from './perf-stats.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
export const BASELINE_PATH = join(ROOT, 'perf-baseline.json');

export const SCHEMA = 3;
export const BENCH_SCHEMA = 2;

/** Ratio-to-control per case, as measured interleaved by the Rust harness. */
export function ratios(report) {
  const out = {};
  for (const c of report.cases) {
    if (!c.control) continue;
    // Zero is legitimate — it means the case's work is now skipped entirely.
    if (!Number.isFinite(c.ratio) || c.ratio < 0) {
      throw new Error(`case ${c.name} reported ratio ${c.ratio}`);
    }
    out[c.name] = c.ratio;
  }
  return out;
}

export function assertUsable(report) {
  if (report.profile !== 'release') {
    throw new Error(
      `bench ran in the ${report.profile} profile; a debug build is several times ` +
        'slower and will trip every threshold. Run it via npm run test:perf.',
    );
  }
  if (report.schema !== BENCH_SCHEMA) {
    throw new Error(`bench emitted schema ${report.schema}, expected ${BENCH_SCHEMA}`);
  }
}

export function runBench() {
  const stdout = execFileSync(
    'cargo',
    ['run', '--release', '--quiet', '--example', 'bench', '--', '--json'],
    { cwd: ROOT, encoding: 'utf8', maxBuffer: 1 << 24 },
  );
  const start = stdout.indexOf('{');
  if (start < 0) throw new Error(`bench produced no JSON:\n${stdout}`);
  const report = JSON.parse(stdout.slice(start));
  assertUsable(report);
  return report;
}

export function bless(repeat = 5) {
  const runs = [];
  for (let i = 0; i < repeat; i++) runs.push(ratios(runBench()));

  const cases = {};
  for (const name of Object.keys(runs[0])) {
    cases[name] = summarize(runs.map(r => r[name]));
  }
  const baseline = {
    schema: SCHEMA,
    note: 'Ratio of each bench case to its control, with a tolerance measured from repeated runs. Re-record with: npm run perf:bless',
    recorded: new Date().toISOString().slice(0, 10),
    repeats: repeat,
    cases,
  };
  writeFileSync(BASELINE_PATH, `${JSON.stringify(baseline, null, 2)}\n`);
  return baseline;
}

export function loadBaseline() {
  if (!existsSync(BASELINE_PATH)) {
    throw new Error('perf-baseline.json is missing — seed it with: npm run perf:bless');
  }
  const baseline = JSON.parse(readFileSync(BASELINE_PATH, 'utf8'));
  if (baseline.schema !== SCHEMA) {
    throw new Error(
      `perf-baseline.json is schema ${baseline.schema}, expected ${SCHEMA} — re-record with npm run perf:bless`,
    );
  }
  return baseline;
}

if (process.argv.includes('--bless')) {
  const flag = process.argv.indexOf('--repeat');
  const repeat = flag < 0 ? 5 : Number(process.argv[flag + 1]);
  const baseline = bless(repeat);
  const names = Object.keys(baseline.cases);
  console.log(`Recorded ${names.length} cases from ${repeat} runs to perf-baseline.json`);
  for (const name of names) {
    const { ratio, tolerance } = baseline.cases[name];
    const band = `±${(tolerance * 100).toFixed(0)}%`;
    console.log(`  ${name.padEnd(28)} ${ratio.toFixed(3).padStart(10)}x  ${band}`);
  }
}
