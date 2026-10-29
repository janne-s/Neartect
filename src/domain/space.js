import { DEFAULT_MATERIAL, MATERIALS } from './materials.js';
import { microphonePosition, OBJECT_DEFAULTS, OBJECT_KINDS, referTo } from './objects.js';

export const SPACE_KIND = 'neartect.space';
export const SPACE_VERSION = 1;
export const OPENING_SHAPES = Object.freeze(['round', 'rectangle']);

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
  thicknessMetres: [0, 0.2]
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
  if (kind === 'grating') object.pattern = readPattern(o.pattern ?? defaults.pattern);
  if (kind === 'wall') object.opening = readOpening(o.opening ?? defaults.opening, size);
  return object;
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
    microphone: { position: [0, -0.7, 1.2] },
    arrival: { azimuthDegrees: 0, elevationDegrees: 10 }
  }, 1);
}

export function readSpace(value, referenceIndex = null) {
  if (!isRecord(value) || value.kind !== SPACE_KIND) throw new SpaceError('This is not a Neartect space.');
  if (value.objects !== undefined && !Array.isArray(value.objects)) throw new SpaceError('The objects are not a list.');
  const air = isRecord(value.air) ? value.air : {};
  const microphone = isRecord(value.microphone) ? value.microphone : {};
  const arrival = isRecord(value.arrival) ? value.arrival : {};
  const render = isRecord(value.render) ? value.render : {};
  const space = {
    kind: SPACE_KIND,
    version: SPACE_VERSION,
    air: { temperatureCelsius: number(air.temperatureCelsius, RANGES.temperatureCelsius, 20) },
    microphone: {
      position: position(microphone.position, [0, 0, 1]),
      reference: typeof microphone.reference === 'string' ? microphone.reference : null,
      offset: position(microphone.offset, [0, 0, 0])
    },
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
  if (referenceIndex !== null && space.objects[referenceIndex]) {
    space.microphone.reference = null;
    referTo(space, space.objects[referenceIndex].id);
  }
  if (!space.objects.some(o => o.id === space.microphone.reference)) {
    space.microphone.reference = null;
    space.microphone.offset = [0, 0, 0];
  }
  space.microphone.position = microphonePosition(space);
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
