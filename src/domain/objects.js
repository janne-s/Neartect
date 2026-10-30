// Objects are what the user builds; each expands into planar surfaces for the
// core. Local frame: x across, y the front normal, z up; the object is tilted
// about x, then turned to its facing (compass yaw) about the vertical.

export const OBJECT_KINDS = Object.freeze(['plane', 'disc', 'grating', 'wall', 'box', 'corner', 'recess', 'pipe']);
export const DISC_SIDES = 48;
export const PIPE_SIDES = 16;
const RECESS_MARGIN_METRES = 0.5;

export const OBJECT_DEFAULTS = Object.freeze({
  plane: { size: [2, 0, 2] },
  disc: { size: [0.6, 0, 0.6] },
  grating: { size: [1, 0, 1], pattern: { kind: 'grid', periodMetres: [0.034, 0.034], barMetres: [0.004, 0.004], thicknessMetres: 0.03 } },
  wall: { size: [3, 0, 2.5], opening: { shape: 'round', sizeMetres: [0.4, 0.4], offsetMetres: [0, 0] } },
  box: { size: [0.6, 0.6, 0.6] },
  corner: { size: [2, 2, 2.5] },
  recess: { size: [0.6, 0.4, 0.8] },
  pipe: { size: [0.3, 2, 0.3] }
});

export const FLAT_KINDS = Object.freeze(['plane', 'disc', 'grating', 'wall']);

export function objectFrame({ yawDegrees, tiltDegrees }) {
  const yaw = yawDegrees * Math.PI / 180;
  const tilt = tiltDegrees * Math.PI / 180;
  const front = [Math.cos(tilt) * Math.sin(yaw), Math.cos(tilt) * Math.cos(yaw), Math.sin(tilt)];
  const across = [Math.cos(yaw), -Math.sin(yaw), 0];
  const up = [
    across[1] * front[2] - across[2] * front[1],
    across[2] * front[0] - across[0] * front[2],
    across[0] * front[1] - across[1] * front[0]
  ];
  return { across, front, up };
}

export function toWorld(object, [x, y, z]) {
  const { across, front, up } = objectFrame(object);
  return object.position.map((p, i) => p + x * across[i] + y * front[i] + z * up[i]);
}

const rectangleXZ = (cx, y, cz, w, h) => [[cx - w / 2, y, cz - h / 2], [cx + w / 2, y, cz - h / 2], [cx + w / 2, y, cz + h / 2], [cx - w / 2, y, cz + h / 2]];
const rectangleYZ = (x, cy, cz, d, h) => [[x, cy - d / 2, cz - h / 2], [x, cy + d / 2, cz - h / 2], [x, cy + d / 2, cz + h / 2], [x, cy - d / 2, cz + h / 2]];
const rectangleXY = (cx, cy, z, w, d) => [[cx - w / 2, cy - d / 2, z], [cx + w / 2, cy - d / 2, z], [cx + w / 2, cy + d / 2, z], [cx - w / 2, cy + d / 2, z]];

function localFaces(object) {
  const [w, d, h] = object.size;
  switch (object.kind) {
    case 'plane':
    case 'grating':
    case 'wall':
      return [{ local: rectangleXZ(0, 0, 0, w, h), pattern: planePattern(object) }];
    case 'disc':
      return [{ local: Array.from({ length: DISC_SIDES }, (_, i) => {
        const a = 2 * Math.PI * i / DISC_SIDES;
        return [w / 2 * Math.cos(a), 0, h / 2 * Math.sin(a)];
      }) }];
    case 'box':
      return [
        rectangleXZ(0, d / 2, 0, w, h), rectangleXZ(0, -d / 2, 0, w, h),
        rectangleYZ(w / 2, 0, 0, d, h), rectangleYZ(-w / 2, 0, 0, d, h),
        rectangleXY(0, 0, h / 2, w, d), rectangleXY(0, 0, -h / 2, w, d)
      ].map(local => ({ local }));
    case 'corner':
      return [rectangleXY(w / 2, d / 2, 0, w, d), rectangleYZ(0, d / 2, h / 2, d, h), rectangleXZ(w / 2, 0, h / 2, w, h)].map(local => ({ local }));
    case 'recess': {
      const m = RECESS_MARGIN_METRES;
      return [
        { local: rectangleXZ(0, 0, 0, w + 2 * m, h + 2 * m), pattern: { kind: 'opening', values: [w / 2 + m, h / 2 + m, w / 2, h / 2, 0, 0] } },
        { local: rectangleXZ(0, -d, 0, w, h) },
        { local: rectangleYZ(-w / 2, -d / 2, 0, d, h) },
        { local: rectangleYZ(w / 2, -d / 2, 0, d, h) },
        { local: rectangleXY(0, -d / 2, h / 2, w, d) },
        { local: rectangleXY(0, -d / 2, -h / 2, w, d) }
      ];
    }
    case 'pipe': {
      const radius = w / 2;
      const ring = i => {
        const a = 2 * Math.PI * i / PIPE_SIDES;
        return [radius * Math.cos(a), radius * Math.sin(a)];
      };
      return Array.from({ length: PIPE_SIDES }, (_, i) => {
        const [x0, z0] = ring(i);
        const [x1, z1] = ring(i + 1);
        return { local: [[x0, -d / 2, z0], [x0, d / 2, z0], [x1, d / 2, z1], [x1, -d / 2, z1]] };
      });
    }
    default:
      return [];
  }
}

function planePattern(object) {
  const [w, , h] = object.size;
  if (object.kind === 'grating' && object.pattern?.kind === 'grid') {
    const p = object.pattern;
    return { kind: 'grid', values: [...p.periodMetres, ...p.barMetres, p.thicknessMetres, 0] };
  }
  if (object.kind === 'wall' && object.opening) {
    const o = object.opening;
    return { kind: 'opening', values: [w / 2 + o.offsetMetres[0], h / 2 + o.offsetMetres[1], o.sizeMetres[0] / 2, o.sizeMetres[1] / 2, o.shape === 'round' ? 1 : 0, 0] };
  }
  return { kind: 'solid', values: [0, 0, 0, 0, 0, 0] };
}

export const RELIEF_SHAPES = Object.freeze(['flat', 'waves', 'arches', 'scallops', 'egg']);
export const FLAT_RELIEF = Object.freeze({ shape: 'flat', heightMetres: 0.02, periodMetres: 0.2, directionDegrees: 0, roughMetres: 0, grainMetres: 0.05, offsetMetres: 0, speedMetresPerSecond: 0 });

export function seedOf(id) {
  let hash = 2166136261;
  for (const ch of id) hash = Math.imul(hash ^ ch.charCodeAt(0), 16777619) >>> 0;
  return hash % 1000003;
}

// The relief's direction lies in the object's across–up plane (0° along
// across, 90° along up) and its field is anchored at the object's position,
// so every face of the object shares one field.
export function reliefValues(object, shift = 0) {
  const r = object.relief ?? FLAT_RELIEF;
  const { across, up } = objectFrame(object);
  const a = r.directionDegrees * Math.PI / 180;
  const direction = across.map((v, i) => v * Math.cos(a) + up[i] * Math.sin(a));
  return [
    RELIEF_SHAPES.indexOf(r.shape), r.shape === 'flat' ? 0 : r.heightMetres, r.periodMetres,
    ...direction, ...object.position,
    r.offsetMetres + shift, r.roughMetres, r.grainMetres, seedOf(object.id)
  ];
}

export function expandObject(object) {
  return localFaces(object).map(({ local, pattern }) => ({
    vertices: local.map(point => toWorld(object, point)),
    material: object.material,
    strength: object.strength,
    pattern: pattern ?? { kind: 'solid', values: [0, 0, 0, 0, 0, 0] },
    relief: reliefValues(object)
  }));
}

export function expandObjects(space) {
  return space.objects.flatMap(expandObject);
}

export function objectBounds(object) {
  const [w, d, h] = object.size;
  switch (object.kind) {
    case 'corner': return { min: [0, 0, 0], max: [w, d, h] };
    case 'recess': return { min: [-w / 2, -d, -h / 2], max: [w / 2, 0, h / 2] };
    default: return { min: [-w / 2, -d / 2, -h / 2], max: [w / 2, d / 2, h / 2] };
  }
}

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

export function toLocal(object, point) {
  const { across, front, up } = objectFrame(object);
  const d = sub(point, object.position);
  return [dot(d, across), dot(d, front), dot(d, up)];
}

export function anchor(object) {
  const [, d] = object.size;
  if (object.kind === 'box' || object.kind === 'pipe') return [0, d / 2, 0];
  return [0, 0, 0];
}

// Microphones and a point source are bound points: an offset from an
// object's anchor, or a plain position when bound to the world.
export function referenceObject(space, point) {
  const id = point.reference;
  return id ? space.objects.find(o => o.id === id) ?? null : null;
}

export function pointPosition(space, point) {
  const object = referenceObject(space, point);
  if (!object) return point.position;
  const base = anchor(object);
  return toWorld(object, point.offset.map((v, i) => v + base[i]));
}

export function referTo(space, point, id) {
  const position = pointPosition(space, point);
  const facing = point.aim ? aimVector(space, point) : null;
  const object = id ? space.objects.find(o => o.id === id) : null;
  point.reference = object ? object.id : null;
  point.offset = object ? toLocal(object, position).map((v, i) => v - anchor(object)[i]) : [0, 0, 0];
  point.position = position;
  if (facing) aimAt(space, point, facing);
}

// A microphone's aim is a compass-style azimuth and an elevation in its
// reference frame: the object's across, front, and up when bound, east,
// north, and up otherwise.
export function aimVector(space, microphone) {
  const az = microphone.aim.azimuthDegrees * Math.PI / 180;
  const el = microphone.aim.elevationDegrees * Math.PI / 180;
  const local = [Math.cos(el) * Math.sin(az), Math.cos(el) * Math.cos(az), Math.sin(el)];
  const object = referenceObject(space, microphone);
  if (!object) return local;
  const { across, front, up } = objectFrame(object);
  return [0, 1, 2].map(i => local[0] * across[i] + local[1] * front[i] + local[2] * up[i]);
}

export function aimAt(space, microphone, direction) {
  const object = referenceObject(space, microphone);
  const d = object ? (() => { const { across, front, up } = objectFrame(object); return [dot(direction, across), dot(direction, front), dot(direction, up)]; })() : direction;
  const n = Math.hypot(...d) || 1;
  const az = Math.atan2(d[0], d[1]) * 180 / Math.PI;
  microphone.aim = {
    azimuthDegrees: ((az % 360) + 360) % 360,
    elevationDegrees: Math.asin(Math.max(-1, Math.min(1, d[2] / n))) * 180 / Math.PI
  };
}

export function placePointAt(space, point, position) {
  const object = referenceObject(space, point);
  if (object) point.offset = toLocal(object, position).map((v, i) => v - anchor(object)[i]);
  point.position = position;
}

export function firstListener(space) {
  return space.microphones[0] ?? space.rigs[0];
}

export function boundPoints(space) {
  return [...space.microphones, ...(space.rigs ?? []), space.source];
}

export function placePoints(space) {
  boundPoints(space).forEach(point => { point.position = pointPosition(space, point); });
}

export function releaseObject(space, id) {
  boundPoints(space).forEach(point => { if (point.reference === id) referTo(space, point, null); });
}

// The inside model's share for a point on a pipe's axis region near a
// mouth: 1 deeper than the radius inside, 0 as far outside, and a smoothstep
// between, so that the filter changes smoothly through the mouth (class C).
export function pipeShare(space, point) {
  for (const object of space.objects) {
    if (object.kind !== 'pipe' || !(object.strength > 0)) continue;
    const [x, y, z] = toLocal(object, point);
    const [diameter, length] = object.size;
    const radius = diameter / 2;
    const depth = length / 2 - Math.abs(y);
    if (Math.hypot(x, z) >= radius || depth <= -radius) continue;
    const t = Math.min(1, Math.max(0, (depth + radius) / (2 * radius)));
    return { pipe: { object, mouthA: toWorld(object, [0, -length / 2, 0]), mouthB: toWorld(object, [0, length / 2, 0]), radius }, share: t * t * (3 - 2 * t) };
  }
  return null;
}

export function pipeAround(space, point) {
  for (const object of space.objects) {
    if (object.kind !== 'pipe' || !(object.strength > 0)) continue;
    const [x, y, z] = toLocal(object, point);
    const [diameter, length] = object.size;
    if (Math.hypot(x, z) < diameter / 2 && Math.abs(y) < length / 2) {
      return { object, mouthA: toWorld(object, [0, -length / 2, 0]), mouthB: toWorld(object, [0, length / 2, 0]), radius: diameter / 2 };
    }
  }
  return null;
}
