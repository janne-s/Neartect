import { loadCore, runJob } from './core.js';
import { core } from './core-ready.js';
import { aimVector, pointPosition } from './objects.js';
import { rigMembers } from './rigs.js';

export const SETTING_NAMES = Object.freeze(core.settingNames());

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

export function channelJobs(space) {
  const flat = core.nearJobs(JSON.stringify(space));
  let at = 0;
  const next = () => flat[at++];
  const take = () => {
    const length = next();
    at += length;
    return flat.slice(at - length, at);
  };
  return Array.from({ length: next() }, () => {
    const channels = next();
    const variants = Array.from({ length: next() }, () => {
      const parts = next();
      const settings = take();
      return { settings, surfaces: take(), parts };
    });
    return { variants, channels };
  });
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

export function sourceSpaces(space) {
  return JSON.parse(core.sourceSpaces(JSON.stringify(space)));
}

export function withSources(sampleRateHz, sources) {
  const first = sources[0] ?? { channels: [], names: [], pans: [] };
  return { sampleRateHz, sources, channels: first.channels, names: first.names, pans: first.pans };
}

export async function computeFilter(space, run) {
  const sources = await Promise.all(sourceSpaces(space).map(async ({ number, name, input, space: alone }) => ({ number, name, input, ...await computeSource(alone, run) })));
  return withSources(space.render.sampleRateHz, sources);
}

async function computeSource(space, run) {
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
  return { channels: jobs.flat(), names: list.map(l => l.name), pans: list.map(l => l.pan) };
}

export async function nearFilter(space) {
  const core = await loadCore();
  const result = await computeFilter(space, async job => runJob(core, job));
  return { ...result, samples: result.channels[0] };
}
