import { aimVector, pointPosition } from './objects.js';

// A rig is one placed element that expands into its capsules: a stereo
// layout or a head with two ears. Spacing and angle are between the outer
// pair; the rig's aim is its front. Pans are [left, right] gains for the
// stereo preview, so a mid-side pair decodes as L = (M + S)/√2, R = (M − S)/√2.
const HALF = Math.SQRT1_2;

export const RIG_KINDS = Object.freeze({
  xy: { label: 'XY', short: 'XY', pattern: 'cardioid', spacingMetres: 0, angleDegrees: 90 },
  blumlein: { label: 'Blumlein', short: 'BL', pattern: 'figure8', spacingMetres: 0, angleDegrees: 90 },
  ms: { label: 'Mid-side', short: 'MS', spacingMetres: 0 },
  ortf: { label: 'ORTF', short: 'ORTF', pattern: 'cardioid', spacingMetres: 0.17, angleDegrees: 110 },
  nos: { label: 'NOS', short: 'NOS', pattern: 'cardioid', spacingMetres: 0.3, angleDegrees: 90 },
  ab: { label: 'Spaced pair', short: 'AB', pattern: 'omni', spacingMetres: 0.6, angleDegrees: 0 },
  decca: { label: 'Decca tree', short: 'Decca', pattern: 'omni', spacingMetres: 2, angleDegrees: 0 },
  binaural: { label: 'Head', spacingMetres: 0.175 }
});

// A head is the spherical model (Brown and Duda 1998, B1), sized by its
// width, or the measured KU100 (SADIE II, B2), whose size is fixed.
export const HEADS = Object.freeze({ measured: 'KU100', sphere: 'Sphere' });

// The measured head's width for drawing and its ears' radius: Woodworth's
// radius for the table's largest interaural delay, 687 µs at 343 m/s.
const MEASURED_HEAD_WIDTH_METRES = 0.183;

export const headWidth = rig => (rig.head === 'measured' ? MEASURED_HEAD_WIDTH_METRES : rig.spacingMetres);

export const HEAD_ATTRIBUTION = 'Measured head: SADIE II, KU100, © 2018 University of York, Apache 2.0 (Armstrong et al. 2018, Applied Sciences 8, 2029).';

export const RIG_SPACING_METRES = Object.freeze([0, 5]);
export const RIG_ANGLE_DEGREES = Object.freeze([0, 180]);

const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const normalize = a => { const n = Math.hypot(...a); return n > 0 ? a.map(v => v / n) : a; };
const along = (centre, right, front, x, y) => centre.map((c, i) => c + right[i] * x + front[i] * y);
const turn = (front, right, degrees) => {
  const r = degrees * Math.PI / 180;
  return front.map((f, i) => f * Math.cos(r) + right[i] * Math.sin(r));
};

export function rigFrame(space, rig) {
  const front = aimVector(space, rig);
  let right = normalize(cross(front, [0, 0, 1]));
  if (Math.hypot(...right) < 0.5) right = [1, 0, 0];
  return { front, right, up: cross(right, front) };
}

export function rigMembers(space, rig) {
  const centre = pointPosition(space, rig);
  const { front, right, up } = rigFrame(space, rig);
  const left = right.map(v => -v);
  const half = (rig.kind === 'binaural' ? headWidth(rig) : rig.spacingMetres) / 2;
  const kind = RIG_KINDS[rig.kind];
  const pair = (pattern, angle) => [
    { name: 'L', position: along(centre, right, front, -half, 0), pattern, facing: turn(front, right, -angle / 2), pan: [1, 0] },
    { name: 'R', position: along(centre, right, front, half, 0), pattern, facing: turn(front, right, angle / 2), pan: [0, 1] }
  ];
  switch (rig.kind) {
    case 'ms':
      return [
        { name: 'M', position: centre, pattern: 'cardioid', facing: front, pan: [HALF, HALF] },
        { name: 'S', position: centre, pattern: 'figure8', facing: left, pan: [HALF, -HALF] }
      ];
    case 'decca':
      return [
        ...pair(kind.pattern, rig.angleDegrees),
        { name: 'C', position: along(centre, right, front, 0, rig.spacingMetres * 0.75), pattern: kind.pattern, facing: front, pan: [HALF, HALF] }
      ];
    case 'binaural':
      return [
        { name: 'L', position: centre, ear: { axis: left, radius: Math.max(0.05, half), head: rig.head, front, up }, facing: left, pan: [1, 0] },
        { name: 'R', position: centre, ear: { axis: right, radius: Math.max(0.05, half), head: rig.head, front, up }, facing: right, pan: [0, 1] }
      ];
    default:
      return pair(kind.pattern, rig.angleDegrees);
  }
}
