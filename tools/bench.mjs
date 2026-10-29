import fs from 'node:fs';
import init from '../src/core/neartect_core.js';
import { loadCore } from '../src/domain/core.js';
import { listeners, nearFilter } from '../src/domain/near-filter.js';
import { SITUATIONS, situationSpace } from '../src/domain/situations.js';
import { defaultRig, defaultSpace, readSpace } from '../src/domain/space.js';

const USAGE = 'node tools/bench.mjs [--only name,...] [--save file] [--compare file]';
const options = { only: null, save: null, compare: null };
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i].replace(/^--/, '');
  if (!(key in options)) throw new Error(USAGE);
  options[key] = process.argv[i + 1];
}

const LISTENERS = {
  omni: space => space,
  cardioid: space => ({ ...space, microphones: space.microphones.map(m => ({ ...m, pattern: 'cardioid' })) }),
  xy: space => ({ ...space, microphones: [], rigs: [defaultRig('xy', { position: space.microphones[0].position })] }),
  sphere: space => ({ ...space, microphones: [], rigs: [defaultRig('binaural', { position: space.microphones[0].position, head: 'sphere' })] }),
  ku100: space => ({ ...space, microphones: [], rigs: [defaultRig('binaural', { position: space.microphones[0].position, head: 'measured' })] })
};

const box = (x, y) => ({ kind: 'box', name: 'Box', position: [x, y, 0.3], size: [0.4, 0.4, 0.6], material: 'plaster' });
const clutter = order => readSpace({
  kind: 'neartect.space',
  render: { order },
  arrival: { azimuthDegrees: 45, elevationDegrees: 20 },
  objects: [
    { kind: 'plane', name: 'Floor', position: [0, 0, 0], size: [6, 0, 6], tiltDegrees: 90, material: 'concrete' },
    { kind: 'corner', name: 'Corner', position: [-1.5, -1.5, 0], size: [3, 3, 2.5], material: 'plaster' },
    box(0.8, 0.5), box(-0.6, 0.9), box(0.4, -0.8)
  ],
  microphones: [{ position: [0.1, 0.1, 1.0] }]
});
const corner = situationSpace('corner', defaultSpace());
const long = (milliseconds, rate) => readSpace(LISTENERS.ku100({ ...corner, render: { ...corner.render, lengthMilliseconds: milliseconds, sampleRateHz: rate } }));

const cases = [
  ...Object.keys(SITUATIONS).flatMap(id => Object.entries(LISTENERS).map(([kind, make]) => [`${id} ${kind}`, () => readSpace(make(situationSpace(id, defaultSpace())))])),
  ...[4, 5, 6].map(order => [`clutter order ${order}`, () => clutter(order)]),
  ['corner ku100 1 s 48 kHz', () => long(1000, 48000)],
  ['corner ku100 1 s 192 kHz', () => long(1000, 192000)]
].filter(([name]) => !options.only || options.only.split(',').some(part => name.includes(part)));

const fixedIds = space => readSpace({ ...space, objects: space.objects.map((object, index) => ({ ...object, id: `object-${index}` })) });

await loadCore();
const wasm = await init();
const reference = options.compare ? JSON.parse(fs.readFileSync(options.compare, 'utf8')) : null;
const saved = {};

for (const [name, make] of cases) {
  const space = fixedIds(make());
  const count = listeners(space).length;
  const started = performance.now();
  const { channels } = await nearFilter(space);
  const milliseconds = performance.now() - started;
  saved[name] = channels.map(channel => Array.from(channel));
  const memory = wasm.memory.buffer.byteLength / 1e6;
  let difference = '';
  if (reference?.[name]) {
    let worst = 0;
    let peak = 1e-30;
    reference[name].forEach((channel, c) => channel.forEach((x, i) => {
      worst = Math.max(worst, Math.abs(x - (channels[c]?.[i] ?? NaN)));
      peak = Math.max(peak, Math.abs(x));
    }));
    difference = `  difference ${(worst / peak).toExponential(1)}`;
  }
  console.log(`${name.padEnd(28)} ${milliseconds.toFixed(0).padStart(7)} ms  ${count} ch  memory ${memory.toFixed(0).padStart(5)} MB${difference}`);
}

if (options.save) fs.writeFileSync(options.save, JSON.stringify(saved));
