export const TRAIL_VERTEX_BUDGET = 250000;
export const TRAIL_FPS_FLOOR = 10;
export const TRAIL_FPS_RECOVER = 12;
export const TRAIL_HISTORY_BYTE_BUDGET = 256 * 1024 * 1024;
export const MIN_TRAIL_FRAMES = 2;
export const MAX_TRAIL_FRAMES = 120;
export const MAX_POPULATION = 1_000_000;

export function populationForSliderValue(value) {
  const position = Math.max(0, Math.min(1000, Math.floor(value)));
  return Math.max(3, Math.round(3 * Math.pow(MAX_POPULATION / 3, position / 1000)));
}

export function maxPopulationForTrailLength(frames) {
  const length = Math.max(MIN_TRAIL_FRAMES, Math.min(MAX_TRAIL_FRAMES, Math.floor(frames)));
  return Math.min(
    MAX_POPULATION,
    Math.max(3, Math.floor(TRAIL_HISTORY_BYTE_BUDGET / (length * 5 * 4))),
  );
}

export function maxPopulationSliderValue(frames) {
  const limit = maxPopulationForTrailLength(frames);
  let low = 0;
  let high = 1000;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (populationForSliderValue(middle) <= limit) low = middle;
    else high = middle - 1;
  }
  return low;
}

export function calculateTrailBudget(population, requestedQuality, trailSlots) {
  const count = Math.max(0, Math.floor(population));
  const quality = Math.max(0, Math.min(1, requestedQuality));
  const verticesPerTrail = Math.max(1, (Math.floor(trailSlots) - 1) * 2);
  const safeCount = Math.floor(TRAIL_VERTEX_BUDGET / verticesPerTrail);
  const requestedCount = quality >= 1 ? count : Math.floor(count * quality);
  const selected = Math.min(count, requestedCount, safeCount);
  return {
    selected,
    effective: count > 0 ? selected / count : 0,
    ceiling: count > 0 ? Math.min(1, safeCount / count) : 0,
  };
}

export function nextTrailQuality(
  requestedQuality,
  effectiveQuality,
  fps,
  population,
  ceilingQuality = 1,
) {
  const requested = Math.max(0, Math.min(1, requestedQuality));
  if (fps < TRAIL_FPS_FLOOR) {
    const next = Math.max(0, Math.min(1, effectiveQuality * fps / TRAIL_FPS_FLOOR));
    return next * population < 1 ? 0 : next;
  }
  if (fps > TRAIL_FPS_RECOVER) {
    return Math.min(1, requested + 0.05 * Math.max(0, Math.min(1, ceilingQuality)));
  }
  return requested;
}

export function trailQualityLabel(enabled, effectiveQuality) {
  if (!enabled) return 'off';
  if (effectiveQuality <= 0) return '0%';
  const percent = effectiveQuality * 100;
  return percent < 1 ? '<1%' : `${Math.round(percent)}%`;
}
