import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultObject, defaultSpace, readSpace, SpaceError } from '../src/domain/space.js';
import { expandObject, objectFrame } from '../src/domain/objects.js';
import { nearestSurface, withGap } from '../src/domain/placement.js';

const round = list => list.map(v => Math.round(v * 1e9) / 1e9 + 0);

test('a space round-trips through JSON', () => {
  const space = defaultSpace();
  assert.deepEqual(readSpace(JSON.parse(JSON.stringify(space))), space);
});

test('the reader fills defaults, drops unknown fields, and clamps ranges', () => {
  const space = readSpace({ kind: 'neartect.space', extra: 1, render: { order: 40 }, objects: [{ kind: 'box', strength: -3, material: 'cheese' }] });
  assert.equal(space.extra, undefined);
  assert.equal(space.render.order, 6);
  assert.equal(space.objects[0].strength, 0);
  assert.equal(space.objects[0].material, 'concrete');
});

test('the reader refuses a document of another kind or with broken objects', () => {
  assert.throws(() => readSpace({ kind: 'rumbletect.scene' }), SpaceError);
  assert.throws(() => readSpace({ kind: 'neartect.space', objects: {} }), SpaceError);
});

test('a plane tilted 90 degrees faces up and one turned to 90 faces east', () => {
  assert.deepEqual(round(objectFrame({ yawDegrees: 0, tiltDegrees: 90 }).front), [0, 0, 1]);
  assert.deepEqual(round(objectFrame({ yawDegrees: 90, tiltDegrees: 0 }).front), [1, 0, 0]);
});

test('tilt turns a full circle about the across axis and reads back into −180…180', () => {
  const space = readSpace({ kind: 'neartect.space', objects: [{ kind: 'plane', tiltDegrees: 270 }, { kind: 'plane', tiltDegrees: -200 }] });
  assert.deepEqual(space.objects.map(o => o.tiltDegrees), [-90, 160]);
  const over = objectFrame({ yawDegrees: 0, tiltDegrees: 120 });
  assert.deepEqual(round(over.front), round([0, Math.cos(2 * Math.PI / 3), Math.sin(2 * Math.PI / 3)]));
  assert.deepEqual(round(over.across), [1, 0, 0]);
  assert.deepEqual(round(objectFrame({ yawDegrees: 0, tiltDegrees: 180 }).front), [0, -1, 0]);
});

test('objects expand into the expected faces', () => {
  const count = kind => expandObject(defaultObject(kind)).length;
  assert.deepEqual(['plane', 'disc', 'grating', 'wall', 'box', 'corner', 'recess', 'pipe'].map(count), [1, 1, 1, 1, 6, 3, 6, 16]);
  assert.equal(expandObject(defaultObject('disc'))[0].vertices.length, 48);
  assert.equal(expandObject(defaultObject('wall'))[0].pattern.kind, 'opening');
});

test('a corner\'s faces share their edges exactly', () => {
  const faces = expandObject(defaultObject('corner', { position: [1, 2, 0] }));
  const key = p => p.map(v => v.toFixed(9)).join(',');
  const shared = faces[0].vertices.filter(v => faces[1].vertices.some(w => key(w) === key(v)));
  assert.equal(shared.length, 2);
});

test('the gap slider puts the microphone at the stated distance from the nearest face', () => {
  const space = defaultSpace();
  const microphone = space.microphones[0];
  microphone.position = withGap(microphone.position, nearestSurface(space, microphone.position), 0.004);
  assert.ok(Math.abs(Math.abs(nearestSurface(space, microphone.position).distance) - 0.004) < 1e-9);
});

test('the microphone follows its reference object and keeps a signed gap', async () => {
  const { pointPosition, referTo } = await import('../src/domain/objects.js');
  const space = defaultSpace();
  const wall = space.objects[1];
  const microphone = space.microphones[0];
  assert.equal(microphone.reference, wall.id);
  assert.ok(Math.abs(microphone.offset[1] - 0.3) < 1e-9);
  wall.position[1] += 1;
  assert.ok(Math.abs(pointPosition(space, microphone)[1] - (-0.7 + 1)) < 1e-9);
  microphone.offset[1] = -0.05;
  assert.ok(pointPosition(space, microphone)[1] < wall.position[1]);
  referTo(space, microphone, null);
  assert.equal(microphone.reference, null);
});

test('an old single microphone reads as a list of one, and the source defaults to far away', () => {
  const space = readSpace({ kind: 'neartect.space', microphone: { position: [1, 2, 3] } });
  assert.equal(space.microphones.length, 1);
  assert.deepEqual(space.microphones[0].position, [1, 2, 3]);
  assert.equal(space.source.kind, 'far');
});

test('removing an object releases the points bound to it', async () => {
  const { releaseObject } = await import('../src/domain/objects.js');
  const space = defaultSpace();
  const wall = space.objects[1];
  const before = [...space.microphones[0].position];
  releaseObject(space, wall.id);
  assert.equal(space.microphones[0].reference, null);
  assert.deepEqual(space.microphones[0].position, before);
});

test('a situation keeps the microphones and their spacing', async () => {
  const { situationSpace } = await import('../src/domain/situations.js');
  const space = readSpace({ kind: 'neartect.space', microphones: [{ position: [0, 0, 1] }, { position: [0.3, 0, 1] }, { position: [0.6, 0, 1] }], source: { kind: 'point', position: [0, 2, 1] } });
  const next = situationSpace('corner', space);
  assert.equal(next.microphones.length, 3);
  const [a, b] = next.microphones.map(m => m.position);
  assert.ok(Math.abs(b[0] - a[0] - 0.3) < 1e-9);
  assert.equal(next.source.kind, 'point');
  assert.ok(Math.abs(next.source.position[1] - a[1] - 2) < 1e-9);
});

test('a situation adds its objects to those built, which stay as they were', async () => {
  const { situationSpace } = await import('../src/domain/situations.js');
  const space = defaultSpace();
  space.objects[1].strength = 0;
  const next = situationSpace('corner', space);
  assert.deepEqual(next.objects.slice(0, 2).map(o => [o.id, o.strength]), space.objects.map(o => [o.id, o.strength]));
  assert.equal(next.objects.length, 3);
  assert.equal(next.microphones[0].reference, next.objects[2].id);
});

test('render quality takes defaults and clamps into range', () => {
  const plain = readSpace({ kind: 'neartect.space' });
  assert.deepEqual(plain.render.quality, { reliefDetail: 6, elementLimit: 400000, movingDetail: 0.5 });
  const odd = readSpace({ kind: 'neartect.space', render: { quality: { reliefDetail: 100, elementLimit: 3, movingDetail: 'x' } } });
  assert.deepEqual(odd.render.quality, { reliefDetail: 24, elementLimit: 1000, movingDetail: 0.5 });
});

test('a head reads its model, measured by default, and other layouts carry none', () => {
  const space = readSpace({ kind: 'neartect.space', rigs: [{ kind: 'binaural' }, { kind: 'binaural', head: 'sphere' }, { kind: 'binaural', head: 'wax' }, { kind: 'xy', head: 'sphere' }] });
  assert.deepEqual(space.rigs.map(r => r.head), ['measured', 'sphere', 'measured', undefined]);
});

test('a saved space states its classes, with strength other than 1 as C, and reads back without them', async () => {
  const { withReliability } = await import('../src/domain/reliability.js');
  const space = readSpace({ kind: 'neartect.space', objects: [{ kind: 'plane', material: 'rigid' }] });
  assert.equal(withReliability(space).reliability.lowest, 'B');
  space.objects[0].strength = 1.5;
  const saved = withReliability(space);
  assert.equal(saved.reliability.components.strength, 'C');
  assert.equal(saved.reliability.lowest, 'C');
  assert.equal(readSpace(JSON.parse(JSON.stringify(saved))).reliability, undefined);
});

test('a wall keeps its opening inside, whatever its size and offset', () => {
  const space = readSpace({ kind: 'neartect.space', objects: [{ kind: 'wall', size: [3, 0, 2.5], opening: { shape: 'rectangle', sizeMetres: [0.9, 4], offsetMetres: [1.4, 0] } }] });
  assert.deepEqual(space.objects[0].opening.sizeMetres, [0.9, 2.5]);
  assert.ok(Math.abs(space.objects[0].opening.offsetMetres[0] - 1.05) < 1e-12);
  const values = expandObject({ ...space.objects[0], size: [1, 0, 2.5] })[0].pattern.values;
  assert.ok(Math.abs(values[0] - 0.55) < 1e-12 && values[0] + values[2] <= 1 + 1e-12);
});

test('centring a bound point puts it mid-pipe, or mid-wall at the same gap', async () => {
  const { centredOffset, pointPosition } = await import('../src/domain/objects.js');
  const space = readSpace({ kind: 'neartect.space', objects: [
    { kind: 'pipe', position: [1, 2, 0.5], size: [0.3, 2, 0.3], yawDegrees: 90 },
    { kind: 'wall', size: [5, 0, 2.5], opening: { shape: 'rectangle', sizeMetres: [0.9, 2.1], offsetMetres: [0.5, -0.2] } }
  ] });
  const [pipe, wall] = space.objects;
  const point = { reference: pipe.id, offset: centredOffset(pipe, [0.05, 0.3, 0.02]), position: [0, 0, 0] };
  assert.deepEqual(round(pointPosition(space, point)), [1, 2, 0.5]);
  assert.deepEqual(round(centredOffset(wall, [1, 0.25, 1])), [0.5, 0.25, -0.2]);
});
