import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { expandObject, microphonePosition, pipeAround } from './objects.js';
import { arrivalVector, speedOfSound } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0, grid: 1, opening: 2 });

export function packSpace(space) {
  const microphone = microphonePosition(space);
  const pipe = pipeAround(space, microphone);
  const settings = new Float64Array([
    space.render.sampleRateHz,
    speedOfSound(space.air.temperatureCelsius),
    Math.round(space.render.lengthMilliseconds / 1000 * space.render.sampleRateHz),
    space.render.order,
    ...microphone,
    ...arrivalVector(space.arrival),
    ...(pipe ? [pipe.radius, ...pipe.mouthA, ...pipe.mouthB] : [])
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
  const { settings, surfaces } = packSpace(space);
  return { sampleRateHz: space.render.sampleRateHz, samples: core.nearFilter(settings, surfaces) };
}
