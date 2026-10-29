import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { aimVector, expandObject, pipeAround, pointPosition } from './objects.js';
import { rigMembers } from './rigs.js';
import { arrivalVector, PATTERNS, speedOfSound } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0, grid: 1, opening: 2 });
const DISTANCE_REFERENCE_METRES = 1;
const NEAREST_LEVEL_METRES = 0.1;

// The core's settings record, in its order (`settingNames` in the core).
export const SETTING_NAMES = Object.freeze([
  'sampleRateHz', 'speedOfSoundMetresPerSecond', 'lengthSamples', 'order',
  'microphoneX', 'microphoneY', 'microphoneZ',
  'arrivalX', 'arrivalY', 'arrivalZ',
  'pointSource', 'sourceX', 'sourceY', 'sourceZ',
  'gain', 'delaySeconds',
  'pipeRadiusMetres', 'pipeMouthAX', 'pipeMouthAY', 'pipeMouthAZ', 'pipeMouthBX', 'pipeMouthBY', 'pipeMouthBZ',
  'pattern', 'facingX', 'facingY', 'facingZ',
  'listenerKind', 'headRadiusMetres',
  'reliefDetail', 'elementLimit',
  'headFrontX', 'headFrontY', 'headFrontZ', 'headUpX', 'headUpY', 'headUpZ'
]);

const vector = (name, v) => ({ [`${name}X`]: v[0], [`${name}Y`]: v[1], [`${name}Z`]: v[2] });
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

// Every channel the space produces: each free microphone, then each rig's
// capsules or ears. Free microphones spread from left to right in the
// preview; a lone one plays in both.
export function listeners(space) {
  const count = space.microphones.length;
  const free = space.microphones.map((m, index) => {
    const angle = count === 1 ? Math.PI / 4 : index / (count - 1) * Math.PI / 2;
    return {
      name: m.name,
      position: pointPosition(space, m),
      pattern: m.pattern,
      facing: aimVector(space, m),
      pan: count === 1 ? [1, 1] : [Math.cos(angle), Math.sin(angle)]
    };
  });
  const rigged = (space.rigs ?? []).flatMap(rig => rigMembers(space, rig).map(member => ({ ...member, name: `${rig.name} ${member.name}` })));
  return [...free, ...rigged];
}

// Each channel's filter is relative to the direct sound at that listener;
// alignment puts them back on one clock and one level, so that a pair keeps
// its time and level differences. The earliest listener has no delay; a point
// source with distance on is levelled to 1 m.
export function alignment(space, positions) {
  const c = speedOfSound(space.air.temperatureCelsius);
  if (space.source.kind === 'point') {
    const source = pointPosition(space, space.source);
    const reach = positions.map(p => Math.hypot(...sub(source, p)));
    const nearest = Math.min(...reach);
    const level = space.source.distance ? DISTANCE_REFERENCE_METRES : Math.max(nearest, NEAREST_LEVEL_METRES);
    return reach.map(r => ({ delaySeconds: (r - nearest) / c, gain: level / Math.max(r, NEAREST_LEVEL_METRES) }));
  }
  const arrival = arrivalVector(space.arrival);
  const lead = positions.map(p => dot(arrival, p));
  const first = Math.max(...lead);
  return lead.map(l => ({ delaySeconds: (first - l) / c, gain: 1 }));
}

export function packSpace(space, listener, aligned, longest) {
  const microphone = listener.position;
  const pipe = pipeAround(space, microphone);
  const point = space.source.kind === 'point';
  const rate = space.render.sampleRateHz;
  const values = {
    sampleRateHz: rate,
    speedOfSoundMetresPerSecond: speedOfSound(space.air.temperatureCelsius),
    lengthSamples: Math.round((space.render.lengthMilliseconds / 1000 + longest) * rate),
    order: space.render.order,
    ...vector('microphone', microphone),
    ...vector('arrival', arrivalVector(space.arrival)),
    pointSource: point ? 1 : 0,
    ...vector('source', point ? pointPosition(space, space.source) : [0, 0, 0]),
    gain: aligned.gain,
    delaySeconds: aligned.delaySeconds,
    pipeRadiusMetres: pipe ? pipe.radius : 0,
    ...vector('pipeMouthA', pipe ? pipe.mouthA : [0, 0, 0]),
    ...vector('pipeMouthB', pipe ? pipe.mouthB : [0, 0, 0]),
    pattern: listener.ear ? 1 : PATTERNS[listener.pattern],
    ...vector('facing', listener.ear ? listener.ear.axis : listener.facing),
    listenerKind: listener.ear ? (listener.ear.head === 'measured' ? 2 : 1) : 0,
    headRadiusMetres: listener.ear ? listener.ear.radius : 0,
    reliefDetail: space.render.quality.reliefDetail,
    elementLimit: space.render.quality.elementLimit,
    ...vector('headFront', listener.ear?.front ?? [0, 1, 0]),
    ...vector('headUp', listener.ear?.up ?? [0, 0, 1])
  };
  const settings = Float64Array.from(SETTING_NAMES, name => values[name]);
  const surfaces = space.objects.filter(object => object !== pipe?.object).flatMap(expandObject);
  const records = surfaces.flatMap(surface => {
    const body = [
      surface.strength,
      ...reflectionBands(surface.material),
      PATTERN_CODES[surface.pattern.kind],
      ...surface.pattern.values,
      ...surface.relief,
      surface.vertices.length,
      ...surface.vertices.flat()
    ];
    return [body.length + 1, ...body];
  });
  return { settings, surfaces: new Float64Array(records) };
}

// Channels with relief split their scattering into parts that workers share;
// the cut is fixed by the space, so the filter does not depend on how many
// workers there are. Flat spaces gain less than a part costs, and a part on
// the measured head costs its 17 spectral sets.
const RELIEF_PARTS = 16;
const MEASURED_HEAD_PARTS = 4;

const hasRelief = object => object.strength > 0 && ((object.relief.shape !== 'flat' && object.relief.heightMetres > 0) || object.relief.roughMetres > 0);

export function channelJobs(space) {
  const list = listeners(space);
  const aligned = alignment(space, list.map(l => l.position));
  const longest = Math.max(...aligned.map(a => a.delaySeconds));
  return list.map((listener, index) => {
    const packed = packSpace(space, listener, aligned[index], longest);
    const split = !pipeAround(space, listener.position) && space.objects.some(hasRelief);
    const parts = !split ? 1 : listener.ear?.head === 'measured' ? MEASURED_HEAD_PARTS : RELIEF_PARTS;
    return { ...packed, parts };
  });
}

export function runJob(core, job) {
  return job.spectra
    ? core.nearFilterFromParts(job.settings, job.spectra)
    : core.nearSpectrumPart(job.settings, job.surfaces, job.part, job.parts);
}

// `run` computes one job (a part's spectrum, or a channel's filter from its
// parts' spectra) and may run them anywhere; the parts are summed in order.
export async function computeFilter(space, run) {
  const list = listeners(space);
  const channels = await Promise.all(channelJobs(space).map(async ({ settings, surfaces, parts }) => {
    const spectra = await Promise.all(Array.from({ length: parts }, (_, part) => run({ settings, surfaces, part, parts })));
    const joined = new Float64Array(spectra.reduce((sum, s) => sum + s.length, 0));
    spectra.reduce((at, s) => { joined.set(s, at); return at + s.length; }, 0);
    return run({ settings, spectra: joined });
  }));
  return { sampleRateHz: space.render.sampleRateHz, channels, names: list.map(l => l.name), pans: list.map(l => l.pan) };
}

export async function nearFilter(space) {
  const core = await loadCore();
  const result = await computeFilter(space, async job => runJob(core, job));
  return { ...result, samples: result.channels[0] };
}
