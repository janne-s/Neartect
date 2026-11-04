import { objectFrame, pipeReliefDirection, seedOf } from './objects.js';

// Mirrors core/src/relief.rs; keep the two in step.
const ROUGH_COMPONENTS = 32;
const MASK = (1n << 64n) - 1n;

function components(seed, grain) {
  let state = (BigInt(Math.trunc(Math.abs(seed))) + 0x9e3779b97f4a7c15n) & MASK;
  const random = () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & MASK;
    return Number(state >> 11n) / 2 ** 53;
  };
  return Array.from({ length: ROUGH_COMPONENTS }, () => {
    const k = 2 * Math.PI / grain * (0.5 + random());
    const a = 2 * Math.PI * random();
    return [k * Math.cos(a), k * Math.sin(a), 2 * Math.PI * random()];
  });
}

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const normalize = a => { const n = Math.hypot(...a); return n > 0 ? a.map(v => v / n) : a; };

export function hasRelief(object) {
  const r = object.relief;
  return Boolean(r) && (r.shape !== 'flat' || r.roughMetres > 0);
}

export function faceRelief(object, normal, shift = 0, vertices = null) {
  const r = object.relief;
  const { across, up } = objectFrame(object);
  const a = r.directionDegrees * Math.PI / 180;
  const wanted = object.kind === 'pipe' && vertices ? pipeReliefDirection(object, vertices) : across.map((v, i) => v * Math.cos(a) + up[i] * Math.sin(a));
  let along = sub(wanted, normal.map(v => v * dot(wanted, normal)));
  if (Math.hypot(...along) < 1e-6) along = Math.hypot(...cross(normal, [0, 0, 1])) > 1e-6 ? cross(normal, [0, 0, 1]) : [1, 0, 0];
  along = normalize(along);
  const sideways = cross(normal, along);
  const rough = r.roughMetres > 0 ? components(seedOf(object.id), Math.max(1e-3, r.grainMetres)) : [];
  const amplitude = r.roughMetres * Math.sqrt(2 / ROUGH_COMPONENTS);
  const height = r.shape === 'flat' ? 0 : r.heightMetres;
  const period = Math.max(1e-3, r.periodMetres);
  const w = 2 * Math.PI / period;
  const offset = r.offsetMetres + shift;
  const at = point => {
    const d = sub(point, object.position);
    const s = dot(d, along) - offset;
    const t = dot(d, sideways);
    let h = 0;
    if (r.shape === 'waves') h = height * Math.sin(w * s);
    else if (r.shape === 'arches') h = height * Math.abs(Math.sin(Math.PI * s / period));
    else if (r.shape === 'scallops') h = -height * Math.abs(Math.sin(Math.PI * s / period));
    else if (r.shape === 'egg') h = height * Math.sin(w * s) * Math.sin(w * t);
    for (const [kx, ky, phase] of rough) h += amplitude * Math.cos(kx * s + ky * t + phase);
    return h;
  };
  const range = height + 3 * r.roughMetres;
  const finest = Math.min(r.shape === 'flat' ? Infinity : period, r.roughMetres > 0 ? r.grainMetres / 1.5 : Infinity);
  return { at, along, sideways, range, finest, oneWay: r.roughMetres === 0 && r.shape !== 'egg' };
}
