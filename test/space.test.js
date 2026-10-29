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
  const nearest = nearestSurface(space);
  space.microphone.position = withGap(space, nearest, 0.004);
  assert.ok(Math.abs(Math.abs(nearestSurface(space).distance) - 0.004) < 1e-9);
});
