export const TRAIL_VERTEX_BUDGET = 250000;
export const TRAIL_FPS_FLOOR = 10;
export const TRAIL_FPS_RECOVER = 12;

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
