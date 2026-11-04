import { test } from 'node:test';
import assert from 'node:assert/strict';
import { bentPoint, flatPoint } from '../src/domain/bend.js';
import { objectBend, pointPosition, referTo, surfaceFrame } from '../src/domain/objects.js';
import { nearestSurface } from '../src/domain/placement.js';
import { nearFilter } from '../src/domain/near-filter.js';
import { readSpace } from '../src/domain/space.js';

const close = (a, b, tolerance = 1e-9) => a.every((v, i) => Math.abs(v - b[i]) < tolerance);
const wall = (angleDegrees, axis = 'up') => ({ kind: 'plane', name: 'Wall', position: [0, -1, 1.5], size: [3, 0, 3], material: 'rigid', bend: { angleDegrees, axis } });

test('the reader keeps a bend on planes, walls, gratings, pipes, and discs only, within ±180°', () => {
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

test('a bent pipe holds a microphone on its arc and turns from mouth A to mouth B', async () => {
  const { pipeAround, pipeCentreline } = await import('../src/domain/objects.js');
  const space = readSpace({ kind: 'neartect.space', objects: [{ kind: 'pipe', position: [0, 0, 1], size: [0.2, 2, 0.2], bend: { angleDegrees: 90, axis: 'up' } }] });
  const pipe = space.objects[0];
  const microphone = { reference: pipe.id, offset: [0, -1, 0], position: [0, 0, 0] };
  const at = pointPosition(space, microphone);
  assert.ok(pipeAround(space, at), 'the middle of the arc is inside');
  assert.equal(pipeAround(space, [0, 0, 1 + 0.5]), null);
  const { mouthA, mouthB, bend } = pipeCentreline(pipe);
  const radius = 2 / (Math.PI / 2);
  const a = mouthA.map((v, i) => v - bend.centre[i]);
  const b = mouthB.map((v, i) => v - bend.centre[i]);
  assert.ok(Math.abs(Math.hypot(...a) - radius) < 1e-9 && Math.abs(Math.hypot(...b) - radius) < 1e-9);
  const turn = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  assert.ok(turn.reduce((s, v, i) => s + v * bend.axis[i], 0) > 0, 'A turns towards B about the axis');
});

test('a microphone inside a bent pipe gets a finite filter unlike the straight pipe’s', async () => {
  const space = angle => readSpace({
    kind: 'neartect.space',
    render: { lengthMilliseconds: 40, order: 1 },
    objects: [{ kind: 'pipe', name: 'Pipe', position: [0, 0, 1], size: [0.3, 2, 0.3], bend: { angleDegrees: angle, axis: 'up' } }],
    microphones: [{ position: [0, 0, 1] }]
  }, 0);
  const straight = await nearFilter(space(0));
  const bent = await nearFilter(space(150));
  assert.ok(bent.samples.every(Number.isFinite));
  const difference = bent.samples.reduce((s, x, i) => s + (x - straight.samples[i]) ** 2, 0);
  assert.ok(difference > 1e-6, `difference ${difference}`);
});

const dish = angleDegrees => ({ kind: 'disc', name: 'Dish', position: [0, 0, 0], size: [2, 0, 2], tiltDegrees: 90, material: 'rigid', bend: { angleDegrees, axis: 'up' } });

test('a point on a bowl or a dome comes back where it was and keeps its gap along the sphere’s normal', () => {
  for (const angle of [120, -75]) {
    const object = readSpace({ kind: 'neartect.space', objects: [dish(angle)] }).objects[0];
    const bend = objectBend(object);
    const flat = [0.4, -0.5, 0.15];
    const bent = bentPoint(bend, flat);
    assert.ok(close(flatPoint(bend, bent), flat), `${angle}`);
    const radius = 2 / (Math.abs(angle) * Math.PI / 180);
    const centre = [0, 0, Math.sign(angle) * radius];
    const surface = bentPoint(bend, [0.4, -0.5, 0]);
    assert.ok(Math.abs(Math.hypot(...surface.map((v, i) => v - centre[i])) - radius) < 1e-9);
    const { across, front, up } = surfaceFrame(object, [0.4, 0, 0.5]);
    const dot = (a, b) => a.reduce((s, v, i) => s + v * b[i], 0);
    assert.ok(Math.abs(dot(across, front)) < 1e-9 && Math.abs(dot(up, front)) < 1e-9 && Math.abs(dot(across, up)) < 1e-9);
  }
});

test('a bowl gathers more of a wave from above than the flat disc at its focus', async () => {
  const space = angle => readSpace({
    kind: 'neartect.space',
    microphones: [{ position: [0, 0, 2 / (Math.PI / 2) / 2] }],
    arrival: { azimuthDegrees: 0, elevationDegrees: 90 },
    render: { lengthMilliseconds: 40, order: 1 },
    objects: [dish(angle)]
  });
  const energy = samples => samples.slice(1).reduce((sum, x) => sum + x * x, 0);
  const flat = await nearFilter(space(0));
  const bowl = await nearFilter(space(90));
  assert.ok(bowl.samples.every(Number.isFinite));
  assert.ok(energy(bowl.samples) > 2 * energy(flat.samples), `${energy(bowl.samples)} vs ${energy(flat.samples)}`);
});
