import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nearFilter } from '../src/domain/near-filter.js';
import { defaultSpace, readSpace } from '../src/domain/space.js';

function magnitudeAt(samples, sampleRate, hz) {
  let re = 0;
  let im = 0;
  for (let n = 0; n < samples.length; n += 1) {
    re += samples[n] * Math.cos(2 * Math.PI * hz * n / sampleRate);
    im -= samples[n] * Math.sin(2 * Math.PI * hz * n / sampleRate);
  }
  return Math.hypot(re, im);
}

test('a space without surfaces gives a unit impulse', async () => {
  const { samples } = await nearFilter(readSpace({ kind: 'neartect.space', render: { lengthMilliseconds: 20 } }));
  assert.ok(Math.abs(samples[0] - 1) < 1e-6);
  assert.ok(samples.slice(1).every(x => Math.abs(x) < 1e-6));
});

test('the default space gives a finite filter of the stated length', async () => {
  const space = defaultSpace();
  const { samples, sampleRateHz } = await nearFilter(space);
  assert.equal(samples.length, Math.round(space.render.lengthMilliseconds / 1000 * sampleRateHz));
  assert.ok(samples.every(Number.isFinite));
});

test('a large rigid floor 1 cm below the microphone lifts by 6 dB and notches near 8.6 kHz', async () => {
  const space = readSpace({
    kind: 'neartect.space',
    air: { temperatureCelsius: 20 },
    microphone: { position: [0, 0, 0.01] },
    arrival: { azimuthDegrees: 0, elevationDegrees: 90 },
    render: { lengthMilliseconds: 60, order: 1 },
    objects: [{ kind: 'plane', position: [0, 0, 0], size: [6, 0, 6], tiltDegrees: 90, material: 'rigid' }]
  });
  const { samples, sampleRateHz } = await nearFilter(space);
  const low = magnitudeAt(samples, sampleRateHz, 1000);
  assert.ok(Math.abs(20 * Math.log10(low) - 6) < 0.5, `lift ${20 * Math.log10(low)} dB`);
  assert.ok(magnitudeAt(samples, sampleRateHz, 343.2 / 0.04) < 0.3);
});

test('the default space builds fast enough for live preview', async () => {
  const space = defaultSpace();
  await nearFilter(space);
  const start = performance.now();
  const runs = 5;
  for (let i = 0; i < runs; i += 1) await nearFilter(space);
  const ms = (performance.now() - start) / runs;
  console.log(`default space: ${ms.toFixed(1)} ms per filter`);
  assert.ok(ms < 200);
});

test('a grating differs from a plane and stays finite', async () => {
  const base = { kind: 'neartect.space', microphone: { position: [0, 0, 0.3] }, arrival: { elevationDegrees: 60 }, render: { lengthMilliseconds: 30, order: 1 } };
  const plate = { position: [0, 0, 0], size: [1, 0, 1], tiltDegrees: 90, material: 'rigid' };
  const solid = await nearFilter(readSpace({ ...base, objects: [{ ...plate, kind: 'plane' }] }));
  const grid = await nearFilter(readSpace({ ...base, objects: [{ ...plate, kind: 'grating', pattern: { periodMetres: [0.05, 0], barMetres: [0.01, 0] } }] }));
  assert.ok(grid.samples.every(Number.isFinite));
  const difference = solid.samples.reduce((sum, x, i) => sum + Math.abs(x - grid.samples[i]), 0);
  assert.ok(difference > 0.1);
});

test('every object kind gives a finite filter', async () => {
  for (const kind of ['plane', 'disc', 'grating', 'wall', 'box', 'corner', 'recess', 'pipe']) {
    const space = readSpace({ kind: 'neartect.space', microphone: { position: [0.1, 0.9, 1.1] }, render: { lengthMilliseconds: 30 }, objects: [{ kind, position: [0, 0, 1] }] });
    const { samples } = await nearFilter(space);
    assert.ok(samples.every(Number.isFinite), kind);
  }
});

test('every situation gives a finite filter that differs from the free field', async () => {
  const { SITUATIONS, situationSpace } = await import('../src/domain/situations.js');
  const { defaultSpace } = await import('../src/domain/space.js');
  for (const id of Object.keys(SITUATIONS)) {
    const { samples } = await nearFilter(situationSpace(id, defaultSpace()));
    assert.ok(samples.every(Number.isFinite), id);
    assert.ok(samples.slice(1).some(x => Math.abs(x) > 1e-3), `${id} adds nothing`);
  }
});
