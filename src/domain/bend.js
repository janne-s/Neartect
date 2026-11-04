// Arc length runs along the flat frame; a positive angle bends towards the front (a pipe: towards across or up).

export const BEND_KINDS = Object.freeze(['plane', 'wall', 'grating', 'pipe', 'disc']);
export const SPHERE_KINDS = Object.freeze(['disc']);
const TIGHTEST_PIPE_BEND = 1.02;
export const BEND_AXES = Object.freeze(['up', 'across']);
export const FLAT_BEND = Object.freeze({ angleDegrees: 0, axis: 'up' });
const FLAT_CURVATURE = 1e-9;

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const along = (p, ...terms) => terms.reduce((sum, [v, s]) => sum.map((x, i) => x + v[i] * s), [...p]);
const length = v => Math.hypot(...v);

function sideways(front) {
  const level = [front[1], -front[0], 0];
  const l = length(level);
  return l > 1e-9 ? level.map(v => v / l) : [1, 0, 0];
}

function outward(bend, p) {
  const d = sub(p, bend.touching);
  if (!bend.sphere) return { e: bend.arc, s: dot(d, bend.arc) };
  const radial = along(d, [bend.front, -dot(d, bend.front)]);
  const rho = length(radial);
  return rho > 1e-12 ? { e: radial.map(v => v / rho), s: rho } : { e: sideways(bend.front), s: 0 };
}

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
  if (SPHERE_KINDS.includes(object.kind)) {
    const across = Math.max(w, h);
    const curvature = bend.angleDegrees * Math.PI / 180 / across;
    if (!(across > 0) || Math.abs(curvature) < FLAT_CURVATURE) return null;
    return { curvature, sphere: true, arc: null, axis: null, front: frame.front, touching: object.position };
  }
  const span = bend.axis === 'up' ? w : h;
  if (!(span > 0)) return null;
  const curvature = bend.angleDegrees * Math.PI / 180 / span;
  if (Math.abs(curvature) < FLAT_CURVATURE) return null;
  const arc = bend.axis === 'up' ? frame.across : frame.up;
  const axis = bend.axis === 'up' ? frame.up : frame.across;
  return { curvature, arc, axis, front: frame.front, touching: object.position };
}

export function bentPoint(bend, p) {
  if (!bend) return p;
  const gap = dot(sub(p, bend.touching), bend.front);
  const { e, s } = outward(bend, p);
  const k = bend.curvature;
  const [sin, cos] = [Math.sin(k * s), Math.cos(k * s)];
  return along(p, [e, sin / k - s - gap * sin], [bend.front, (1 - cos) / k - gap + gap * cos]);
}

export function bentNormal(bend, p) {
  if (!bend) return null;
  const { e, s } = outward(bend, p);
  const [sin, cos] = [Math.sin(bend.curvature * s), Math.cos(bend.curvature * s)];
  return along([0, 0, 0], [bend.front, cos], [e, -sin]);
}

export function bentTangent(bend, p) {
  if (!bend) return null;
  const { e, s } = outward(bend, p);
  const [sin, cos] = [Math.sin(bend.curvature * s), Math.cos(bend.curvature * s)];
  return along([0, 0, 0], [e, cos], [bend.front, sin]);
}

export function bentTurn(bend, p, w) {
  if (!bend) return w;
  const { e, s } = outward(bend, p);
  const [sin, cos] = [Math.sin(bend.curvature * s), Math.cos(bend.curvature * s)];
  const [we, wn] = [dot(w, e), dot(w, bend.front)];
  return along(w, [e, (cos - 1) * we - sin * wn], [bend.front, sin * we + (cos - 1) * wn]);
}

export function flatPoint(bend, p) {
  if (!bend) return p;
  const k = bend.curvature;
  const sign = Math.sign(k);
  const centre = along(bend.touching, [bend.front, 1 / k]);
  const w = sub(p, centre);
  if (bend.sphere) {
    const height = -sign * dot(w, bend.front);
    const side = along(w, [bend.front, -dot(w, bend.front)]);
    const width = length(side);
    const rho = Math.atan2(width, height) / Math.abs(k);
    const gap = sign * (1 / Math.abs(k) - length(w));
    const e = width > 1e-15 ? side.map(v => v / width) : sideways(bend.front);
    return along(bend.touching, [e, rho], [bend.front, gap]);
  }
  const t = dot(sub(p, bend.touching), bend.axis);
  const radial = along(w, [bend.axis, -dot(w, bend.axis)]);
  const angle = Math.atan2(sign * dot(radial, bend.arc), -sign * dot(radial, bend.front));
  const s = angle / k;
  const gap = sign * (1 / Math.abs(k) - Math.hypot(...radial));
  return along(bend.touching, [bend.arc, s], [bend.front, gap], [bend.axis, t]);
}

export function bendValues(bend, faceNormal) {
  if (!bend) return [0, 0, 0, 0, 0, 0, 0, 0];
  const towards = Math.sign(dot(faceNormal, bend.front)) || 1;
  return [bend.curvature * towards, ...(bend.arc ?? [0, 0, 0]), ...bend.touching, bend.sphere ? 1 : 0];
}
