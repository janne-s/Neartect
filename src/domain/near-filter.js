import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { arrivalVector, speedOfSound, surfaceVertices } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0 });
const PATTERN_VALUES = 6;

export function packSpace(space) {
  const settings = new Float64Array([
    space.render.sampleRateHz,
    speedOfSound(space.air.temperatureCelsius),
    Math.round(space.render.lengthMilliseconds / 1000 * space.render.sampleRateHz),
    space.render.order,
    ...space.microphone.position,
    ...arrivalVector(space.arrival)
  ]);
  const records = space.surfaces.flatMap(surface => {
    const vertices = surfaceVertices(surface);
    const body = [
      surface.strength,
      ...reflectionBands(surface.material),
      PATTERN_CODES[surface.pattern.kind] ?? 0,
      ...Array(PATTERN_VALUES).fill(0),
      vertices.length,
      ...vertices.flat()
    ];
    return [body.length + 1, ...body];
  });
  return { settings, surfaces: new Float64Array(records) };
}

export async function nearFilter(space) {
  const core = await loadCore();
  const { settings, surfaces } = packSpace(space);
  return { sampleRateHz: space.render.sampleRateHz, samples: core.nearFilter(settings, surfaces) };
}
