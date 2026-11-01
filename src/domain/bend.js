// A plane, wall, or grating bent into a circular arc, as the core bends it
// (core/src/bend.rs). The object's flat frame stays its laid-out shape: arc
// length runs along across (axis up) or along up (axis across), and offsets
// from it in front become distances along the arc's normal. A positive angle
// bends the surface towards its front, so that it is concave seen from there.
// A pipe's centreline bends the same way, its arc along front and towards
// across (axis up) or up (axis across), no tighter than its own radius.

export const BEND_KINDS = Object.freeze(['plane', 'wall', 'grating', 'pipe']);
const TIGHTEST_PIPE_BEND = 1.02;
export const BEND_AXES = Object.freeze(['up', 'across']);
export const FLAT_BEND = Object.freeze({ angleDegrees: 0, axis: 'up' });
const FLAT_CURVATURE = 1e-9;

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const along = (p, ...terms) => terms.reduce((sum, [v, s]) => sum.map((x, i) => x + v[i] * s), [...p]);

// The bend of an object in space: curvature towards its front (1/m), the
// arc's direction, the axis, the front, and the point where the arc touches
// the flat frame. Null when the object is flat or cannot bend.
export function bendOf(object, frame) {
  const bend = object.bend ?? FLAT_BEND;
  if (!BEND_KINDS.includes(object.kind) || !bend.angleDegrees) return null;
  const [w, d, h] = object.size;
  if (object.kind === 'pipe') {
    const tightest = 1 / (TIGHTEST_PIPE_BEND * w / 2);
    const turn = bend.angleDegrees * Math.PI / 180 / d;
    const curvature = Math.sign(turn) * Math.min(Math.abs(turn), tightest);
    if (!(d > 0) || Math.abs(curvature) < FLAT_CURVATURE) return null;
    const towards = bend.axis === 'up' ? frame.across : frame.up;
    return { curvature, arc: frame.front, axis: bend.axis === 'up' ? frame.up : frame.across, front: towards, touching: object.position };
  }
  const length = bend.axis === 'up' ? w : h;
  if (!(length > 0)) return null;
  const curvature = bend.angleDegrees * Math.PI / 180 / length;
  if (Math.abs(curvature) < FLAT_CURVATURE) return null;
  const arc = bend.axis === 'up' ? frame.across : frame.up;
  const axis = bend.axis === 'up' ? frame.up : frame.across;
  return { curvature, arc, axis, front: frame.front, touching: object.position };
}

// A point given in the flat frame (arc length, distance in front, along the
// axis) to its place on or off the bent surface.
export function bentPoint(bend, p) {
  if (!bend) return p;
  const d = sub(p, bend.touching);
  const s = dot(d, bend.arc);
  const gap = dot(d, bend.front);
  const k = bend.curvature;
  const [sin, cos] = [Math.sin(k * s), Math.cos(k * s)];
  return along(p, [bend.arc, sin / k - s - gap * sin], [bend.front, (1 - cos) / k - gap + gap * cos]);
}

export function bentNormal(bend, p) {
  if (!bend) return null;
  const s = dot(sub(p, bend.touching), bend.arc);
  const [sin, cos] = [Math.sin(bend.curvature * s), Math.cos(bend.curvature * s)];
  return along([0, 0, 0], [bend.front, cos], [bend.arc, -sin]);
}

export function bentTangent(bend, p) {
  if (!bend) return null;
  const s = dot(sub(p, bend.touching), bend.arc);
  const [sin, cos] = [Math.sin(bend.curvature * s), Math.cos(bend.curvature * s)];
  return along([0, 0, 0], [bend.arc, cos], [bend.front, sin]);
}

// The inverse of `bentPoint`: a point in space back to the flat frame.
export function flatPoint(bend, p) {
  if (!bend) return p;
  const k = bend.curvature;
  const sign = Math.sign(k);
  const centre = along(bend.touching, [bend.front, 1 / k]);
  const w = sub(p, centre);
  const t = dot(sub(p, bend.touching), bend.axis);
  const radial = along(w, [bend.axis, -dot(w, bend.axis)]);
  const angle = Math.atan2(sign * dot(radial, bend.arc), -sign * dot(radial, bend.front));
  const s = angle / k;
  const gap = sign * (1 / Math.abs(k) - Math.hypot(...radial));
  return along(bend.touching, [bend.arc, s], [bend.front, gap], [bend.axis, t]);
}

// The core's numbers for a face: curvature towards the face's own normal,
// the arc's direction, and the touching point.
export function bendValues(bend, faceNormal) {
  if (!bend) return [0, 0, 0, 0, 0, 0, 0];
  const towards = Math.sign(dot(faceNormal, bend.front)) || 1;
  return [bend.curvature * towards, ...bend.arc, ...bend.touching];
}
