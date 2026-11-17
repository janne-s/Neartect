import { core } from './core-ready.js';
import { DEFAULT_MATERIAL } from './materials.js';
import { OBJECT_DEFAULTS, placePoints, referTo } from './objects.js';
import { fillSourceSlots } from './sources.js';

export const SPACE_KIND = 'neartect.space';
export const SPACE_VERSION = 1;
export const OPENING_SHAPES = Object.freeze(['round', 'rectangle']);
export const SOURCE_KINDS = Object.freeze(['far', 'point']);
export const MOST_MICROPHONES = 8;
export const MOST_RIGS = 4;
export const OBJECT_SLOTS = 16;
export const PATTERNS = Object.freeze({ omni: 1, wide: 0.75, cardioid: 0.5, supercardioid: 0.37, hypercardioid: 0.25, figure8: 0 });

export const RANGES = Object.freeze({
  positionMetres: [-50, 50],
  sizeMetres: [0.01, 50],
  yawDegrees: [0, 360],
  tiltDegrees: [-180, 180],
  bendDegrees: [-180, 180],
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
  speedMetresPerSecond: [-5, 5],
  reliefDetail: [1, 24],
  elementLimit: [1000, 5000000],
  movingDetail: [0.1, 1],
  pan: [-1, 1],
  levelDb: [-36, 12]
});

export const DEFAULT_QUALITY = Object.freeze({ reliefDetail: 6, elementLimit: 400000, movingDetail: 0.5 });

export class SpaceError extends Error {}

const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value);

export const signedTurn = degrees => degrees - 360 * Math.round(degrees / 360);

let nextId = 1;
const freshId = () => `o${Date.now().toString(36)}${(nextId += 1).toString(36)}`;

const clone = value => JSON.parse(JSON.stringify(value));

function readAlone(list, item) {
  const [read] = readSpace({ kind: SPACE_KIND, objects: [], [list]: [item] })[list];
  delete read.slot;
  return read;
}

function keepBinding(point, overrides) {
  if (typeof overrides.reference !== 'string') return point;
  return { ...point, reference: overrides.reference, offset: Array.isArray(overrides.offset) ? [...overrides.offset] : point.offset };
}

export function defaultObject(kind, overrides = {}) {
  const base = OBJECT_DEFAULTS[kind] ?? OBJECT_DEFAULTS.plane;
  return readAlone('objects', { id: freshId(), kind, name: kind[0].toUpperCase() + kind.slice(1), material: DEFAULT_MATERIAL, ...clone(base), ...overrides });
}

export function defaultMicrophone(index, overrides = {}) {
  return keepBinding(readAlone('microphones', { id: freshId(), name: `Mic ${index + 1}`, ...overrides }), overrides);
}

export function defaultSource(slot, overrides = {}) {
  return { ...keepBinding(readAlone('moreSources', { id: freshId(), name: `Source ${slot}`, ...overrides }), overrides), slot };
}

export function defaultRig(kind, overrides = {}) {
  return keepBinding(readAlone('rigs', { id: freshId(), kind, ...overrides }), overrides);
}

function fill(list, most, none) {
  const taken = new Set(list.map(item => item.slot).filter(Number.isInteger));
  for (const item of list) {
    if (item.slot !== undefined) continue;
    let slot = 1;
    while (taken.has(slot)) slot += 1;
    item.slot = slot <= most ? slot : none;
    if (slot <= most) taken.add(slot);
  }
}

export function fillSlots(space) {
  fill(space.objects, OBJECT_SLOTS, null);
  fill(space.microphones, MOST_MICROPHONES, 0);
  fill(space.rigs, MOST_RIGS, 0);
  fillSourceSlots(space);
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
  if (!isRecord(value)) throw new SpaceError('This is not a Neartect space.');
  let space;
  try {
    space = JSON.parse(core.readSpace(JSON.stringify(value)));
  } catch (error) {
    throw new SpaceError(error.message);
  }
  if (referenceIndex !== null && space.objects[referenceIndex]) {
    [...space.microphones, ...space.rigs].forEach(m => {
      m.reference = null;
      referTo(space, m, space.objects[referenceIndex].id);
    });
    placePoints(space);
  }
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
