import { loadCore } from './core.js';
import { reflectionBands } from './materials.js';
import { aimVector, expandObject, pipeAround, pointPosition } from './objects.js';
import { rigMembers } from './rigs.js';
import { arrivalVector, PATTERNS, speedOfSound } from './space.js';

const PATTERN_CODES = Object.freeze({ solid: 0, grid: 1, opening: 2 });
const DISTANCE_REFERENCE_METRES = 1;
const NEAREST_LEVEL_METRES = 0.1;

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

// Every channel the space produces: each free microphone, then each rig's
// capsules or ears. Free microphones spread from left to right in the
// preview; a lone one plays in both.
export function listeners(space) {
  const count = space.microphones.length;
  const free = space.microphones.map((m, index) => {
    const angle = count === 1 ? Math.PI / 4 : index / (count - 1) * Math.PI / 2;
    return {
      name: m.name,
      position: pointPosition(space, m),
      pattern: m.pattern,
      facing: aimVector(space, m),
      pan: count === 1 ? [1, 1] : [Math.cos(angle), Math.sin(angle)]
    };
  });
  const rigged = (space.rigs ?? []).flatMap(rig => rigMembers(space, rig).map(member => ({ ...member, name: `${rig.name} ${member.name}` })));
  return [...free, ...rigged];
}

// Each channel's filter is relative to the direct sound at that listener;
// alignment puts them back on one clock and one level, so that a pair keeps
// its time and level differences. The earliest listener has no delay; a point
// source with distance on is levelled to 1 m.
export function alignment(space, positions) {
  const c = speedOfSound(space.air.temperatureCelsius);
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

export function packSpace(space, listener, aligned, longest) {
  const microphone = listener.position;
  const pipe = pipeAround(space, microphone);
  const point = space.source.kind === 'point';
  const rate = space.render.sampleRateHz;
  const settings = new Float64Array([
    rate,
    speedOfSound(space.air.temperatureCelsius),
    Math.round((space.render.lengthMilliseconds / 1000 + longest) * rate),
    space.render.order,
    ...microphone,
    ...arrivalVector(space.arrival),
    point ? 1 : 0,
    ...(point ? pointPosition(space, space.source) : [0, 0, 0]),
    aligned.gain,
    aligned.delaySeconds,
    ...(pipe ? [pipe.radius, ...pipe.mouthA, ...pipe.mouthB] : [0, 0, 0, 0, 0, 0, 0]),
    listener.ear ? 1 : PATTERNS[listener.pattern],
    ...(listener.ear ? listener.ear.axis : listener.facing),
    listener.ear ? 1 : 0,
    listener.ear ? listener.ear.radius : 0
  ]);
  const surfaces = space.objects.filter(object => object !== pipe?.object).flatMap(expandObject);
  const records = surfaces.flatMap(surface => {
    const body = [
      surface.strength,
      ...reflectionBands(surface.material),
      PATTERN_CODES[surface.pattern.kind],
      ...surface.pattern.values,
      ...surface.relief,
      surface.vertices.length,
      ...surface.vertices.flat()
    ];
    return [body.length + 1, ...body];
  });
  return { settings, surfaces: new Float64Array(records) };
}

export async function channelFilter(space, index) {
  const core = await loadCore();
  const list = listeners(space);
  const aligned = alignment(space, list.map(l => l.position));
  const longest = Math.max(...aligned.map(a => a.delaySeconds));
  const { settings, surfaces } = packSpace(space, list[index], aligned[index], longest);
  return core.nearFilter(settings, surfaces);
}

export async function nearFilter(space) {
  const list = listeners(space);
  const channels = [];
  for (let index = 0; index < list.length; index += 1) channels.push(await channelFilter(space, index));
  return { sampleRateHz: space.render.sampleRateHz, channels, names: list.map(l => l.name), pans: list.map(l => l.pan), samples: channels[0] };
}
