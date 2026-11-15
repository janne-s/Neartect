import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createParameterSync, plainOf } from '../src/plugin-parameters.js';
import { readSpace, SPACE_KIND } from '../src/domain/space.js';

const specs = JSON.parse(readFileSync(new URL('../plugin/tests/parameters.json', import.meta.url), 'utf8'));

const full = () => readSpace({
  kind: SPACE_KIND,
  objects: Array.from({ length: 18 }, (_, i) => ({ kind: i % 2 ? 'grating' : 'plane', id: `o${i}`, position: [i * 0.1, 0, 1] })),
  microphones: Array.from({ length: 8 }, (_, i) => ({ id: `m${i}`, position: [0, i * 0.1, 1], ...(i === 0 ? { reference: 'o0', offset: [0, 0.01, 0] } : {}) })),
  rigs: Array.from({ length: 4 }, (_, i) => ({ kind: 'ortf', id: `r${i}`, position: [i, 0, 1] })),
  source: { kind: 'point' }
});

test('every parameter finds its place in a space with every slot taken', () => {
  const space = full();
  const missing = specs.filter(spec => plainOf(space, spec) === undefined && !(spec.kind === 'number' && spec.path[0] === 'pan')).map(s => s.id);
  assert.deepEqual(missing, []);
  assert.equal(plainOf(space, specs.find(s => s.id === 'mic1_y')), 0.01, 'a bound microphone shows its offset');
});

test('the page sends only what differs, ends the gesture on commit, and does not echo the host', () => {
  const sent = [];
  const sync = createParameterSync(message => sent.push(message));
  const space = full();
  sync.start(specs, Object.fromEntries(specs.map(s => [s.id, plainOf(space, s) ?? 0])));
  sync.push(space, true);
  assert.deepEqual(sent, []);
  space.objects[2].position[0] = 1.5;
  sync.push(space, false);
  sync.push(space, false);
  assert.deepEqual(sent, [{ type: 'set', id: 'object3_x', value: 1.5 }]);
  sync.push(space, true);
  assert.deepEqual(sent.at(-1), { type: 'end' });
  sent.length = 0;
  sync.take(space, { object3_x: 2, mic2_pan_auto: 0, mic2_pan: -0.25, object1_material: 0 });
  assert.equal(space.objects[2].position[0], 2);
  assert.equal(space.microphones[1].pan, -0.25);
  assert.equal(space.objects[0].material, 'rigid');
  sync.push(space, true);
  assert.deepEqual(sent, []);
  sync.take(space, { mic2_pan_auto: 1 });
  assert.equal(space.microphones[1].pan, null);
});

test('objects past the sixteenth slot have no parameters', () => {
  const space = full();
  assert.deepEqual(space.objects.slice(16).map(o => o.slot), [null, null]);
  assert.ok(!specs.some(s => s.holder.list === 'objects' && s.holder.slot > 16));
});
