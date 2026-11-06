import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { aimVector, expandObject, pipeAround, pipeShare, pointPosition, RELIEF_SHAPES, seedOf } from './objects.js';
import { bendValues } from './bend.js';
import { faceNormal } from './placement.js';
import { rigMembers } from './rigs.js';
import { arrivalVector, PATTERNS, speedOfSound } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0, grid: 1, opening: 2 });
const DISTANCE_REFERENCE_METRES = 1;
const NEAREST_LEVEL_METRES = 0.1;

// In the core's order (`settingNames`).
export const SETTING_NAMES = Object.freeze([
  'sampleRateHz', 'speedOfSoundMetresPerSecond', 'lengthSamples', 'order',
  'microphoneX', 'microphoneY', 'microphoneZ',
  'arrivalX', 'arrivalY', 'arrivalZ',
  'pointSource', 'sourceX', 'sourceY', 'sourceZ',
  'gain', 'delaySeconds',
  'pipeRadiusMetres', 'pipeMouthAX', 'pipeMouthAY', 'pipeMouthAZ', 'pipeMouthBX', 'pipeMouthBY', 'pipeMouthBZ',
  'pipeBendCentreX', 'pipeBendCentreY', 'pipeBendCentreZ', 'pipeBendAxisX', 'pipeBendAxisY', 'pipeBendAxisZ',
  'pattern', 'facingX', 'facingY', 'facingZ',
  'listenerKind', 'headRadiusMetres',
  'reliefDetail', 'elementLimit',
  'headFrontX', 'headFrontY', 'headFrontZ', 'headUpX', 'headUpY', 'headUpZ',
  'secondPattern', 'secondFacingX', 'secondFacingY', 'secondFacingZ',
  'pipeReflection63Hz', 'pipeReflection125Hz', 'pipeReflection250Hz', 'pipeReflection500Hz',
  'pipeReflection1000Hz', 'pipeReflection2000Hz', 'pipeReflection4000Hz', 'pipeReflection8000Hz',
  'pipeReliefShape', 'pipeReliefHeightMetres', 'pipeReliefPeriodMetres', 'pipeReliefRadians',
  'pipeReliefOffsetMetres', 'pipeRoughMetres', 'pipeGrainMetres', 'pipeReliefSeed'
]);
const PIPE_REFLECTION_NAMES = SETTING_NAMES.filter(name => name.startsWith('pipeReflection'));

const vector = (name, v) => ({ [`${name}X`]: v[0], [`${name}Y`]: v[1], [`${name}Z`]: v[2] });

function pipeWall(object) {
  const bands = reflectionBands(object?.material);
  const r = object?.relief;
  return {
    ...Object.fromEntries(PIPE_REFLECTION_NAMES.map((name, i) => [name, object ? bands[i] : 1])),
    pipeReliefShape: r ? RELIEF_SHAPES.indexOf(r.shape) : 0,
    pipeReliefHeightMetres: r && r.shape !== 'flat' ? r.heightMetres : 0,
    pipeReliefPeriodMetres: r ? r.periodMetres : 1,
    pipeReliefRadians: r ? r.directionDegrees * Math.PI / 180 : 0,
    pipeReliefOffsetMetres: r ? r.offsetMetres : 0,
    pipeRoughMetres: r ? r.roughMetres : 0,
    pipeGrainMetres: r ? r.grainMetres : 1,
    pipeReliefSeed: object ? seedOf(object.id) : 0
  };
}
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

export const hearing = list => list.filter(point => point.on !== false);

export function panOf(space, microphone) {
  if (microphone.pan !== null) return microphone.pan;
  const heard = hearing(space.microphones);
  const index = heard.findIndex(m => m.id === microphone.id);
  return heard.length > 1 && index >= 0 ? 2 * index / (heard.length - 1) - 1 : 0;
}

function panGains(pan, alone) {
  const angle = (pan + 1) * Math.PI / 4;
  const gains = [Math.cos(angle), Math.sin(angle)];
  const loudest = alone ? Math.max(...gains) : 1;
  return gains.map(gain => gain / loudest);
}

export function listeners(space) {
  const microphones = hearing(space.microphones);
  const free = microphones.map(m => ({
    name: m.name,
    position: pointPosition(space, m),
    pattern: m.pattern,
    facing: aimVector(space, m),
    pan: panGains(panOf(space, m), microphones.length === 1)
  }));
  const rigged = hearing(space.rigs ?? []).flatMap(rig => rigMembers(space, rig).map(member => ({ ...member, name: `${rig.name} ${member.name}` })));
  return [...free, ...rigged];
}

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

export function packSpace(space, listener, aligned, longest, second = null, pipe = pipeAround(space, listener.position), share = 1) {
  const microphone = listener.position;
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
    gain: aligned.gain * share,
    delaySeconds: aligned.delaySeconds,
    pipeRadiusMetres: pipe ? pipe.radius : 0,
    ...vector('pipeMouthA', pipe ? pipe.mouthA : [0, 0, 0]),
    ...vector('pipeBendCentre', pipe?.bend ? pipe.bend.centre : [0, 0, 0]),
    ...vector('pipeBendAxis', pipe?.bend ? pipe.bend.axis : [0, 0, 0]),
    ...vector('pipeMouthB', pipe ? pipe.mouthB : [0, 0, 0]),
    pattern: listener.ear ? 1 : PATTERNS[listener.pattern],
    ...vector('facing', listener.ear ? listener.ear.axis : listener.facing),
    listenerKind: second ? 3 : listener.ear ? (listener.ear.head === 'measured' ? 2 : 1) : 0,
    headRadiusMetres: listener.ear ? listener.ear.radius : 0,
    reliefDetail: space.render.quality.reliefDetail,
    elementLimit: space.render.quality.elementLimit,
    ...vector('headFront', listener.ear?.front ?? [0, 1, 0]),
    ...vector('headUp', listener.ear?.up ?? [0, 0, 1]),
    secondPattern: second ? PATTERNS[second.pattern] : 1,
    ...vector('secondFacing', second ? second.facing : [0, 1, 0]),
    ...pipeWall(pipe?.object)
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
      ...bendValues(surface.bend, faceNormal(surface.vertices)),
      surface.vertices.length,
      ...surface.vertices.flat()
    ];
    return [body.length + 1, ...body];
  });
  return { settings, surfaces: new Float64Array(records) };
}

const RELIEF_PARTS = 16;
const MEASURED_HEAD_PARTS = 4;

const hasRelief = object => object.strength > 0 && ((object.relief.shape !== 'flat' && object.relief.heightMetres > 0) || object.relief.roughMetres > 0);

const coincident = (a, b) => b && !a.ear && !b.ear && a.position.every((v, i) => v === b.position[i]);

export function channelJobs(space) {
  const list = listeners(space);
  const aligned = alignment(space, list.map(l => l.position));
  const longest = Math.max(...aligned.map(a => a.delaySeconds));
  const jobs = [];
  for (let index = 0; index < list.length; index += 1) {
    const listener = list[index];
    const second = coincident(listener, list[index + 1]) ? list[index + 1] : null;
    const near = pipeShare(space, listener.position);
    const models = !near || near.share <= 0 ? [[null, 1]] : near.share >= 1 ? [[near.pipe, 1]] : [[near.pipe, near.share], [null, 1 - near.share]];
    const variants = models.map(([pipe, share]) => {
      const packed = packSpace(space, listener, aligned[index], longest, second, pipe, share);
      const split = !pipe && space.objects.some(hasRelief);
      return { ...packed, parts: !split ? 1 : listener.ear?.head === 'measured' ? MEASURED_HEAD_PARTS : RELIEF_PARTS };
    });
    jobs.push({ variants, channels: second ? 2 : 1 });
    if (second) index += 1;
  }
  return jobs;
}

const BYTES_PER_COMPLEX = 16;
const HISTOGRAM_LISTS = 4;
const HORIZON_MARGIN_SAMPLES = 256;
const OVERSAMPLING = 2;

// Measured peaks exceed the arrays by up to 35 %.
const ALLOCATION_SLACK = 1.4;
const BASE_BYTES = 8 * 2 ** 20;

const nextPowerOfTwo = value => 2 ** Math.ceil(Math.log2(Math.max(1, value)));

export function jobBytes({ settings, surfaces, spectra }) {
  if (spectra) return 2 * spectra.byteLength + BASE_BYTES;
  const value = name => settings[SETTING_NAMES.indexOf(name)];
  const length = value('lengthSamples');
  const n = 2 * nextPowerOfTwo(Math.max(length, 16));
  const fine = OVERSAMPLING * n;
  const kind = value('listenerKind');
  const sets = kind === 2 ? 17 : kind === 3 ? 4 : kind === 1 || value('pattern') < 1 ? 2 : 1;
  let count = 0;
  for (let at = 0; at < surfaces.length; at += surfaces[at]) count += 1;
  const pipe = value('pipeRadiusMetres') > 0;
  const pipeMetres = Math.hypot(...['X', 'Y', 'Z'].map(axis => value(`pipeMouthB${axis}`) - value(`pipeMouthA${axis}`))) + value('pipeRadiusMetres');
  const lead = pipe ? Math.ceil(pipeMetres / value('speedOfSoundMetresPerSecond') * value('sampleRateHz')) : 0;
  const horizon = Math.min(fine, OVERSAMPLING * (length + HORIZON_MARGIN_SAMPLES + lead));
  const bins = n / 2 + 1;
  const outside = horizon * sets * HISTOGRAM_LISTS * 8 + (sets * bins + count * n + 3 * fine) * BYTES_PER_COMPLEX;
  return ALLOCATION_SLACK * ((pipe ? 2 : 1) * outside + 2 * bins * BYTES_PER_COMPLEX) + BASE_BYTES;
}

export function runJob(core, job) {
  return job.spectra
    ? core.nearFilterFromParts(job.settings, job.spectra)
    : core.nearSpectrumPart(job.settings, job.surfaces, job.part, job.parts);
}

export async function computeFilter(space, run) {
  const list = listeners(space);
  const jobs = await Promise.all(channelJobs(space).map(async ({ variants, channels: count }) => {
    const settings = variants[0].settings;
    const spectra = await Promise.all(variants.flatMap(({ settings, surfaces, parts }) => Array.from({ length: parts }, (_, part) => run({ settings, surfaces, part, parts }))));
    const joined = new Float64Array(spectra.reduce((sum, s) => sum + s.length, 0));
    spectra.reduce((at, s) => { joined.set(s, at); return at + s.length; }, 0);
    const filters = await run({ settings, spectra: joined });
    const length = filters.length / count;
    return Array.from({ length: count }, (_, channel) => filters.slice(channel * length, (channel + 1) * length));
  }));
  const channels = jobs.flat();
  return { sampleRateHz: space.render.sampleRateHz, channels, names: list.map(l => l.name), pans: list.map(l => l.pan) };
}

export async function nearFilter(space) {
  const core = await loadCore();
  const result = await computeFilter(space, async job => runJob(core, job));
  return { ...result, samples: result.channels[0] };
}
