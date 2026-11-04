import { MATERIALS } from './materials.js';

const ORDER = ['A', 'B', 'C'];
const worst = classes => classes.reduce((a, b) => (ORDER.indexOf(b) > ORDER.indexOf(a) ? b : a), 'A');

export function reliabilityOf(space) {
  const objects = space.objects.filter(o => o.strength > 0);
  const components = { specularReflection: 'A' };
  if (objects.length) {
    components.materials = worst(objects.map(o => MATERIALS[o.material]?.reliability ?? 'B'));
    components.finiteSize = 'B';
  }
  if (objects.some(o => o.relief && (o.relief.shape !== 'flat' || o.relief.roughMetres > 0))) components.relief = 'B';
  if (objects.some(o => o.kind === 'grating')) components.gratingLowFrequency = 'C';
  if (objects.some(o => o.bend?.angleDegrees)) components.bend = 'B';
  if (objects.filter(o => o.bend?.angleDegrees).length > 0 && space.render.order > 1) components.bendFocusLimit = 'C';
  if (objects.some(o => o.kind === 'pipe')) components.pipeInterior = 'B';
  if (objects.some(o => o.kind === 'pipe' && o.bend?.angleDegrees)) components.pipeBendModes = 'C';
  const pipes = objects.filter(o => o.kind === 'pipe');
  if (pipes.some(o => o.material !== 'rigid')) components.pipeWall = 'B';
  if (pipes.some(o => o.relief?.roughMetres > 0)) components.pipeRoughness = 'C';
  if (space.objects.some(o => o.strength !== 1)) components.strength = 'C';
  if (space.microphones.some(m => m.pattern !== 'omni') || space.rigs.length) components.pickup = 'A';
  if (space.rigs.some(r => r.kind === 'binaural')) components.head = 'B';
  return { lowest: worst(Object.values(components)), components };
}

export function withReliability(space) {
  return { ...space, reliability: reliabilityOf(space) };
}
