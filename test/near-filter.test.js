import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nearFilter, packSpace, listeners, alignment, SETTING_NAMES } from '../src/domain/near-filter.js';
import { loadCore } from '../src/domain/core.js';
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

test('a microphone inside a pipe hears the pipe resonate', async () => {
  const { magnitudeResponse } = await import('../src/domain/response.js');
  const pipe = { kind: 'pipe', position: [0, 0, 1], size: [0.1, 1, 0.1], material: 'concrete' };
  const space = readSpace({ kind: 'neartect.space', arrival: { azimuthDegrees: 90 }, render: { lengthMilliseconds: 200, order: 1 }, objects: [pipe], microphone: { position: [0, 0, 1] } });
  const { samples, sampleRateHz } = await nearFilter(space);
  const { frequencies, decibels } = magnitudeResponse(samples, sampleRateHz);
  let peak = 0;
  for (let k = 1; k < frequencies.length; k += 1) if (frequencies[k] > 100 && frequencies[k] < 250 && decibels[k] > decibels[peak]) peak = k;
  assert.ok(Math.abs(frequencies[peak] / (343.2 / (2 * (1 + 2 * 0.6133 * 0.05))) - 1) < 0.05, `peak ${frequencies[peak]}`);
});

test('two microphones keep the time and level difference from a point source', async () => {
  const space = readSpace({
    kind: 'neartect.space',
    render: { lengthMilliseconds: 20, sampleRateHz: 48000 },
    source: { kind: 'point', position: [0, 0, 1], distance: false },
    microphones: [{ position: [1, 0, 1] }, { position: [2, 0, 1] }]
  });
  const { channels, sampleRateHz } = await nearFilter(space);
  assert.equal(channels.length, 2);
  const peak = channel => channel.reduce((best, x, i) => (Math.abs(x) > Math.abs(channel[best]) ? i : best), 0);
  assert.equal(peak(channels[0]), 0);
  const c = 331.3 * Math.sqrt(1 + 20 / 273.15);
  assert.ok(Math.abs(peak(channels[1]) - sampleRateHz / c) <= 1, `delay ${peak(channels[1])}`);
  assert.ok(Math.abs(Math.max(...channels[1].map(Math.abs)) - 0.5) < 0.05);
});

test('a point source with distance on is levelled to one metre', async () => {
  const space = readSpace({ kind: 'neartect.space', render: { lengthMilliseconds: 20 }, source: { kind: 'point', position: [0, 0, 1] }, microphones: [{ position: [0, 2, 1] }] });
  const { samples } = await nearFilter(space);
  assert.ok(Math.abs(samples[0] - 0.5) < 1e-6);
});

test('a point source over a large floor matches its image source', async () => {
  const space = readSpace({
    kind: 'neartect.space',
    render: { lengthMilliseconds: 80, order: 1 },
    objects: [{ kind: 'plane', position: [0, 0, 0], size: [40, 0, 40], tiltDegrees: 90, material: 'rigid' }],
    source: { kind: 'point', position: [0, 0, 1], distance: false },
    microphones: [{ position: [1, 0, 0.5] }]
  });
  const { samples, sampleRateHz } = await nearFilter(space);
  const direct = Math.hypot(1, 0.5);
  const image = Math.hypot(1, 1.5);
  const c = 331.3 * Math.sqrt(1 + 20 / 273.15);
  for (const hz of [60, 250, 500]) {
    const phase = 2 * Math.PI * hz * (image - direct) / c;
    const exact = Math.hypot(1 + direct / image * Math.cos(phase), direct / image * Math.sin(phase));
    assert.ok(Math.abs(magnitudeAt(samples, sampleRateHz, hz) - exact) < 0.03, `${hz} Hz`);
  }
});

test('an ORTF pair is 17 cm apart and its capsules turn 55° from its front', async () => {
  const { rigMembers } = await import('../src/domain/rigs.js');
  const space = readSpace({ kind: 'neartect.space', rigs: [{ kind: 'ortf', position: [0, 0, 1] }] });
  const [left, right] = rigMembers(space, space.rigs[0]);
  assert.ok(Math.abs(Math.hypot(...left.position.map((v, i) => v - right.position[i])) - 0.17) < 1e-9);
  const angle = Math.acos(left.facing[0] * right.facing[0] + left.facing[1] * right.facing[1] + left.facing[2] * right.facing[2]) * 180 / Math.PI;
  assert.ok(Math.abs(angle - 110) < 1e-6);
});

test('a head hears a source on its right earlier and louder in the right ear', async () => {
  const space = readSpace({
    kind: 'neartect.space',
    render: { lengthMilliseconds: 20 },
    microphones: [{ position: [0, 0, 1] }],
    arrival: { azimuthDegrees: 90, elevationDegrees: 0 },
    rigs: [{ kind: 'binaural', position: [0, 0, 1], aim: { azimuthDegrees: 0 } }]
  });
  const { channels, names } = await nearFilter(space);
  assert.deepEqual(names.slice(1), ['Head L', 'Head R']);
  const peak = channel => channel.reduce((best, x, i) => (Math.abs(x) > Math.abs(channel[best]) ? i : best), 0);
  assert.ok(peak(channels[2]) < peak(channels[1]));
  const energy = channel => channel.reduce((sum, x) => sum + x * x, 0);
  assert.ok(energy(channels[2]) > energy(channels[1]));
});

test('a mid-side pair decodes to a left and right that favour the source side', async () => {
  const { stereo } = await import('../src/audio/preview.js');
  const space = readSpace({
    kind: 'neartect.space',
    render: { lengthMilliseconds: 20 },
    arrival: { azimuthDegrees: 300, elevationDegrees: 0 },
    rigs: [{ kind: 'ms', position: [0, 0, 1], aim: { azimuthDegrees: 0 } }]
  });
  const { channels, pans } = await nearFilter(space);
  const [left, right] = stereo(channels, pans);
  const energy = channel => channel.reduce((sum, x) => sum + x * x, 0);
  assert.ok(energy(left) > 2 * energy(right));
});

test('a corrugated wall changes the filter, and moving its relief changes it again', async () => {
  const wall = relief => ({ kind: 'plane', position: [0, 1, 1], size: [3, 0, 2], material: 'rigid', relief });
  const space = relief => readSpace({ kind: 'neartect.space', render: { lengthMilliseconds: 20, order: 1 }, microphones: [{ position: [0, 0.6, 1] }], objects: [wall(relief)] });
  const flat = (await nearFilter(space({ shape: 'flat' }))).samples;
  const arches = (await nearFilter(space({ shape: 'arches', heightMetres: 0.03, periodMetres: 0.12 }))).samples;
  const moved = (await nearFilter(space({ shape: 'arches', heightMetres: 0.03, periodMetres: 0.12, offsetMetres: 0.04 }))).samples;
  const difference = (a, b) => a.reduce((sum, x, i) => sum + Math.abs(x - b[i]), 0);
  assert.ok(difference(flat, arches) > 0.05);
  assert.ok(difference(arches, moved) > 0.01);
  assert.ok(arches.every(Number.isFinite));
});

test('a measured head and a spherical head both hear a source on the left louder in the left ear', async () => {
  for (const head of ['measured', 'sphere']) {
    const space = readSpace({
      kind: 'neartect.space', microphones: [],
      rigs: [{ kind: 'binaural', head, position: [0, 0, 1.5], aim: { azimuthDegrees: 0, elevationDegrees: 0 } }],
      arrival: { azimuthDegrees: 270, elevationDegrees: 0 }, render: { lengthMilliseconds: 20 }
    });
    const { channels } = await nearFilter(space);
    const energy = c => c.reduce((sum, x) => sum + x * x, 0);
    assert.ok(energy(channels[0]) > 2 * energy(channels[1]), head);
  }
});

test('the settings record names every field the core reads, in its order', async () => {
  const core = await loadCore();
  assert.deepEqual(core.settingNames(), [...SETTING_NAMES]);
  const space = defaultSpace();
  const list = listeners(space);
  const { settings } = packSpace(space, list[0], alignment(space, list.map(l => l.position))[0], 0);
  assert.equal(settings.length, SETTING_NAMES.length);
  assert.ok(settings.every(Number.isFinite));
});
