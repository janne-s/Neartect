import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultSpace, readSpace, SpaceError, surfaceFrame, surfaceVertices, microphoneDistance } from '../src/domain/space.js';

test('a space round-trips through JSON', () => {
  const space = defaultSpace();
  assert.deepEqual(readSpace(JSON.parse(JSON.stringify(space))), space);
});

test('the reader fills defaults, drops unknown fields, and clamps ranges', () => {
  const space = readSpace({ kind: 'neartect.space', extra: 1, render: { order: 40 }, surfaces: [{ strength: -3, material: 'cheese' }] });
  assert.equal(space.extra, undefined);
  assert.equal(space.render.order, 6);
  assert.equal(space.surfaces[0].strength, 0);
  assert.equal(space.surfaces[0].material, 'concrete');
});

test('the reader refuses a document of another kind or with broken surfaces', () => {
  assert.throws(() => readSpace({ kind: 'rumbletect.scene' }), SpaceError);
  assert.throws(() => readSpace({ kind: 'neartect.space', surfaces: {} }), SpaceError);
});

test('a floor faces up and a wall at yaw 90 faces east', () => {
  assert.deepEqual(surfaceFrame({ yawDegrees: 0, tiltDegrees: 90 }).normal.map(v => Math.round(v * 1e9) / 1e9), [0, 0, 1]);
  assert.deepEqual(surfaceFrame({ yawDegrees: 90, tiltDegrees: 0 }).normal.map(v => Math.round(v * 1e9) / 1e9), [1, 0, 0]);
});

test('the default wall stands on the floor edge and the microphone is 30 cm in front of it', () => {
  const space = defaultSpace();
  const [floor, wall] = space.surfaces;
  const lowest = Math.min(...surfaceVertices(wall).map(v => v[2]));
  const floorSouth = Math.min(...surfaceVertices(floor).map(v => v[1]));
  assert.ok(Math.abs(lowest) < 1e-9 && Math.abs(floorSouth + 1) < 1e-9);
  assert.ok(Math.abs(microphoneDistance(space, wall) - 0.3) < 1e-9);
});

test('a grid pattern keeps bars no wider than their period', () => {
  const space = readSpace({ kind: 'neartect.space', surfaces: [{ pattern: { kind: 'grid', periodMetres: [0.02, 0], barMetres: [0.05, 0.01] } }] });
  assert.deepEqual(space.surfaces[0].pattern.barMetres, [0.02, 0]);
});
