import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadCore } from '../src/domain/core.js';
import { readFileSync } from 'node:fs';
import { SPACE_KIND } from '../src/domain/space.js';

const core = await loadCore();

function close(actual, expected, path) {
  if (typeof expected === 'number') {
    assert.equal(typeof actual, 'number', path);
    const scale = Math.max(1, Math.abs(expected));
    assert.ok(Math.abs(actual - expected) <= 1e-9 * scale, `${path}: ${actual} vs ${expected}`);
    return;
  }
  if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), path);
    assert.equal(actual.length, expected.length, `${path} length`);
    expected.forEach((v, i) => close(actual[i], v, `${path}[${i}]`));
    return;
  }
  if (expected && typeof expected === 'object') {
    assert.deepEqual(Object.keys(actual).filter(k => k !== 'slot').sort(), Object.keys(expected).sort(), `${path} keys`);
    for (const key of Object.keys(expected)) close(actual[key], expected[key], `${path}.${key}`);
    return;
  }
  assert.equal(actual, expected, path);
}

const recorded = JSON.parse(readFileSync(new URL('./fixtures/js-reader-and-jobs.json', import.meta.url), 'utf8'));

const withoutIds = space => JSON.parse(JSON.stringify(space, (key, value) => (key === 'id' ? undefined : value)));

test('the Rust reader reads a space as the JS reader did', () => {
  for (const { name, space } of recorded.spaces) close(JSON.parse(core.readSpace(JSON.stringify(space))), space, name);
  recorded.read.forEach(({ document, space }, i) => close(withoutIds(JSON.parse(core.readSpace(JSON.stringify(document)))), space, `messy ${i}`));
});

test('the Rust reader refuses what the JS reader refuses', () => {
  for (const text of ['{"kind":"rumbletect.scene"}', '{"kind":"neartect.space","objects":{}}', '[]', 'not json']) {
    assert.throws(() => core.readSpace(text), /not a Neartect space|not a list/);
  }
});

test('the Rust jobs match the JS jobs as recorded', () => {
  for (const { name, space, jobs } of recorded.spaces) close(Array.from(core.nearJobs(JSON.stringify(space))), jobs, name);
});

test('slots: kept, filled in order, freed slots stay free, none past sixteen objects', () => {
  const objects = Array.from({ length: 18 }, (_, i) => ({ kind: 'plane', id: `p${i}`, ...(i === 0 ? { slot: 5 } : i === 1 ? { slot: null } : i === 2 ? { slot: 5 } : {}) }));
  const space = JSON.parse(core.readSpace(JSON.stringify({ kind: SPACE_KIND, objects, microphones: [{ slot: 3 }, {}, { slot: 3 }], rigs: [{ slot: 9 }, { slot: 2 }] })));
  assert.deepEqual(space.objects.map(o => o.slot), [5, null, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, null]);
  assert.deepEqual(space.microphones.map(m => m.slot), [3, 1, 2]);
  assert.deepEqual(space.rigs.map(r => r.slot), [1, 2]);
  const again = JSON.parse(core.readSpace(JSON.stringify(space)));
  assert.deepEqual(again.objects.map(o => o.slot), space.objects.map(o => o.slot));
});
