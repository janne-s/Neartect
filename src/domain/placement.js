import { expandObjects } from './objects.js';

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const normalize = a => { const n = Math.hypot(...a); return n > 0 ? a.map(v => v / n) : a; };

export const SMALLEST_GAP_METRES = 0.001;

function faceNormal(vertices) {
  const n = [0, 0, 0];
  vertices.forEach((a, i) => {
    const b = vertices[(i + 1) % vertices.length];
    n[0] += (a[1] - b[1]) * (a[2] + b[2]);
    n[1] += (a[2] - b[2]) * (a[0] + b[0]);
    n[2] += (a[0] - b[0]) * (a[1] + b[1]);
  });
  return normalize(n);
}

function footInside(vertices, normal, point) {
  const u = normalize(sub(vertices[1], vertices[0]));
  const v = cross(normal, u);
  const flat = p => [dot(sub(p, vertices[0]), u), dot(sub(p, vertices[0]), v)];
  const q = flat(point);
  const polygon = vertices.map(flat);
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i, i += 1) {
    const [xi, yi] = polygon[i];
    const [xj, yj] = polygon[j];
    if ((yi > q[1]) !== (yj > q[1]) && q[0] < (xj - xi) * (q[1] - yi) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

export function nearestSurface(space, point) {
  let best = null;
  const owners = space.objects.flatMap((object, index) => expandObjectsOf(object).map(face => ({ face, index })));
  for (const { face, index } of owners) {
    const normal = faceNormal(face.vertices);
    if (!footInside(face.vertices, normal, point)) continue;
    const distance = dot(sub(point, face.vertices[0]), normal);
    if (!best || Math.abs(distance) < Math.abs(best.distance)) best = { index, distance, normal };
  }
  return best;
}

function expandObjectsOf(object) {
  return expandObjects({ objects: [object] });
}

export function withGap(point, nearest, gapMetres) {
  const side = nearest.distance < 0 ? -1 : 1;
  const shift = side * Math.max(SMALLEST_GAP_METRES, gapMetres) - nearest.distance;
  return point.map((p, i) => p + nearest.normal[i] * shift);
}

export function spaceCentre(space) {
  const points = expandObjects(space).flatMap(face => face.vertices);
  if (!points.length) return [...space.microphones[0].position];
  return [0, 1, 2].map(i => points.reduce((sum, p) => sum + p[i], 0) / points.length);
}
