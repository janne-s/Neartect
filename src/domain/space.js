import { DEFAULT_MATERIAL, MATERIALS } from './materials.js';

export const SPACE_KIND = 'neartect.space';
export const SPACE_VERSION = 1;
export const SURFACE_SHAPES = Object.freeze(['rectangle']);
export const PATTERN_KINDS = Object.freeze(['solid', 'grid']);

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

export function defaultSurface(overrides = {}) {
  return readSurface({
    name: 'Wall',
    shape: 'rectangle',
    centre: [0, 0, 1],
    widthMetres: 2,
    heightMetres: 2,
    yawDegrees: 0,
    tiltDegrees: 0,
    material: DEFAULT_MATERIAL,
    strength: 1,
    pattern: { kind: 'solid' },
    ...overrides
  });
}

function readSurface(value, index = 0) {
  const s = isRecord(value) ? value : {};
  return {
    name: typeof s.name === 'string' && s.name.trim() ? s.name.trim().slice(0, 60) : `Surface ${index + 1}`,
    shape: choice(s.shape, SURFACE_SHAPES, 'rectangle'),
    centre: position(s.centre, [0, 0, 1]),
    widthMetres: number(s.widthMetres, RANGES.sizeMetres, 2),
    heightMetres: number(s.heightMetres, RANGES.sizeMetres, 2),
    yawDegrees: angle(s.yawDegrees, 0),
    tiltDegrees: number(s.tiltDegrees, RANGES.tiltDegrees, 0),
    material: Object.hasOwn(MATERIALS, s.material) ? s.material : DEFAULT_MATERIAL,
    strength: number(s.strength, RANGES.strength, 1),
    pattern: readPattern(s.pattern)
  };
}

function readPattern(value) {
  const p = isRecord(value) ? value : {};
  const kind = choice(p.kind, PATTERN_KINDS, 'solid');
  if (kind === 'solid') return { kind };
  const pair = (list, fallback) => (Array.isArray(list) && list.length === 2 ? list : fallback)
    .map((v, i) => number(v, RANGES.periodMetres, fallback[i]));
  const periodMetres = pair(p.periodMetres, [0.04, 0.04]);
  const barMetres = pair(p.barMetres, [0.008, 0.008]).map((bar, i) => Math.min(bar, periodMetres[i]));
  return { kind, periodMetres, barMetres, thicknessMetres: number(p.thicknessMetres, RANGES.thicknessMetres, 0.003) };
}

export function defaultSpace() {
  return readSpace({
    kind: SPACE_KIND,
    surfaces: [
      { name: 'Floor', centre: [0, 1, 0], widthMetres: 4, heightMetres: 4, yawDegrees: 0, tiltDegrees: 90, material: 'concrete' },
      { name: 'Wall', centre: [0, -1, 1.25], widthMetres: 4, heightMetres: 2.5, yawDegrees: 0, tiltDegrees: 0, material: 'brick' }
    ],
    microphone: { position: [0, -0.7, 1.2] },
    arrival: { azimuthDegrees: 0, elevationDegrees: 10 }
  });
}

export function readSpace(value) {
  if (!isRecord(value) || value.kind !== SPACE_KIND) throw new SpaceError('This is not a Neartect space.');
  if (value.surfaces !== undefined && !Array.isArray(value.surfaces)) throw new SpaceError('The surfaces are not a list.');
  const air = isRecord(value.air) ? value.air : {};
  const microphone = isRecord(value.microphone) ? value.microphone : {};
  const arrival = isRecord(value.arrival) ? value.arrival : {};
  const render = isRecord(value.render) ? value.render : {};
  return {
    kind: SPACE_KIND,
    version: SPACE_VERSION,
    air: { temperatureCelsius: number(air.temperatureCelsius, RANGES.temperatureCelsius, 20) },
    microphone: { position: position(microphone.position, [0, 0, 1]) },
    arrival: {
      azimuthDegrees: angle(arrival.azimuthDegrees, 0),
      elevationDegrees: number(arrival.elevationDegrees, RANGES.elevationDegrees, 0)
    },
    render: {
      sampleRateHz: Math.round(number(render.sampleRateHz, RANGES.sampleRateHz, 48000)),
      lengthMilliseconds: number(render.lengthMilliseconds, RANGES.lengthMilliseconds, 80),
      order: Math.round(number(render.order, RANGES.order, 3))
    },
    surfaces: (value.surfaces ?? []).map(readSurface)
  };
}

export function speedOfSound(temperatureCelsius) {
  return 331.3 * Math.sqrt(1 + temperatureCelsius / 273.15);
}

export function arrivalVector({ azimuthDegrees, elevationDegrees }) {
  const az = azimuthDegrees * Math.PI / 180;
  const el = elevationDegrees * Math.PI / 180;
  return [Math.cos(el) * Math.sin(az), Math.cos(el) * Math.cos(az), Math.sin(el)];
}

export function surfaceFrame(surface) {
  const yaw = surface.yawDegrees * Math.PI / 180;
  const tilt = surface.tiltDegrees * Math.PI / 180;
  const normal = [Math.cos(tilt) * Math.sin(yaw), Math.cos(tilt) * Math.cos(yaw), Math.sin(tilt)];
  const across = [Math.cos(yaw), -Math.sin(yaw), 0];
  const up = [
    across[1] * normal[2] - across[2] * normal[1],
    across[2] * normal[0] - across[0] * normal[2],
    across[0] * normal[1] - across[1] * normal[0]
  ];
  return { normal, across, up };
}

export function surfaceVertices(surface) {
  const { across, up } = surfaceFrame(surface);
  const w = surface.widthMetres / 2;
  const h = surface.heightMetres / 2;
  return [[-w, -h], [w, -h], [w, h], [-w, h]].map(([a, b]) =>
    surface.centre.map((c, i) => c + a * across[i] + b * up[i]));
}

export function microphoneDistance(space, surface) {
  const { normal } = surfaceFrame(surface);
  return space.microphone.position.reduce((sum, p, i) => sum + (p - surface.centre[i]) * normal[i], 0);
}
