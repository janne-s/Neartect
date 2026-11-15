import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadCore } from '../src/domain/core.js';
import { defaultSpace, readSpace, SPACE_KIND } from '../src/domain/space.js';
import { SITUATIONS, situationSpace } from '../src/domain/situations.js';
import { channelJobs } from '../src/domain/near-filter.js';
import { RIG_KINDS } from '../src/domain/rigs.js';
import { OBJECT_KINDS } from '../src/domain/objects.js';

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

const flatJs = jobs => [jobs.length, ...jobs.flatMap(job => [job.channels, job.variants.length, ...job.variants.flatMap(v => [v.parts, v.settings.length, ...v.settings, v.surfaces.length, ...v.surfaces])])];

const object = (kind, extra = {}) => ({ kind, id: kind, name: kind, ...extra });

function documents() {
  const list = [];
  const base = defaultSpace();
  list.push(['default', base]);
  for (const id of Object.keys(SITUATIONS)) list.push([`situation ${id}`, situationSpace(id, defaultSpace())]);
  for (const kind of Object.keys(RIG_KINDS)) {
    list.push([`rig ${kind}`, readSpace({ ...base, microphones: [], rigs: [{ kind, id: 'r', position: [0.2, -0.5, 1.1], aim: { azimuthDegrees: 30, elevationDegrees: -10 }, head: 'sphere' }] })]);
  }
  list.push(['measured head over waves', readSpace({ ...situationSpace('waves', defaultSpace()), microphones: [], rigs: [{ kind: 'binaural', id: 'h', position: [0, 0, 0.4] }] })]);
  list.push(['xy over relief', readSpace({ ...situationSpace('corrugated', defaultSpace()), microphones: [], rigs: [{ kind: 'xy', id: 'x', position: [0, -0.7, 1.2] }] })]);
  for (const kind of OBJECT_KINDS) {
    for (const axis of ['up', 'across']) {
      list.push([`bent ${kind} ${axis}`, readSpace({
        kind: SPACE_KIND,
        objects: [object(kind, { position: [0.3, 0.1, 0.4], yawDegrees: 35, tiltDegrees: 20, size: [1.2, 0.8, 0.9], material: 'grass', bend: { angleDegrees: 70, axis }, relief: { shape: 'waves', heightMetres: 0.01, periodMetres: 0.3, directionDegrees: 40, roughMetres: 0.002 } })],
        microphones: [
          { id: 'a', reference: kind, offset: [0.1, 0.2, 0.05], pattern: 'cardioid', aim: { azimuthDegrees: 200, elevationDegrees: 15 } },
          { id: 'b', position: [0.5, 0.6, 1], pattern: 'figure8', pan: 0.3 }
        ],
        source: { kind: 'point', reference: kind, offset: [0, 1, 0.2], distance: false },
        arrival: { azimuthDegrees: 10, elevationDegrees: 30 }
      })]);
    }
  }
  list.push(['inside a pipe mouth', readSpace({
    kind: SPACE_KIND,
    objects: [object('pipe', { position: [0, 0, 0.5], size: [0.3, 2, 0.3], yawDegrees: 90, material: 'soil', bend: { angleDegrees: 120, axis: 'across' }, relief: { shape: 'arches', heightMetres: 0.01, periodMetres: 0.1, directionDegrees: 90 } }), object('plane', { position: [0, 0, 0], size: [6, 0, 6], tiltDegrees: 90 })],
    microphones: [{ id: 'm', reference: 'pipe', offset: [0, -0.1, 0] }, { id: 'n', position: [0.9, 0, 0.5] }, { id: 'o', position: [1.2, 0, 0.5], on: false }]
  })]);
  list.push(['grating and opening', readSpace({
    kind: SPACE_KIND,
    air: { temperatureCelsius: -12 },
    render: { sampleRateHz: 44100, lengthMilliseconds: 120, order: 4, quality: { reliefDetail: 3, elementLimit: 20000, movingDetail: 0.3 } },
    objects: [object('grating', { size: [1, 0, 1], pattern: { periodMetres: [0.05, 0.02], barMetres: [0.01, 0.03], thicknessMetres: 0.01 } }), object('wall', { position: [0, -1, 1], size: [3, 0, 2], opening: { shape: 'rectangle', sizeMetres: [5, 0.5], offsetMetres: [3, -2] } }), object('recess'), object('box', { strength: 0.4 }), object('corner', { material: 'metal' })],
    microphones: [{ id: 'm', position: [0, 0.2, 0.3] }, { id: 'n', position: [0, 0.2, 0.3], pattern: 'figure8' }]
  })]);
  return list;
}

function messy() {
  return [
    { kind: SPACE_KIND },
    { kind: SPACE_KIND, extra: 1, render: { order: 40, sampleRateHz: 1 }, objects: [{ kind: 'box', strength: -3, material: 'cheese', size: [1, 'x', 900] }] },
    { kind: SPACE_KIND, objects: [{ kind: 'plane', tiltDegrees: 270, yawDegrees: -30 }, { kind: 'plane', tiltDegrees: -200 }, { kind: 'pipe', size: [0.5, 3] }, { kind: 'wall', opening: null }, { kind: 'grating', pattern: 3 }] },
    { kind: SPACE_KIND, microphone: { position: [1, 2, 3], pattern: 'nope', aim: { elevationDegrees: 400 } } },
    { kind: SPACE_KIND, microphones: Array.from({ length: 12 }, (_, i) => ({ id: 'same', name: `  M${i}  `, pan: i })), rigs: [{ kind: 'binaural', head: 'x' }, { kind: 'zz' }, {}, {}, {}, {}] },
    { kind: SPACE_KIND, source: { kind: 'point', reference: 'missing', offset: [1, 1, 1], position: [100, -100, 'a'] }, arrival: { azimuthDegrees: -725 } }
  ];
}

const withoutIds = space => JSON.parse(JSON.stringify(space, (key, value) => (key === 'id' ? undefined : value)));

test('the Rust reader reads a space as the JS reader does', () => {
  for (const [name, space] of documents()) {
    close(JSON.parse(core.readSpace(JSON.stringify(space))), JSON.parse(JSON.stringify(space)), name);
  }
  messy().forEach((document, i) => {
    close(withoutIds(JSON.parse(core.readSpace(JSON.stringify(document)))), withoutIds(readSpace(document)), `messy ${i}`);
  });
});

test('the Rust reader refuses what the JS reader refuses', () => {
  for (const text of ['{"kind":"rumbletect.scene"}', '{"kind":"neartect.space","objects":{}}', '[]', 'not json']) {
    assert.throws(() => core.readSpace(text), /not a Neartect space|not a list/);
  }
});

test('the Rust jobs match the JS jobs', () => {
  for (const [name, space] of documents()) {
    close(Array.from(core.nearJobs(JSON.stringify(space))), flatJs(channelJobs(space)), name);
  }
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
