import { test } from 'node:test';
import assert from 'node:assert/strict';
import { bentPoint, flatPoint } from '../src/domain/bend.js';
import { objectBend, pointPosition, referTo, surfaceFrame } from '../src/domain/objects.js';
import { nearestSurface } from '../src/domain/placement.js';
import { nearFilter } from '../src/domain/near-filter.js';
import { readSpace } from '../src/domain/space.js';

const close = (a, b, tolerance = 1e-9) => a.every((v, i) => Math.abs(v - b[i]) < tolerance);
const wall = (angleDegrees, axis = 'up') => ({ kind: 'plane', name: 'Wall', position: [0, -1, 1.5], size: [3, 0, 3], material: 'rigid', bend: { angleDegrees, axis } });

test('the reader keeps a bend on planes, walls, and gratings only, within ±180°', () => {
  const space = readSpace({ kind: 'neartect.space', objects: [wall(400), { kind: 'box', bend: { angleDegrees: 30 } }, { ...wall(20), bend: { axis: 'sideways' } }] });
  assert.deepEqual(space.objects[0].bend, { angleDegrees: 180, axis: 'up' });
  assert.equal(space.objects[1].bend, undefined);
  assert.deepEqual(space.objects[2].bend, { angleDegrees: 0, axis: 'up' });
});

test('a point laid flat and bent comes back where it was, about either axis', () => {
  for (const axis of ['up', 'across']) {
    for (const angle of [120, -75]) {
      const object = readSpace({ kind: 'neartect.space', objects: [wall(angle, axis)] }).objects[0];
      const bend = objectBend(object);
      const flat = [0.9, -1 + 0.3, 1.2];
      const bent = bentPoint(bend, flat);
      assert.ok(close(flatPoint(bend, bent), flat), `${axis} ${angle}`);
    }
  }
});

test('a half circle bent towards its front puts its ends a diameter apart, a radius in front', () => {
  const object = readSpace({ kind: 'neartect.space', objects: [wall(180)] }).objects[0];
  const bend = objectBend(object);
  const radius = 3 / Math.PI;
  const ends = [1.5, -1.5].map(x => bentPoint(bend, [x, -1, 1.5]));
  assert.ok(close(ends[0], [radius, -1 + radius, 1.5]) && close(ends[1], [-radius, -1 + radius, 1.5]), ends.join(' '));
});

test('a microphone bound to a bent wall keeps its gap along the arc’s normal', () => {
  const space = readSpace({ kind: 'neartect.space', objects: [wall(90)], microphones: [{ position: [0, -0.8, 1.5] }] });
  const microphone = space.microphones[0];
  referTo(space, microphone, space.objects[0].id);
  microphone.offset = [1, 0.2, 0];
  const at = pointPosition(space, microphone);
  const nearest = nearestSurface(space, at);
  assert.ok(Math.abs(Math.abs(nearest.distance) - 0.2) < 1e-9, `gap ${nearest.distance}`);
  const { front } = surfaceFrame(space.objects[0], [1, 0, 0]);
  assert.ok(close(nearest.normal.map(Math.abs), front.map(Math.abs), 1e-9));
});

test('a concave wall gathers more of a frontal wave than the flat wall at its focus', async () => {
  const space = angle => readSpace({
    kind: 'neartect.space',
    microphones: [{ position: [0, -1 + 3 / (Math.PI / 2) / 2, 1.5] }],
    arrival: { azimuthDegrees: 0, elevationDegrees: 0 },
    render: { lengthMilliseconds: 40, order: 1 },
    objects: [wall(angle)]
  });
  const energy = samples => samples.slice(1).reduce((sum, x) => sum + x * x, 0);
  const flat = await nearFilter(space(0));
  const concave = await nearFilter(space(90));
  assert.ok(concave.samples.every(Number.isFinite));
  assert.ok(energy(concave.samples) > 2 * energy(flat.samples), `${energy(concave.samples)} vs ${energy(flat.samples)}`);
});
