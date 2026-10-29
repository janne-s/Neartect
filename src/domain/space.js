import { DEFAULT_MATERIAL, MATERIALS } from './materials.js';
import { FLAT_RELIEF, OBJECT_DEFAULTS, OBJECT_KINDS, placePoints, referTo, RELIEF_SHAPES } from './objects.js';
import { RIG_ANGLE_DEGREES, RIG_KINDS, RIG_SPACING_METRES } from './rigs.js';

export const SPACE_KIND = 'neartect.space';
export const SPACE_VERSION = 1;
export const OPENING_SHAPES = Object.freeze(['round', 'rectangle']);
export const SOURCE_KINDS = Object.freeze(['far', 'point']);
export const MOST_MICROPHONES = 8;
export const MOST_RIGS = 4;
export const PATTERNS = Object.freeze({ omni: 1, wide: 0.75, cardioid: 0.5, supercardioid: 0.37, hypercardioid: 0.25, figure8: 0 });

export const RANGES = Object.freeze({
  positionMetres: [-50, 50],
  sizeMetres: [0.01, 50],
  yawDegrees: [0, 360],
  tiltDegrees: [-90, 90],
  strength: [0, 2],
  azimuthDegrees: [0, 360],
  elevationDegrees: [-90, 90],
  temperatureCelsius: [-40, 50],
  sampleRateHz: [8000, 192000],
  lengthMilliseconds: [10, 1000],
  order: [1, 6],
  periodMetres: [0, 1],
  thicknessMetres: [0, 0.2],
  reliefHeightMetres: [0, 0.5],
  reliefPeriodMetres: [0.01, 5],
  roughMetres: [0, 0.05],
  grainMetres: [0.01, 1],
  speedMetresPerSecond: [-5, 5]
});

export class SpaceError extends Error {}

const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const clamp = (value, [lo, hi]) => Math.min(hi, Math.max(lo, value));

function number(value, range, fallback) {
  const n = typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  return clamp(n, range);
}

function angle(value, fallback) {
  const n = typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  return ((n % 360) + 360) % 360;
}

function position(value, fallback) {
  const list = Array.isArray(value) && value.length === 3 ? value : fallback;
  return list.map((v, i) => number(v, RANGES.positionMetres, fallback[i]));
}

function choice(value, options, fallback) {
  return options.includes(value) ? value : fallback;
}

let nextId = 1;
const freshId = () => `o${Date.now().toString(36)}${(nextId += 1).toString(36)}`;

export function defaultObject(kind, overrides = {}) {
  const base = OBJECT_DEFAULTS[kind] ?? OBJECT_DEFAULTS.plane;
  return readObject({ kind, name: kind[0].toUpperCase() + kind.slice(1), material: DEFAULT_MATERIAL, ...clone(base), ...overrides });
}

const clone = value => JSON.parse(JSON.stringify(value));

function readObject(value, index = 0) {
  const o = isRecord(value) ? value : {};
  const kind = choice(o.kind, OBJECT_KINDS, 'plane');
  const defaults = OBJECT_DEFAULTS[kind];
  const size = (Array.isArray(o.size) && o.size.length === 3 ? o.size : defaults.size)
    .map((v, i) => (defaults.size[i] === 0 ? 0 : number(v, RANGES.sizeMetres, defaults.size[i])));
  const object = {
    id: typeof o.id === 'string' && o.id ? o.id.slice(0, 40) : freshId(),
    kind,
    name: typeof o.name === 'string' && o.name.trim() ? o.name.trim().slice(0, 60) : `Object ${index + 1}`,
    position: position(o.position, [0, 0, 1]),
    yawDegrees: angle(o.yawDegrees, 0),
    tiltDegrees: number(o.tiltDegrees, RANGES.tiltDegrees, 0),
    size,
    material: Object.hasOwn(MATERIALS, o.material) ? o.material : DEFAULT_MATERIAL,
    strength: number(o.strength, RANGES.strength, 1)
  };
  if (kind === 'pipe') object.size[2] = object.size[0];
  object.relief = readRelief(o.relief);
  if (kind === 'grating') object.pattern = readPattern(o.pattern ?? defaults.pattern);
  if (kind === 'wall') object.opening = readOpening(o.opening ?? defaults.opening, size);
  return object;
}

function readPoint(value, fallbackPosition) {
  const p = isRecord(value) ? value : {};
  return {
    position: position(p.position, fallbackPosition),
    reference: typeof p.reference === 'string' ? p.reference : null,
    offset: position(p.offset, [0, 0, 0])
  };
}

function readMicrophone(value, index) {
  const m = isRecord(value) ? value : {};
  return {
    id: typeof m.id === 'string' && m.id ? m.id.slice(0, 40) : freshId(),
    name: typeof m.name === 'string' && m.name.trim() ? m.name.trim().slice(0, 60) : `Mic ${index + 1}`,
    ...readPoint(m, [0, 0, 1]),
    pattern: Object.hasOwn(PATTERNS, m.pattern) ? m.pattern : 'omni',
    aim: readAim(m.aim)
  };
}

export function defaultMicrophone(index, overrides = {}) {
  return readMicrophone(overrides, index);
}

function readAim(value) {
  return {
    azimuthDegrees: angle(value?.azimuthDegrees, 0),
    elevationDegrees: number(value?.elevationDegrees, RANGES.elevationDegrees, 0)
  };
}

function readRig(value) {
  const r = isRecord(value) ? value : {};
  const kind = Object.hasOwn(RIG_KINDS, r.kind) ? r.kind : 'xy';
  const defaults = RIG_KINDS[kind];
  return {
    id: typeof r.id === 'string' && r.id ? r.id.slice(0, 40) : freshId(),
    kind,
    name: typeof r.name === 'string' && r.name.trim() ? r.name.trim().slice(0, 60) : defaults.label,
    ...readPoint(r, [0, 0, 1]),
    aim: readAim(r.aim),
    spacingMetres: number(r.spacingMetres, RIG_SPACING_METRES, defaults.spacingMetres),
    angleDegrees: number(r.angleDegrees, RIG_ANGLE_DEGREES, defaults.angleDegrees ?? 0)
  };
}

export function defaultRig(kind, overrides = {}) {
  return readRig({ kind, ...overrides });
}

function readSource(value) {
  const s = isRecord(value) ? value : {};
  return {
    kind: choice(s.kind, SOURCE_KINDS, 'far'),
    ...readPoint(s, [0, 2, 1.5]),
    distance: typeof s.distance === 'boolean' ? s.distance : true
  };
}

function readRelief(value) {
  const r = isRecord(value) ? value : {};
  const d = FLAT_RELIEF;
  return {
    shape: choice(r.shape, RELIEF_SHAPES, 'flat'),
    heightMetres: number(r.heightMetres, RANGES.reliefHeightMetres, d.heightMetres),
    periodMetres: number(r.periodMetres, RANGES.reliefPeriodMetres, d.periodMetres),
    directionDegrees: angle(r.directionDegrees, d.directionDegrees),
    roughMetres: number(r.roughMetres, RANGES.roughMetres, d.roughMetres),
    grainMetres: number(r.grainMetres, RANGES.grainMetres, d.grainMetres),
    offsetMetres: number(r.offsetMetres, RANGES.positionMetres, 0),
    speedMetresPerSecond: number(r.speedMetresPerSecond, RANGES.speedMetresPerSecond, 0)
  };
}

function readOpening(value, size) {
  const o = isRecord(value) ? value : {};
  const sizeMetres = (Array.isArray(o.sizeMetres) && o.sizeMetres.length === 2 ? o.sizeMetres : [0.4, 0.4])
    .map((v, i) => Math.min(number(v, RANGES.sizeMetres, 0.4), size[i * 2]));
  const offsetMetres = (Array.isArray(o.offsetMetres) && o.offsetMetres.length === 2 ? o.offsetMetres : [0, 0])
    .map((v, i) => number(v, RANGES.positionMetres, 0));
  return { shape: choice(o.shape, OPENING_SHAPES, 'round'), sizeMetres, offsetMetres };
}

function readPattern(value) {
  const p = isRecord(value) ? value : {};
  const pair = (list, fallback) => (Array.isArray(list) && list.length === 2 ? list : fallback)
    .map((v, i) => number(v, RANGES.periodMetres, fallback[i]));
  const periodMetres = pair(p.periodMetres, [0.034, 0.034]);
  const barMetres = pair(p.barMetres, [0.004, 0.004]).map((bar, i) => Math.min(bar, periodMetres[i]));
  return { kind: 'grid', periodMetres, barMetres, thicknessMetres: number(p.thicknessMetres, RANGES.thicknessMetres, 0.03) };
}

export function defaultSpace() {
  return readSpace({
    kind: SPACE_KIND,
    objects: [
      { kind: 'plane', name: 'Floor', position: [0, 1, 0], size: [4, 0, 4], tiltDegrees: 90, material: 'concrete' },
      { kind: 'plane', name: 'Wall', position: [0, -1, 1.25], size: [4, 0, 2.5], material: 'brick' }
    ],
    microphones: [{ position: [0, -0.7, 1.2] }],
    arrival: { azimuthDegrees: 0, elevationDegrees: 10 }
  }, 1);
}

export function readSpace(value, referenceIndex = null) {
  if (!isRecord(value) || value.kind !== SPACE_KIND) throw new SpaceError('This is not a Neartect space.');
  if (value.objects !== undefined && !Array.isArray(value.objects)) throw new SpaceError('The objects are not a list.');
  const air = isRecord(value.air) ? value.air : {};
  const microphones = Array.isArray(value.microphones) && value.microphones.length
    ? value.microphones.slice(0, MOST_MICROPHONES)
    : [isRecord(value.microphone) ? value.microphone : {}];
  const arrival = isRecord(value.arrival) ? value.arrival : {};
  const render = isRecord(value.render) ? value.render : {};
  const space = {
    kind: SPACE_KIND,
    version: SPACE_VERSION,
    air: { temperatureCelsius: number(air.temperatureCelsius, RANGES.temperatureCelsius, 20) },
    microphones: microphones.map(readMicrophone),
    rigs: (Array.isArray(value.rigs) ? value.rigs.slice(0, MOST_RIGS) : []).map(readRig),
    source: readSource(value.source),
    arrival: {
      azimuthDegrees: angle(arrival.azimuthDegrees, 0),
      elevationDegrees: number(arrival.elevationDegrees, RANGES.elevationDegrees, 0)
    },
    render: {
      sampleRateHz: Math.round(number(render.sampleRateHz, RANGES.sampleRateHz, 48000)),
      lengthMilliseconds: number(render.lengthMilliseconds, RANGES.lengthMilliseconds, 80),
      order: Math.round(number(render.order, RANGES.order, 3))
    },
    objects: (value.objects ?? []).map(readObject)
  };
  const ids = new Set();
  [...space.microphones, ...space.rigs].forEach(m => {
    if (ids.has(m.id)) m.id = freshId();
    ids.add(m.id);
  });
  for (const point of [...space.microphones, ...space.rigs, space.source]) {
    if (!space.objects.some(o => o.id === point.reference)) {
      point.reference = null;
      point.offset = [0, 0, 0];
    }
  }
  if (referenceIndex !== null && space.objects[referenceIndex]) {
    [...space.microphones, ...space.rigs].forEach(m => {
      m.reference = null;
      referTo(space, m, space.objects[referenceIndex].id);
    });
  }
  placePoints(space);
  return space;
}

export function speedOfSound(temperatureCelsius) {
  return 331.3 * Math.sqrt(1 + temperatureCelsius / 273.15);
}

export function arrivalVector({ azimuthDegrees, elevationDegrees }) {
  const az = azimuthDegrees * Math.PI / 180;
  const el = elevationDegrees * Math.PI / 180;
  return [Math.cos(el) * Math.sin(az), Math.cos(el) * Math.cos(az), Math.sin(el)];
}
