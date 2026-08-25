// Pure statistics shared by the native gate (perf-baseline.mjs, Node) and the
// in-page harness (perf-harness.mjs, browser). No I/O, no platform APIs, so
// both environments can import it and both are scored the same way.

// Floor and ceiling for a measured tolerance. The ceiling stops a hopelessly
// noisy case from quietly becoming an un-gate.
export const MIN_TOLERANCE = 0.15;
export const MAX_TOLERANCE = 0.6;
// Headroom above the spread observed while blessing.
export const SPREAD_MARGIN = 1.5;

export function median(xs) {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

export function clampTolerance(t) {
  return Math.min(MAX_TOLERANCE, Math.max(MIN_TOLERANCE, t));
}

/** Collapse repeated runs into a baseline ratio plus a measured band. */
export function summarize(samples) {
  const mid = median(samples);
  // A case whose work has been optimized away measures zero, and zero has no
  // meaningful relative spread — fall back to the floor band.
  const spread = mid > 0 ? (Math.max(...samples) - Math.min(...samples)) / mid : 0;
  return {
    ratio: Number(mid.toFixed(6)),
    tolerance: Number(clampTolerance(spread * SPREAD_MARGIN).toFixed(3)),
    samples: samples.length,
  };
}

/**
 * Signed relative change vs baseline: >0 is slower, <0 is faster. A baseline of
 * zero means the work was previously eliminated, so any measurable cost now is
 * an unbounded regression.
 */
export function drift(current, baseline) {
  if (baseline === 0) return current === 0 ? 0 : Infinity;
  return (current - baseline) / baseline;
}

export function compare(currentRatios, baselineCases) {
  const regressions = [];
  const improvements = [];
  const missing = [];
  for (const [name, entry] of Object.entries(baselineCases)) {
    if (!(name in currentRatios)) {
      missing.push(name);
      continue;
    }
    const d = drift(currentRatios[name], entry.ratio);
    const tolerance = entry.tolerance ?? MIN_TOLERANCE;
    if (d > tolerance) regressions.push({ name, drift: d, tolerance });
    else if (d < -tolerance) improvements.push({ name, drift: d, tolerance });
  }
  const added = Object.keys(currentRatios).filter(n => !(n in baselineCases));
  return { regressions, improvements, missing, added };
}

export function formatRow({ name, drift: d, tolerance }) {
  const pct = `${(d * 100).toFixed(1)}%`.padStart(8);
  return `  ${name.padEnd(28)} ${pct}  (band ±${(tolerance * 100).toFixed(0)}%)`;
}
