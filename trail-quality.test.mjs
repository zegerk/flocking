import test from 'node:test';
import assert from 'node:assert/strict';

import {
  calculateTrailBudget,
  maxPopulationForTrailLength,
  maxPopulationSliderValue,
  nextTrailQuality,
  populationForSliderValue,
  trailQualityLabel,
} from './trail-quality.mjs';

test('trail length limits population within the history memory budget', () => {
  assert.equal(maxPopulationForTrailLength(2), 1_000_000);
  assert.equal(maxPopulationForTrailLength(30), 447_392);
  assert.equal(maxPopulationForTrailLength(120), 111_848);

  for (const frames of [2, 30, 120]) {
    const position = maxPopulationSliderValue(frames);
    const population = populationForSliderValue(position);
    assert.ok(population <= maxPopulationForTrailLength(frames));
    assert.ok(population * frames * 5 * 4 <= 256 * 1024 * 1024);
    if (position < 1000) {
      assert.ok(populationForSliderValue(position + 1) > maxPopulationForTrailLength(frames));
    }
  }
});

test('trail budget starts complete and respects the geometry ceiling', () => {
  assert.deepEqual(calculateTrailBudget(2178, 1, 30), {
    selected: 2178,
    effective: 1,
    ceiling: 1,
  });
  assert.deepEqual(calculateTrailBudget(2178, 1, 120), {
    selected: 2178,
    effective: 1,
    ceiling: 1,
  });

  const large = calculateTrailBudget(1_000_000, 1, 30);
  assert.equal(large.selected, 64_280);
  assert.equal(large.effective, 0.06428);
  assert.equal(large.ceiling, 0.06428);
});

test('quality controller reduces, holds, restores, and reaches zero', () => {
  assert.equal(nextTrailQuality(1, 1, 5, 100), 0.5);
  assert.equal(nextTrailQuality(0.4, 0.4, 5, 100), 0.2);
  assert.equal(nextTrailQuality(0.4, 0.4, 10, 100), 0.4);
  assert.equal(nextTrailQuality(0.4, 0.4, 11, 100), 0.4);
  assert.equal(nextTrailQuality(0.4, 0.4, 12, 100), 0.4);
  assert.ok(Math.abs(nextTrailQuality(0.4, 0.4, 13, 100) - 0.45) < 1e-12);
  assert.equal(nextTrailQuality(0.001, 0.001, 5, 100), 0);
});

test('recovery scales with the allocation-safe ceiling', () => {
  assert.ok(
    Math.abs(nextTrailQuality(0, 0, 13, 1_000_000, 0.00431) - 0.0002155) < 1e-12,
  );
});

test('quality label distinguishes off, zero, and sub-percent trails', () => {
  assert.equal(trailQualityLabel(false, 1), 'off');
  assert.equal(trailQualityLabel(true, 0), '0%');
  assert.equal(trailQualityLabel(true, 0.00431), '<1%');
  assert.equal(trailQualityLabel(true, 0.514), '51%');
});
