import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { aimVector, expandObject, pipeAround, pointPosition } from './objects.js';
import { arrivalVector, PATTERNS, speedOfSound } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0, grid: 1, opening: 2 });
const DISTANCE_REFERENCE_METRES = 1;
const NEAREST_LEVEL_METRES = 0.1;

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

// Each microphone's filter is relative to the direct sound at that
// microphone; alignment puts them back on one clock and one level, so that a
// pair keeps its time and level differences. The earliest microphone has no
// delay; a point source with distance on is levelled to 1 m.
export function alignment(space) {
  const c = speedOfSound(space.air.temperatureCelsius);
  const positions = space.microphones.map(m => pointPosition(space, m));
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

export function packSpace(space, microphoneIndex = 0, aligned = alignment(space)) {
  const chosen = space.microphones[microphoneIndex];
  const microphone = pointPosition(space, chosen);
  const pipe = pipeAround(space, microphone);
  const point = space.source.kind === 'point';
  const rate = space.render.sampleRateHz;
  const longest = Math.max(...aligned.map(a => a.delaySeconds));
  const { delaySeconds, gain } = aligned[microphoneIndex];
  const settings = new Float64Array([
    rate,
    speedOfSound(space.air.temperatureCelsius),
    Math.round((space.render.lengthMilliseconds / 1000 + longest) * rate),
    space.render.order,
    ...microphone,
    ...arrivalVector(space.arrival),
    point ? 1 : 0,
    ...(point ? pointPosition(space, space.source) : [0, 0, 0]),
    gain,
    delaySeconds,
    ...(pipe ? [pipe.radius, ...pipe.mouthA, ...pipe.mouthB] : [0, 0, 0, 0, 0, 0, 0]),
    PATTERNS[chosen.pattern],
    ...aimVector(space, chosen)
  ]);
  const surfaces = space.objects.filter(object => object !== pipe?.object).flatMap(expandObject);
  const records = surfaces.flatMap(surface => {
    const body = [
      surface.strength,
      ...reflectionBands(surface.material),
      PATTERN_CODES[surface.pattern.kind],
      ...surface.pattern.values,
      surface.vertices.length,
      ...surface.vertices.flat()
    ];
    return [body.length + 1, ...body];
  });
  return { settings, surfaces: new Float64Array(records) };
}

export async function nearFilter(space) {
  const core = await loadCore();
  const aligned = alignment(space);
  const channels = space.microphones.map((_, index) => {
    const { settings, surfaces } = packSpace(space, index, aligned);
    return core.nearFilter(settings, surfaces);
  });
  return { sampleRateHz: space.render.sampleRateHz, channels, samples: channels[0] };
}
