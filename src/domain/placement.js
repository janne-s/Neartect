import { surfaceFrame, surfaceVertices } from './space.js';

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

export const SMALLEST_GAP_METRES = 0.001;

function footInside(surface, point) {
  const { across, up } = surfaceFrame(surface);
  const d = sub(point, surface.centre);
  return Math.abs(dot(d, across)) <= surface.widthMetres / 2 && Math.abs(dot(d, up)) <= surface.heightMetres / 2;
}

export function nearestSurface(space) {
  const point = space.microphone.position;
  let best = null;
  space.surfaces.forEach((surface, index) => {
    if (!footInside(surface, point)) return;
    const distance = dot(sub(point, surface.centre), surfaceFrame(surface).normal);
    if (!best || Math.abs(distance) < Math.abs(best.distance)) best = { index, distance };
  });
  return best;
}

export function withGap(space, index, gapMetres) {
  const surface = space.surfaces[index];
  const { normal } = surfaceFrame(surface);
  const point = space.microphone.position;
  const current = dot(sub(point, surface.centre), normal);
  const side = current < 0 ? -1 : 1;
  const shift = side * Math.max(SMALLEST_GAP_METRES, gapMetres) - current;
  return point.map((p, i) => p + normal[i] * shift);
}

export function spaceCentre(space) {
  const points = space.surfaces.flatMap(surfaceVertices);
  if (!points.length) return [...space.microphone.position];
  return [0, 1, 2].map(i => points.reduce((sum, p) => sum + p[i], 0) / points.length);
}
