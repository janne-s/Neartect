import { fitCanvas, palette } from '../shared/ui/canvas.js';
import { arrivalVector, signedTurn } from '../domain/space.js';
import { aimAt, aimVector, boundPoints, expandObject, followResize, resizedPosition, objectBounds, objectFrame, placePointAt, pointFrame, pointPosition, referenceObject, surfacePoint, toWorld } from '../domain/objects.js';
import { bentNormal, bentPoint } from '../domain/bend.js';
import { wheelGesture } from '../shared/ui/wheel.js';
import { faceNormal, nearestSurface } from '../domain/placement.js';
import { faceRelief, hasRelief } from '../domain/relief.js';
import { headWidth, rigFrame, rigMembers } from '../domain/rigs.js';

const FIELD_OF_VIEW = 0.9;
const CLICK_PIXELS = 4;
const HANDLE_PIXELS = 8;
const ARROW_PIXELS = 70;
const RING_PIXELS = 58;
const RING_SEGMENTS = 48;
const MOST_BARS_DRAWN = 80;
const NEAR_METRES = 0.05;
const POINT_ARROW_START_PIXELS = 16;
const MOVE_ARROW_PIXELS = 34;
const MOVE_ARROW_SHORTEST_PIXELS = 6;
const MOVE_HEAD_PIXELS = 11;
const MOVE_HEAD_HALF_PIXELS = 9;
const MOVE_SHAFT_PIXELS = 5;
const AIM_RING_PIXELS = 40;
const GRID_LEAST_METRES = 6;
const GRID_MOST_METRES = 40;
const GRID_FINE_DISTANCE_METRES = 3;
const GRID_FINE_REACH_METRES = 3;
const RIG_FRONT_PIXELS = 46;
const HEAD_LEAST_PIXELS = 9;
const HEAD_NOSE_PIXELS = 18;
const RIG_COLOUR_START = 3;
const ARRIVAL_STOP = 0.6;
const ARROWHEAD_PIXELS = 9;
const ANGLE_SNAP_DEGREES = 5;
const SNAP_PER_GRID = 20;
const WORLD_AXES = Object.freeze([[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
const AXIS_NAMES = Object.freeze(['East', 'North', 'Up']);
const SIZE_NAMES = Object.freeze(['W', 'D', 'H']);
const OFFSET_NAMES = Object.freeze(['Across', 'Gap', 'Up']);
const OWN_AXIS_NAMES = Object.freeze(['Across', 'Front', 'Up']);
const DIMMED = 0.3;
const SURFACE_SNAP_PIXELS = 10;
const ZOOM_PER_PIXEL = 0.003;
const AIM_PIXELS = 26;
const MOST_RELIEF_CELLS = 2400;
const MOST_RELIEF_CELLS_ALONG = 240;
const MOST_PROFILE_POINTS = 400;
const BEND_STRIPS = 24;
const BEND_RINGS = 4;
const BEND_PIECES = 16;

const withAlpha = (color, alpha) => {
  const hex = color.trim().replace('#', '');
  if (hex.length !== 6) return color;
  const [r, g, b] = [0, 2, 4].map(i => parseInt(hex.slice(i, i + 2), 16));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
};
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const scale = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const normalize = a => { const n = Math.hypot(...a); return n > 0 ? scale(a, 1 / n) : a; };
const degrees = radians => radians * 180 / Math.PI;
const wrap = angle => ((angle % 360) + 360) % 360;

export function formatGap(metres) {
  const m = Math.abs(metres);
  const sign = metres < 0 ? '−' : '';
  if (m < 0.01) return `${sign}${+(m * 1000).toFixed(1)} mm`;
  if (m < 1) return `${sign}${+(m * 100).toFixed(1)} cm`;
  return `${sign}${+m.toFixed(2)} m`;
}

function distanceToSegment([px, py], [ax, ay], [bx, by]) {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / Math.max(1e-9, dx * dx + dy * dy)));
  return Math.hypot(px - ax - t * dx, py - ay - t * dy);
}

function insidePolygon([x, y], corners) {
  let inside = false;
  for (let a = 0, b = corners.length - 1; a < corners.length; b = a, a += 1) {
    const [xa, ya] = corners[a];
    const [xb, yb] = corners[b];
    if ((ya > y) !== (yb > y) && x < (xb - xa) * (y - ya) / (yb - ya) + xa) inside = !inside;
  }
  return inside;
}

export function createNearView(canvas, { onSelect, onEdit, onCommit, onAreaEnd = () => {}, snapSurfaces = () => false, showGrid = () => true, scrollPans = () => false, ownAxes = () => false }) {
  const camera = { yaw: -0.6, pitch: 0.45, distance: 6, target: [0, 0, 1] };
  let space = null;
  let selection = null;
  let hover = null;
  let faces = [];
  let handles = [];
  let size = { width: 1, height: 1 };
  let drag = null;
  let readout = null;
  let areaArmed = false;
  let reliefTime = 0;

  function basis() {
    const forward = [Math.cos(camera.pitch) * Math.sin(camera.yaw), Math.cos(camera.pitch) * Math.cos(camera.yaw), -Math.sin(camera.pitch)];
    const right = [Math.cos(camera.yaw), -Math.sin(camera.yaw), 0];
    const up = [
      right[1] * forward[2] - right[2] * forward[1],
      right[2] * forward[0] - right[0] * forward[2],
      right[0] * forward[1] - right[1] * forward[0]
    ];
    return { forward, right, up, eye: sub(camera.target, scale(forward, camera.distance)) };
  }

  const focal = () => size.height / 2 / Math.tan(FIELD_OF_VIEW / 2);

  function project(point) {
    const { forward, right, up, eye } = basis();
    const d = sub(point, eye);
    const depth = dot(d, forward);
    if (depth < 0.01) return null;
    return [size.width / 2 + focal() * dot(d, right) / depth, size.height / 2 - focal() * dot(d, up) / depth, depth];
  }

  function ray(x, y) {
    const { forward, right, up, eye } = basis();
    return { origin: eye, direction: normalize(add(forward, add(scale(right, (x - size.width / 2) / focal()), scale(up, -(y - size.height / 2) / focal())))) };
  }

  const pixelsToMetres = (pixels, point) => pixels * (project(point)?.[2] ?? camera.distance) / focal();
  const gridSpacing = () => 10 ** Math.floor(Math.log10(camera.distance / 2));

  function snap(value, step, free) {
    return free ? value : Math.round(value / step) * step;
  }

  function clipped(a, b) {
    const { forward, eye } = basis();
    const da = dot(sub(a, eye), forward) - NEAR_METRES;
    const db = dot(sub(b, eye), forward) - NEAR_METRES;
    if (da < 0 && db < 0) return null;
    if (da < 0) return [add(a, scale(sub(b, a), da / (da - db))), b];
    if (db < 0) return [a, add(a, scale(sub(b, a), da / (da - db)))];
    return [a, b];
  }

  function segment(context, a, b) {
    const cut = clipped(a, b);
    if (!cut) return null;
    const p = project(cut[0]);
    const q = project(cut[1]);
    if (!p || !q) return null;
    context.moveTo(p[0], p[1]);
    context.lineTo(q[0], q[1]);
    return [p, q];
  }

  function line(context, a, b) {
    context.beginPath();
    const ends = segment(context, a, b);
    if (ends) context.stroke();
    return ends;
  }

  function objectCentre(object) {
    const { min, max } = objectBounds(object);
    return toWorld(object, [0, 1, 2].map(i => (min[i] + max[i]) / 2));
  }

  const activeObjectId = () => (selection?.type === 'object' ? selection.id : !selection && hover?.type === 'object' ? hover.id : null);
  const focusing = () => Boolean(drag?.handle) || Boolean(selection?.type);

  function points() {
    const list = space.microphones.map((point, index) => ({ key: `microphone:${point.id}`, type: 'microphone', id: point.id, index, point }));
    (space.rigs ?? []).forEach((point, index) => list.push({ key: `rig:${point.id}`, type: 'rig', id: point.id, index: RIG_COLOUR_START + index, point }));
    if (space.source.kind === 'point') list.push({ key: 'source', type: 'source', point: space.source });
    return list;
  }

  const grouped = () => (selection?.type === 'group' ? selection : null);
  const inGroup = key => Boolean(grouped()?.points.includes(key));
  const objectInGroup = id => Boolean(grouped()?.objects.includes(id));
  const isSelected = item => (selection?.type === item.type && (item.type === 'source' || selection.id === item.id)) || inGroup(item.key);
  const itemKey = item => (item.type === 'source' ? 'source' : item.type === 'microphone' || item.type === 'rig' ? `${item.type}:${item.id}` : null);

  function groupCentre() {
    const group = grouped();
    const places = [
      ...space.objects.filter(o => group.objects.includes(o.id)).map(objectCentre),
      ...points().filter(item => group.points.includes(item.key)).map(item => item.point.position)
    ];
    if (!places.length) return null;
    return [0, 1, 2].map(i => places.reduce((sum, p) => sum + p[i], 0) / places.length);
  }

  function groupSnapshot() {
    const group = grouped();
    return {
      objects: space.objects.filter(o => group.objects.includes(o.id)).map(o => ({ id: o.id, position: [...o.position], yawDegrees: o.yawDegrees })),
      points: points().filter(item => group.points.includes(item.key)).map(item => ({ key: item.key, position: [...pointPosition(space, item.point)] }))
    };
  }

  function moveGroup(next, snapshot, map, yawDelta) {
    const ids = new Set(snapshot.objects.map(o => o.id));
    for (const o of snapshot.objects) {
      const object = next.objects.find(x => x.id === o.id);
      if (!object) continue;
      object.position = map(o.position);
      object.yawDegrees = wrap(o.yawDegrees + yawDelta);
    }
    for (const p of snapshot.points) {
      const point = findPoint(next, p.key);
      if (!point || ids.has(point.reference)) continue;
      placePointAt(next, point, map(p.position));
    }
  }
  const findPoint = (next, key) => (key === 'source' ? next.source
    : key.startsWith('rig:') ? next.rigs.find(r => `rig:${r.id}` === key)
    : next.microphones.find(m => `microphone:${m.id}` === key));

  function listeningCentre() {
    const chosen = space.microphones.find(m => selection?.type === 'microphone' && m.id === selection.id)
      ?? space.rigs.find(r => selection?.type === 'rig' && r.id === selection.id)
      ?? space.microphones[0] ?? space.rigs[0];
    return chosen ? chosen.position : camera.target;
  }

  const lerp = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
  const placed = (face, p) => bentPoint(face.bend, p);

  function faceParts(face) {
    if (!face.bend) return [{ world: face.vertices, edges: null, first: true }];
    if (face.bend.sphere) {
      const centre = face.bend.touching;
      const rim = face.vertices;
      return Array.from({ length: BEND_RINGS }, (_, i) => rim.map((v, j) => {
        const w = rim[(j + 1) % rim.length];
        const [t0, t1] = [i / BEND_RINGS, (i + 1) / BEND_RINGS];
        const world = [lerp(centre, v, t0), lerp(centre, v, t1), lerp(centre, w, t1), lerp(centre, w, t0)].map(p => placed(face, p));
        return { world, edges: [false, i === BEND_RINGS - 1, false, false], first: i === 0 && j === 0 };
      })).flat();
    }
    const [a, b, c, d] = face.vertices;
    const arcAcross = Math.abs(dot(normalize(sub(b, a)), face.bend.arc)) > 0.5;
    const [p0, p1, q0, q1] = arcAcross ? [a, b, d, c] : [a, d, b, c];
    return Array.from({ length: BEND_STRIPS }, (_, i) => {
      const [t0, t1] = [i / BEND_STRIPS, (i + 1) / BEND_STRIPS];
      const world = [lerp(p0, p1, t0), lerp(p0, p1, t1), lerp(q0, q1, t1), lerp(q0, q1, t0)].map(p => placed(face, p));
      return { world, edges: [true, i === BEND_STRIPS - 1, true, i === 0], first: i === 0 };
    });
  }

  function drawFaces(context, colors, fill, edge) {
    faces = [];
    space.objects.forEach(object => {
      expandObject(object).forEach((face, faceIndex) => {
        for (const part of faceParts(face)) {
          const corners = part.world.map(project);
          if (corners.some(c => !c)) continue;
          faces.push({ object, face, faceIndex, corners, part, depth: corners.reduce((s, c) => s + c[2], 0) / corners.length });
        }
      });
    });
    faces.sort((a, b) => b.depth - a.depth);
    const active = activeObjectId();
    for (const { object, face, corners, part } of faces) {
      const member = object.id === active || objectInGroup(object.id);
      const highlighted = member || (hover?.type === 'object' && hover.id === object.id);
      const dim = focusing() && !member ? DIMMED : 1;
      const outlineColor = highlighted ? colors.accent : edge;
      context.globalAlpha = (object.strength > 0 ? 1 : 0.4) * dim;
      const hole = face.pattern.kind === 'opening' ? openingOutline(face) : null;
      const shape = () => {
        context.beginPath();
        corners.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.closePath();
        if (hole) {
          hole.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
          context.closePath();
        }
      };
      if (part.edges) {
        context.save();
        context.beginPath();
        corners.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.closePath();
        context.clip();
        if (face.pattern.kind !== 'grid') {
          shape();
          context.fillStyle = fill;
          context.fill('evenodd');
        }
        if (hole) {
          context.beginPath();
          hole.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
          context.closePath();
          context.strokeStyle = outlineColor;
          context.lineWidth = member ? 2 : 1;
          context.stroke();
        }
        context.restore();
        context.strokeStyle = outlineColor;
        context.lineWidth = member ? 2 : 1;
        context.beginPath();
        part.edges.forEach((on, i) => {
          if (!on) return;
          const [p, q] = [corners[i], corners[(i + 1) % 4]];
          context.moveTo(p[0], p[1]);
          context.lineTo(q[0], q[1]);
        });
        context.stroke();
      } else {
        shape();
        if (face.pattern.kind !== 'grid') {
          context.fillStyle = fill;
          context.fill('evenodd');
        }
        context.strokeStyle = outlineColor;
        context.lineWidth = member ? 2 : 1;
        context.stroke();
      }
      context.lineWidth = 1;
      if (part.first && face.pattern.kind === 'grid') drawBars(context, face, outlineColor);
      if (part.first && hasRelief(object)) drawRelief(context, object, face, !part.edges ? (hole ? [corners, hole] : [corners]) : null, outlineColor);
      context.globalAlpha = 1;
    }
  }

  function faceAxes(face) {
    const [a, b, , d] = face.vertices;
    return { origin: a, u: normalize(sub(b, a)), v: normalize(sub(d, a)), width: Math.hypot(...sub(b, a)), height: Math.hypot(...sub(d, a)) };
  }

  function openingOutline(face) {
    const { origin, u, v } = faceAxes(face);
    const [cu, cv, hu, hv, round] = face.pattern.values;
    const points = round
      ? Array.from({ length: RING_SEGMENTS }, (_, i) => [cu + hu * Math.cos(2 * Math.PI * i / RING_SEGMENTS), cv + hv * Math.sin(2 * Math.PI * i / RING_SEGMENTS)])
      : [[cu - hu, cv - hv], [cu + hu, cv - hv], [cu + hu, cv + hv], [cu - hu, cv + hv]];
    const projected = points.map(([a, b]) => project(placed(face, add(origin, add(scale(u, a), scale(v, b))))));
    return projected.every(Boolean) ? projected : null;
  }

  function drawBars(context, face, color) {
    const { origin, u, v, width, height } = faceAxes(face);
    const [pu, pv, bu, bv] = face.pattern.values;
    context.strokeStyle = color;
    [[pu, bu, u, v, width, height], [pv, bv, v, u, height, width]].forEach(([period, bar, along, other, span, otherSpan]) => {
      if (!(period > 0) || !(bar > 0)) return;
      const count = Math.floor(span / period);
      const step = Math.max(1, Math.ceil(count / MOST_BARS_DRAWN));
      context.fillStyle = color;
      for (let i = 0; i <= count; i += step) {
        const from = Math.max(0, Math.min(i * period - bar / 2, span));
        const to = Math.max(0, Math.min(i * period + bar / 2, span));
        const start = add(origin, scale(along, from));
        if (face.bend) {
          const middle = add(start, scale(along, (to - from) / 2));
          const pieces = Array.from({ length: BEND_PIECES + 1 }, (_, k) => project(placed(face, add(middle, scale(other, otherSpan * k / BEND_PIECES)))));
          if (pieces.every(Boolean)) {
            context.beginPath();
            pieces.forEach(([x, y], k) => (k ? context.lineTo(x, y) : context.moveTo(x, y)));
            context.stroke();
          }
          continue;
        }
        const strip = [start, add(start, scale(along, to - from))].flatMap(p => [p, add(p, scale(other, otherSpan))]);
        const corners = [strip[0], strip[2], strip[3], strip[1]].map(project);
        if (step > 1 || !corners.every(Boolean) || Math.hypot(corners[1][0] - corners[0][0], corners[1][1] - corners[0][1]) < 1.5) {
          const middle = add(start, scale(along, (to - from) / 2));
          line(context, middle, add(middle, scale(other, otherSpan)));
          continue;
        }
        context.beginPath();
        corners.forEach(([x, y], k) => (k ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.closePath();
        context.fill();
      }
    });
  }

  function drawRelief(context, object, face, outline, color) {
    const normal = faceNormal(face.vertices);
    const relief = faceRelief(object, normal, object.relief.speedMetresPerSecond * reliefTime);
    if (!(relief.range > 0)) return;
    const { along, sideways } = relief;
    const origin = face.vertices[0];
    const coordinates = face.vertices.map(v => [dot(sub(v, origin), along), dot(sub(v, origin), sideways)]);
    const [s0, s1] = [Math.min(...coordinates.map(c => c[0])), Math.max(...coordinates.map(c => c[0]))];
    const [t0, t1] = [Math.min(...coordinates.map(c => c[1])), Math.max(...coordinates.map(c => c[1]))];
    const step = relief.finest / 6;
    const ns = Math.max(1, Math.min(MOST_RELIEF_CELLS_ALONG, Math.ceil((s1 - s0) / step)));
    const nt = relief.oneWay ? 1 : Math.max(1, Math.min(Math.floor(MOST_RELIEF_CELLS / ns), Math.ceil((t1 - t0) / step)));
    const point = (s, t) => add(origin, add(scale(along, s), scale(sideways, t)));
    const shown = p => placed(face, p);
    const normalAt = p => bentNormal(face.bend, p)?.map(v => v * Math.sign(dot(normal, face.bend.front))) ?? normal;
    context.save();
    if (outline) {
      context.beginPath();
      outline.forEach(path => path.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y))));
      context.clip('evenodd');
    }
    const ds = (s1 - s0) / ns;
    const dt = (t1 - t0) / nt;
    for (let i = 0; i < ns; i += 1) {
      for (let j = 0; j < nt; j += 1) {
        const s = s0 + (i + 0.5) * ds;
        const t = t0 + (j + 0.5) * dt;
        const h = relief.at(point(s, t)) / relief.range;
        const corners = [[s - ds / 2, t - dt / 2], [s + ds / 2, t - dt / 2], [s + ds / 2, t + dt / 2], [s - ds / 2, t + dt / 2]].map(([a, b]) => project(shown(point(a, b))));
        if (corners.some(c => !c)) continue;
        context.fillStyle = h >= 0 ? `rgba(255, 255, 255, ${(0.35 * h).toFixed(3)})` : `rgba(0, 0, 0, ${(-0.45 * h).toFixed(3)})`;
        context.beginPath();
        corners.forEach(([x, y], k) => (k ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.closePath();
        context.fill();
      }
    }
    context.restore();
    context.strokeStyle = color;
    const samples = Math.min(MOST_PROFILE_POINTS, Math.max(2, Math.ceil((s1 - s0) / (relief.finest / 8))));
    for (const t of [t0 + (t1 - t0) / 4, (t0 + t1) / 2, t1 - (t1 - t0) / 4]) {
      const points = Array.from({ length: samples + 1 }, (_, k) => {
        const p = point(s0 + (s1 - s0) * k / samples, t);
        return project(add(shown(p), scale(normalAt(p), relief.at(p))));
      });
      if (points.some(p => !p)) continue;
      context.beginPath();
      points.forEach(([x, y], k) => (k ? context.lineTo(x, y) : context.moveTo(x, y)));
      context.stroke();
    }
  }

  function arrowHead(context, from, to) {
    const angle = Math.atan2(to[1] - from[1], to[0] - from[0]);
    context.beginPath();
    context.moveTo(to[0], to[1]);
    context.lineTo(to[0] - ARROWHEAD_PIXELS * Math.cos(angle - 0.4), to[1] - ARROWHEAD_PIXELS * Math.sin(angle - 0.4));
    context.lineTo(to[0] - ARROWHEAD_PIXELS * Math.cos(angle + 0.4), to[1] - ARROWHEAD_PIXELS * Math.sin(angle + 0.4));
    context.closePath();
    context.fill();
  }

  function isHot(handle) {
    const live = drag?.handle ?? hover?.handle;
    return live && live.id === handle.id;
  }

  function moveHandles(context, colors, centre, target, axes = WORLD_AXES, startPixels = POINT_ARROW_START_PIXELS) {
    const c = project(centre);
    if (!c) return;
    axes.forEach((axis, index) => {
      const handle = { id: `move-${target}-${index}`, type: 'move', target, axis: index, vector: axis };
      const a = project(add(centre, scale(axis, pixelsToMetres(startPixels, centre))));
      const b = project(add(centre, scale(axis, pixelsToMetres(startPixels + MOVE_ARROW_PIXELS, centre))));
      if (!a || !b) return;
      const along = [b[0] - a[0], b[1] - a[1]];
      const length = Math.hypot(...along);
      if (length < MOVE_ARROW_SHORTEST_PIXELS) return;
      const u = [along[0] / length, along[1] / length];
      const v = [-u[1], u[0]];
      const at = (s, w) => [a[0] + u[0] * s + v[0] * w, a[1] + u[1] * s + v[1] * w];
      const neck = Math.max(0, length - MOVE_HEAD_PIXELS);
      const outline = [at(0, -MOVE_SHAFT_PIXELS), at(neck, -MOVE_SHAFT_PIXELS), at(neck, -MOVE_HEAD_HALF_PIXELS), at(length, 0), at(neck, MOVE_HEAD_HALF_PIXELS), at(neck, MOVE_SHAFT_PIXELS), at(0, MOVE_SHAFT_PIXELS)];
      const hot = isHot(handle);
      const color = hot ? colors.accent : colors.ink;
      const fade = context.createLinearGradient(a[0], a[1], b[0], b[1]);
      fade.addColorStop(0, withAlpha(color, 0));
      fade.addColorStop(0.45, withAlpha(color, hot ? 0.9 : 0.55));
      fade.addColorStop(1, withAlpha(color, 1));
      context.beginPath();
      outline.forEach(([x, y], k) => (k ? context.lineTo(x, y) : context.moveTo(x, y)));
      context.closePath();
      context.fillStyle = withAlpha(color, hot ? 0.18 : 0.06);
      context.fill();
      context.strokeStyle = fade;
      context.lineWidth = hot ? 1.8 : 1.2;
      context.stroke();
      context.lineWidth = 1;
      handles.push({ ...handle, segment: [a, b], reach: MOVE_HEAD_HALF_PIXELS + 2 });
    });
  }

  function sizeHandles(context, colors, object) {
    const { min, max } = objectBounds(object);
    for (let axis = 0; axis < 3; axis += 1) {
      if (object.size[axis] === 0) continue;
      if (object.kind === 'pipe' && axis === 2) continue;
      const symmetric = Math.abs(min[axis] + max[axis]) < 1e-9;
      const sides = symmetric ? [1, -1] : [Math.abs(max[axis]) >= Math.abs(min[axis]) ? 1 : -1];
      for (const side of sides) {
        const local = [0, 1, 2].map(i => (i === axis ? (side > 0 ? max[i] : min[i]) : (min[i] + max[i]) / 2));
        const p = project(surfacePoint(object, local));
        if (!p) continue;
        const handle = { id: `size-${axis}-${side}`, type: 'size', axis, side, symmetric, point: p, vector: [objectFrame(object).across, objectFrame(object).front, objectFrame(object).up][axis] };
        context.fillStyle = isHot(handle) ? colors.accent : colors.ink;
        context.fillRect(p[0] - 4, p[1] - 4, 8, 8);
        handles.push(handle);
      }
    }
  }

  function ringPoints(centre, a, b, radius) {
    return Array.from({ length: RING_SEGMENTS + 1 }, (_, i) => {
      const t = 2 * Math.PI * i / RING_SEGMENTS;
      return project(add(centre, add(scale(a, radius * Math.cos(t)), scale(b, radius * Math.sin(t)))));
    });
  }

  function rotationHandles(context, colors, object, centre) {
    const radius = pixelsToMetres(RING_PIXELS, centre);
    const { across } = objectFrame(object);
    const yaw = object.yawDegrees * Math.PI / 180;
    const heading = [Math.sin(yaw), Math.cos(yaw), 0];
    const rings = [
      { id: 'ring-yaw', type: 'ring', which: 'yaw', points: ringPoints(centre, [1, 0, 0], [0, 1, 0], radius) },
      { id: 'ring-tilt', type: 'ring', which: 'tilt', points: ringPoints(centre, heading, [0, 0, 1], radius * 0.8), across }
    ];
    for (const ring of rings) {
      if (ring.points.some(p => !p)) continue;
      context.strokeStyle = isHot(ring) ? colors.accent : colors.muted;
      context.lineWidth = isHot(ring) ? 2 : 1;
      context.setLineDash(ring.which === 'tilt' ? [4, 3] : []);
      context.beginPath();
      ring.points.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
      context.stroke();
      context.setLineDash([]);
      context.lineWidth = 1;
      handles.push(ring);
    }
  }

  let drawRequested = false;
  function draw() {
    if (drawRequested) return;
    drawRequested = true;
    requestAnimationFrame(() => {
      drawRequested = false;
      drawNow();
    });
  }

  function drawNow() {
    const fitted = fitCanvas(canvas);
    const { context } = fitted;
    size = { width: fitted.width, height: fitted.height };
    handles = [];
    if (!space) return;
    const colors = palette();
    const style = getComputedStyle(document.documentElement);
    const fill = style.getPropertyValue('--surface').trim();
    const edge = style.getPropertyValue('--surface-edge').trim();
    const sourceColor = style.getPropertyValue('--source').trim();

    if (showGrid()) drawGrid(context, colors);

    drawFaces(context, colors, fill, edge);

    if (space.source.kind === 'far') {
      const centre = listeningCentre();
      const reach = pixelsToMetres(ARROW_PIXELS * 1.6, centre);
      const arrival = arrivalVector(space.arrival);
      const from = project(add(centre, scale(arrival, reach)));
      const to = project(add(centre, scale(arrival, reach * ARRIVAL_STOP)));
      if (from && to) {
        const handle = { id: 'arrival', type: 'arrival', point: from };
        const color = isHot(handle) ? colors.accent : colors.direct;
        context.strokeStyle = color;
        context.fillStyle = color;
        context.lineWidth = 2;
        context.beginPath();
        context.moveTo(from[0], from[1]);
        context.lineTo(to[0], to[1]);
        context.stroke();
        context.lineWidth = 1;
        arrowHead(context, from, to);
        context.beginPath();
        context.arc(from[0], from[1], 5, 0, 2 * Math.PI);
        context.fill();
        handles.push(handle);
      }
    } else {
      const source = space.source.position;
      context.strokeStyle = sourceColor;
      context.globalAlpha = 0.5;
      context.setLineDash([2, 4]);
      [...space.microphones, ...(space.rigs ?? [])].forEach(m => line(context, source, m.position));
      context.setLineDash([]);
      context.globalAlpha = 1;
    }

    const selectedPoint = grouped() ? null : points().find(isSelected);
    for (const item of points()) {
      const p = project(item.point.position);
      if (!p) continue;
      const active = isSelected(item);
      const color = item.type === 'source' ? sourceColor : colors.mics[item.index % colors.mics.length] || colors.receiver;
      context.globalAlpha = focusing() && !active ? 0.5 : 1;
      context.fillStyle = color;
      context.strokeStyle = color;
      if (item.type === 'rig') {
        drawRig(context, item.point, color);
        context.fillStyle = color;
        context.fillRect(p[0] - 4, p[1] - 4, 8, 8);
      } else {
        context.beginPath();
        context.arc(p[0], p[1], item.type === 'source' ? 6 : 5, 0, 2 * Math.PI);
        context.fill();
      }
      if (item.type === 'source') {
        for (const radius of [10, 14]) {
          context.beginPath();
          context.arc(p[0], p[1], radius, -0.6, 0.6);
          context.stroke();
          context.beginPath();
          context.arc(p[0], p[1], radius, Math.PI - 0.6, Math.PI + 0.6);
          context.stroke();
        }
      }
      if (active) {
        context.strokeStyle = colors.accent;
        context.lineWidth = 2;
        context.beginPath();
        context.arc(p[0], p[1], item.type === 'source' ? 7 : 6, 0, 2 * Math.PI);
        context.stroke();
        context.lineWidth = 1;
      }
      if (item.type === 'microphone' && item.point.pattern && item.point.pattern !== 'omni') {
        const facing = aimVector(space, item.point);
        const reach = pixelsToMetres(AIM_PIXELS, item.point.position);
        const ends = line(context, item.point.position, add(item.point.position, scale(facing, reach)));
        if (ends) arrowHead(context, ends[0], ends[1]);
        if (item.point.pattern === 'figure8') {
          context.setLineDash([2, 3]);
          line(context, item.point.position, sub(item.point.position, scale(facing, reach * 0.8)));
          context.setLineDash([]);
        }
      }
      if (item.type === 'microphone' && (active || space.microphones.length === 1)) gapLabel(context, colors, item.point, p);
      context.globalAlpha = 1;
    }
    const aimed = selectedPoint?.type === 'rig' || (selectedPoint?.type === 'microphone' && selectedPoint.point.pattern !== 'omni');
    if (aimed) {
      const centre = selectedPoint.point.position;
      const facing = aimVector(space, selectedPoint.point);
      const level = Math.hypot(facing[0], facing[1]);
      const heading = level > 0.01 ? [facing[0] / level, facing[1] / level, 0] : [0, 1, 0];
      const rings = [
        { id: 'ring-aim', type: 'aim', which: 'yaw', key: selectedPoint.key, points: ringPoints(centre, [1, 0, 0], [0, 1, 0], pixelsToMetres(AIM_RING_PIXELS, centre)) },
        { id: 'ring-aim-tilt', type: 'aim', which: 'tilt', key: selectedPoint.key, heading, points: ringPoints(centre, heading, [0, 0, 1], pixelsToMetres(AIM_RING_PIXELS * 0.8, centre)) }
      ];
      for (const ring of rings) {
        if (ring.points.some(p => !p)) continue;
        context.strokeStyle = isHot(ring) ? colors.accent : colors.muted;
        context.lineWidth = isHot(ring) ? 2 : 1;
        context.setLineDash(ring.which === 'tilt' ? [4, 3] : [1, 3]);
        context.beginPath();
        ring.points.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.stroke();
        context.setLineDash([]);
        context.lineWidth = 1;
        handles.push(ring);
      }
    }
    if (selectedPoint) {
      const reference = referenceObject(space, selectedPoint.point);
      const start = aimed ? AIM_RING_PIXELS + 3 : POINT_ARROW_START_PIXELS;
      if (reference) {
        const { across, front, up } = pointFrame(reference, selectedPoint.point);
        moveHandles(context, colors, selectedPoint.point.position, selectedPoint.key, [across, front, up], start);
      } else {
        moveHandles(context, colors, selectedPoint.point.position, selectedPoint.key, WORLD_AXES, start);
      }
    }

    const activeId = activeObjectId();
    const object = space.objects.find(o => o.id === activeId);
    if (object) {
      const centre = objectCentre(object);
      rotationHandles(context, colors, object, centre);
      sizeHandles(context, colors, object);
      const { across, front, up } = objectFrame(object);
      moveHandles(context, colors, centre, 'object', ownAxes() ? [across, front, up] : WORLD_AXES, RING_PIXELS + 3);
    }

    if (grouped()) {
      const centre = groupCentre();
      if (centre) {
        const radius = pixelsToMetres(RING_PIXELS, centre);
        const ring = { id: 'ring-group', type: 'ring', which: 'yaw', group: true, points: ringPoints(centre, [1, 0, 0], [0, 1, 0], radius) };
        if (!ring.points.some(p => !p)) {
          context.strokeStyle = isHot(ring) ? colors.accent : colors.muted;
          context.lineWidth = isHot(ring) ? 2 : 1;
          context.beginPath();
          ring.points.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
          context.stroke();
          context.lineWidth = 1;
          handles.push(ring);
        }
        moveHandles(context, colors, centre, 'group', WORLD_AXES, RING_PIXELS + 3);
      }
      const count = grouped().objects.length + grouped().points.length;
      context.fillStyle = colors.accent;
      context.font = '11px system-ui, sans-serif';
      context.fillText(`▣ ${count}`, 70, size.height - 10);
    }

    if (drag?.marquee) {
      const [a, b] = [drag.from, drag.to];
      context.strokeStyle = colors.accent;
      context.setLineDash([4, 3]);
      context.strokeRect(Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1]));
      context.setLineDash([]);
    }

    if (readout) {
      context.font = '12px system-ui, sans-serif';
      const width = context.measureText(readout.text).width + 10;
      context.fillStyle = colors.panel;
      context.fillRect(readout.x + 12, readout.y - 26, width, 18);
      context.fillStyle = colors.ink;
      context.fillText(readout.text, readout.x + 17, readout.y - 13);
    }
  }

  function drawGrid(context, colors) {
    const reach = Math.min(GRID_MOST_METRES, Math.max(GRID_LEAST_METRES, Math.ceil(camera.distance * 3)));
    const cx = Math.round(camera.target[0]);
    const cy = Math.round(camera.target[1]);
    const lines = (step, extent, color) => {
      context.strokeStyle = color;
      const count = Math.round(extent / step);
      const x0 = Math.round(cx / step) * step;
      const y0 = Math.round(cy / step) * step;
      context.beginPath();
      for (let i = -count; i <= count; i += 1) {
        segment(context, [x0 + i * step, y0 - extent, 0], [x0 + i * step, y0 + extent, 0]);
        segment(context, [x0 - extent, y0 + i * step, 0], [x0 + extent, y0 + i * step, 0]);
      }
      context.stroke();
    };
    const fine = camera.distance < GRID_FINE_DISTANCE_METRES;
    if (fine) {
      context.globalAlpha = 0.45;
      lines(0.1, Math.min(reach, GRID_FINE_REACH_METRES), colors.grid);
      context.globalAlpha = 1;
    }
    lines(1, reach, colors.grid);
    context.fillStyle = colors.faint;
    context.font = '11px system-ui, sans-serif';
    context.fillText(fine ? '▦ 1 m · 10 cm' : '▦ 1 m', 10, size.height - 10);
  }

  function drawRig(context, rig, color) {
    const centre = rig.position;
    const { front, right } = rigFrame(space, rig);
    const c = project(centre);
    if (!c) return;
    context.strokeStyle = color;
    context.fillStyle = color;
    context.font = '10px system-ui, sans-serif';
    const tip = (point, direction, pixels) => project(add(point, scale(direction, pixelsToMetres(pixels, point))));
    const label = (text, at, from) => {
      const dx = at[0] - from[0];
      const dy = at[1] - from[1];
      const n = Math.hypot(dx, dy) || 1;
      context.fillText(text, at[0] + dx / n * 6 - 3, at[1] + dy / n * 6 + 3);
    };
    if (rig.kind === 'binaural') {
      const radius = Math.max(HEAD_LEAST_PIXELS, headWidth(rig) / 2 / pixelsToMetres(1, centre));
      context.lineWidth = 1.5;
      context.beginPath();
      context.arc(c[0], c[1], radius, 0, 2 * Math.PI);
      context.stroke();
      const nose = tip(centre, front, radius + HEAD_NOSE_PIXELS);
      const base = tip(centre, front, radius - 1);
      if (nose && base) {
        context.lineWidth = 2.5;
        context.beginPath();
        context.moveTo(base[0], base[1]);
        context.lineTo(nose[0], nose[1]);
        context.stroke();
        arrowHead(context, base, nose);
      }
      context.lineWidth = 1;
      for (const [name, side] of [['L', -1], ['R', 1]]) {
        const ear = tip(centre, right.map(v => v * side), radius);
        if (!ear) continue;
        context.beginPath();
        context.arc(ear[0], ear[1], 3, 0, 2 * Math.PI);
        context.fill();
        label(name, ear, c);
      }
      return;
    }
    const front0 = tip(centre, front, 6);
    const front1 = tip(centre, front, RIG_FRONT_PIXELS);
    if (front0 && front1) {
      context.lineWidth = 3;
      context.globalAlpha *= 0.6;
      context.beginPath();
      context.moveTo(front0[0], front0[1]);
      context.lineTo(front1[0], front1[1]);
      context.stroke();
      arrowHead(context, front0, front1);
      context.globalAlpha /= 0.6;
      context.lineWidth = 1;
    }
    for (const member of rigMembers(space, rig)) {
      const m = project(member.position);
      if (!m) continue;
      const together = Math.hypot(m[0] - c[0], m[1] - c[1]) < 2;
      if (!together) line(context, centre, member.position);
      context.beginPath();
      context.arc(m[0], m[1], 3, 0, 2 * Math.PI);
      context.fill();
      if (member.pattern !== 'omni') {
        const end = tip(member.position, member.facing, AIM_PIXELS);
        if (end) {
          context.beginPath();
          context.moveTo(m[0], m[1]);
          context.lineTo(end[0], end[1]);
          context.stroke();
          arrowHead(context, m, end);
          label(member.name, end, m);
          continue;
        }
      }
      label(member.name, m, together ? (tip(centre, front, -10) ?? c) : c);
    }
  }

  function gapLabel(context, colors, point, p) {
    const reference = referenceObject(space, point);
    context.fillStyle = colors.muted;
    context.font = '11px system-ui, sans-serif';
    if (reference) {
      const { front } = pointFrame(reference, point);
      const gap = point.offset[1];
      context.setLineDash([3, 3]);
      context.strokeStyle = colors.muted;
      line(context, point.position, sub(point.position, scale(front, gap)));
      context.setLineDash([]);
      context.fillText(`⟂ ${formatGap(gap)}`, p[0] + 9, p[1] + 14);
    } else {
      const nearest = nearestSurface(space, point.position);
      if (nearest) context.fillText(formatGap(Math.abs(nearest.distance)), p[0] + 9, p[1] + 14);
    }
  }

  function pointAt(screen) {
    if (!space) return null;
    const list = points();
    for (let i = list.length - 1; i >= 0; i -= 1) {
      const p = project(list[i].point.position);
      if (p && Math.hypot(screen[0] - p[0], screen[1] - p[1]) < HANDLE_PIXELS * 1.5) return list[i];
    }
    return null;
  }

  function handleAt(point) {
    const onPoint = pointAt(point);
    const unselectedPoint = onPoint && !isSelected(onPoint);
    for (let i = handles.length - 1; i >= 0; i -= 1) {
      const h = handles[i];
      if (unselectedPoint && h.type !== 'arrival') continue;
      if (h.segment && distanceToSegment(point, h.segment[0], h.segment[1]) < (h.reach ?? HANDLE_PIXELS)) return h;
      if (h.point && Math.hypot(point[0] - h.point[0], point[1] - h.point[1]) < HANDLE_PIXELS) return h;
      if (h.points) {
        for (let k = 1; k < h.points.length; k += 1) {
          if (distanceToSegment(point, h.points[k - 1], h.points[k]) < HANDLE_PIXELS * 0.75) return h;
        }
      }
    }
    return null;
  }

  function itemAt(point) {
    const onPoint = pointAt(point);
    if (onPoint) return onPoint.type === 'source' ? { type: 'source' } : { type: onPoint.type, id: onPoint.id };
    for (let i = faces.length - 1; i >= 0; i -= 1) {
      if (insidePolygon(point, faces[i].corners)) return { type: 'object', id: faces[i].object.id };
    }
    return null;
  }

  function planePoint(centre, normal, point) {
    const r = ray(...point);
    const along = dot(normal, r.direction);
    if (Math.abs(along) < 1e-6) return null;
    const s = dot(normal, sub(centre, r.origin)) / along;
    return s > 0 ? add(r.origin, scale(r.direction, s)) : null;
  }

  function startDrag(handle, point) {
    if (handle.type === 'aim') {
      const microphone = findPoint(space, handle.key);
      const centre = [...microphone.position];
      const facing = aimVector(space, microphone);
      const start = { point, azimuth: degrees(Math.atan2(facing[0], facing[1])), elevation: degrees(Math.asin(Math.max(-1, Math.min(1, facing[2])))) };
      start.angle = aimAngle(handle, centre, point) ?? 0;
      return { handle, snapshot: null, centre, start, bound: true, aim: true };
    }
    if (handle.target === 'group' || handle.group) {
      const centre = groupCentre();
      const start = { point };
      if (handle.type === 'move') {
        start.axis = handle.vector;
        start.screen = screenAxis(centre, handle.vector);
      } else {
        const p = planePoint(centre, [0, 0, 1], point);
        start.angle = p ? degrees(Math.atan2(p[0] - centre[0], p[1] - centre[1])) : 0;
      }
      return { handle, snapshot: groupSnapshot(), centre, start, group: true };
    }
    const object = space.objects.find(o => o.id === activeObjectId());
    const bound = handle.target !== 'object' && handle.type === 'move' ? findPoint(space, handle.target) : null;
    const snapshot = JSON.parse(JSON.stringify(bound ?? (handle.type === 'arrival' ? { arrival: space.arrival } : object)));
    const centre = bound ? bound.position : handle.type === 'arrival' ? listeningCentre() : object ? objectCentre(object) : null;
    const start = { point };
    if (handle.type === 'size') start.offsets = new Map(boundPoints(space).filter(p => p.reference === object.id).map(p => [p, [...p.offset]]));
    if (handle.type === 'move' || handle.type === 'size') {
      start.axis = handle.vector;
      start.screen = screenAxis(centre, handle.vector);
    }
    if (handle.type === 'ring') {
      start.centre = centre;
      const p = planePoint(centre, handle.which === 'yaw' ? [0, 0, 1] : handle.across, point);
      start.angle = p ? ringAngle(handle, object, centre, p) : 0;
    }
    return { handle, snapshot, centre, start, objectId: object?.id, bound: Boolean(bound) };
  }

  function aimAngle(handle, centre, point) {
    if (handle.which === 'tilt') {
      const h = handle.heading;
      const p = planePoint(centre, [h[1], -h[0], 0], point);
      if (!p) return null;
      const d = sub(p, centre);
      return degrees(Math.atan2(d[2], d[0] * h[0] + d[1] * h[1]));
    }
    const p = planePoint(centre, [0, 0, 1], point);
    return p ? degrees(Math.atan2(p[0] - centre[0], p[1] - centre[1])) : null;
  }

  function screenAxis(centre, axis) {
    const length = pixelsToMetres(ARROW_PIXELS, centre);
    const a = project(centre);
    const b = project(add(centre, scale(axis, length)));
    if (!a || !b) return null;
    const d = [b[0] - a[0], b[1] - a[1]];
    const pixels = Math.hypot(...d);
    return pixels < 6 ? null : { unit: [d[0] / pixels, d[1] / pixels], metresPerPixel: length / pixels };
  }

  function alongAxis(start, point) {
    if (!start.screen) return 0;
    const moved = [point[0] - start.point[0], point[1] - start.point[1]];
    return (moved[0] * start.screen.unit[0] + moved[1] * start.screen.unit[1]) * start.screen.metresPerPixel;
  }

  function surfaceSnap(objectId, axis, candidate, base) {
    const object = space.objects.find(o => o.id === objectId);
    const shift = candidate - base;
    const own = expandObject(object).flatMap(f => f.vertices).map(v => v[axis] + shift);
    const [lo, hi] = [Math.min(...own), Math.max(...own)];
    const reach = pixelsToMetres(SURFACE_SNAP_PIXELS, object.position);
    let best = null;
    for (const other of space.objects) {
      if (other.id === objectId) continue;
      const values = expandObject(other).flatMap(f => f.vertices).map(v => v[axis]);
      for (const target of [Math.min(...values), Math.max(...values)]) {
        for (const edge of [lo, hi]) {
          const difference = target - edge;
          if (Math.abs(difference) < reach && (!best || Math.abs(difference) < Math.abs(best))) best = difference;
        }
      }
    }
    return best === null ? candidate : candidate + best;
  }

  function ringAngle(handle, object, centre, p) {
    const d = sub(p, centre);
    if (handle.which === 'yaw') return degrees(Math.atan2(d[0], d[1]));
    const yaw = object.yawDegrees * Math.PI / 180;
    return degrees(Math.atan2(d[2], d[0] * Math.sin(yaw) + d[1] * Math.cos(yaw)));
  }

  function dragTo(point, keys) {
    const free = drag.handle.type === 'size' ? keys.command : keys.shift;
    const { handle, snapshot, centre, start } = drag;
    const step = gridSpacing() / SNAP_PER_GRID;
    if (drag.aim && handle.which === 'tilt') {
      const angle = aimAngle(handle, centre, point);
      if (angle === null) return;
      const turned = snap(start.elevation + angle - start.angle, ANGLE_SNAP_DEGREES, free);
      const e = turned * Math.PI / 180;
      readout = { x: point[0], y: point[1], text: `Tilt ${Math.round(degrees(Math.asin(Math.sin(e))))}°` };
      const h = handle.heading;
      onEdit(next => aimAt(next, findPoint(next, handle.key), [h[0] * Math.cos(e), h[1] * Math.cos(e), Math.sin(e)]));
      return;
    }
    if (drag.aim) {
      const angle = aimAngle(handle, centre, point);
      if (angle === null) return;
      const turned = angle - start.angle;
      const azimuth = wrap(snap(start.azimuth + turned, ANGLE_SNAP_DEGREES, free));
      readout = { x: point[0], y: point[1], text: `↻ ${Math.round(azimuth)}°` };
      onEdit(next => {
        const microphone = findPoint(next, handle.key);
        const current = aimVector(next, microphone);
        const flat = Math.hypot(current[0], current[1]) < 0.01;
        const level = flat ? 1 : Math.hypot(current[0], current[1]);
        const a = azimuth * Math.PI / 180;
        aimAt(next, microphone, [level * Math.sin(a), level * Math.cos(a), flat ? 0 : current[2]]);
      });
      return;
    }
    if (drag.group) {
      if (handle.type === 'move') {
        const delta = snap(alongAxis(start, point), step, free);
        readout = { x: point[0], y: point[1], text: `${AXIS_NAMES[handle.axis]} ${delta >= 0 ? '+' : '−'}${formatGap(Math.abs(delta))}` };
        onEdit(next => moveGroup(next, snapshot, p => add(p, scale(handle.vector, delta)), 0));
      } else {
        const p = planePoint(centre, [0, 0, 1], point);
        if (!p) return;
        const turn = snap(degrees(Math.atan2(p[0] - centre[0], p[1] - centre[1])) - start.angle, ANGLE_SNAP_DEGREES, free);
        const r = turn * Math.PI / 180;
        const [cos, sin] = [Math.cos(r), Math.sin(r)];
        readout = { x: point[0], y: point[1], text: `↻ ${Math.round(turn)}°` };
        onEdit(next => moveGroup(next, snapshot, q => {
          const dx = q[0] - centre[0];
          const dy = q[1] - centre[1];
          return [centre[0] + dx * cos + dy * sin, centre[1] - dx * sin + dy * cos, q[2]];
        }, turn));
      }
      return;
    }
    if (handle.type === 'move' && drag.bound && snapshot.reference) {
      const value = snap(snapshot.offset[handle.axis] + alongAxis(start, point), step, free);
      readout = { x: point[0], y: point[1], text: `${OFFSET_NAMES[handle.axis]} ${formatGap(value)}` };
      onEdit(next => { findPoint(next, handle.target).offset[handle.axis] = value; });
    } else if (handle.type === 'move' && !drag.bound && ownAxes()) {
      const delta = snap(alongAxis(start, point), step, free);
      readout = { x: point[0], y: point[1], text: `${OWN_AXIS_NAMES[handle.axis]} ${delta >= 0 ? '+' : '−'}${formatGap(Math.abs(delta))}` };
      onEdit(next => { next.objects.find(o => o.id === drag.objectId).position = add(snapshot.position, scale(handle.vector, delta)); });
    } else if (handle.type === 'move') {
      let value = snap(snapshot.position[handle.axis] + alongAxis(start, point), step, free);
      if (!drag.bound && snapSurfaces() && !free) value = surfaceSnap(drag.objectId, handle.axis, value, space.objects.find(o => o.id === drag.objectId).position[handle.axis]);
      readout = { x: point[0], y: point[1], text: `${AXIS_NAMES[handle.axis]} ${formatGap(value)}` };
      onEdit(next => {
        const target = drag.bound ? findPoint(next, handle.target).position : next.objects.find(o => o.id === drag.objectId).position;
        target[handle.axis] = value;
      });
    } else if (handle.type === 'size') {
      const delta = alongAxis(start, point) * handle.side;
      const length = Math.max(0.01, snap(snapshot.size[handle.axis] + delta, step, free));
      const ratio = length / snapshot.size[handle.axis];
      const size = keys.shift ? snapshot.size.map(v => v * ratio) : snapshot.size.map((v, i) => (i === handle.axis ? length : v));
      if (snapshot.kind === 'pipe' && handle.axis === 0) size[2] = size[0];
      const keep = [0, 1, 2].map(i => keys.alt || i !== handle.axis);
      const side = [0, 1, 2].map(i => (i === handle.axis ? handle.side : 1));
      readout = { x: point[0], y: point[1], text: `${SIZE_NAMES[handle.axis]} ${formatGap(length)}${keys.shift ? ` · ×${ratio.toFixed(2)}` : ''}` };
      onEdit(next => {
        const object = next.objects.find(o => o.id === drag.objectId);
        object.size = size;
        object.position = resizedPosition(snapshot, size, keep, side);
        followResize(next, snapshot, object, start.offsets);
      });
    } else if (handle.type === 'ring') {
      const object = space.objects.find(o => o.id === drag.objectId);
      const p = planePoint(start.centre, handle.which === 'yaw' ? [0, 0, 1] : handle.across, point);
      if (!p) return;
      const delta = ringAngle(handle, object, start.centre, p) - start.angle;
      if (handle.which === 'yaw') {
        const value = wrap(snap(snapshot.yawDegrees + delta, ANGLE_SNAP_DEGREES, free));
        readout = { x: point[0], y: point[1], text: `↻ ${Math.round(value)}°` };
        onEdit(next => { next.objects.find(o => o.id === drag.objectId).yawDegrees = value; });
      } else {
        const value = signedTurn(snap(snapshot.tiltDegrees + delta, ANGLE_SNAP_DEGREES, free));
        readout = { x: point[0], y: point[1], text: `⤢ ${Math.round(value)}°` };
        onEdit(next => { next.objects.find(o => o.id === drag.objectId).tiltDegrees = value; });
      }
    } else if (handle.type === 'arrival') {
      const microphone = listeningCentre();
      const r = ray(...point);
      const radius = Math.hypot(...sub(drag.start.tail ?? arrivalTail(), microphone));
      const oc = sub(r.origin, microphone);
      const b = dot(oc, r.direction);
      const disc = b * b - (dot(oc, oc) - radius * radius);
      const s = disc >= 0 ? -b - Math.sqrt(disc) : -b;
      const direction = normalize(sub(add(r.origin, scale(r.direction, s)), microphone));
      const azimuth = wrap(snap(degrees(Math.atan2(direction[0], direction[1])), ANGLE_SNAP_DEGREES, free));
      const elevation = snap(degrees(Math.asin(Math.max(-1, Math.min(1, direction[2])))), ANGLE_SNAP_DEGREES, free);
      readout = { x: point[0], y: point[1], text: `${Math.round(azimuth)}° · ${Math.round(elevation)}°` };
      onEdit(next => { next.arrival.azimuthDegrees = azimuth; next.arrival.elevationDegrees = elevation; });
    }
  }

  function arrivalTail() {
    const microphone = listeningCentre();
    return add(microphone, scale(arrivalVector(space.arrival), pixelsToMetres(ARROW_PIXELS * 1.6, microphone)));
  }

  function asGroup(current) {
    if (!current) return { type: 'group', objects: [], points: [] };
    if (current.type === 'group') return { type: 'group', objects: [...current.objects], points: [...current.points] };
    if (current.type === 'object') return { type: 'group', objects: [current.id], points: [] };
    return { type: 'group', objects: [], points: [itemKey(current)] };
  }

  function simplified(group) {
    const count = group.objects.length + group.points.length;
    if (count === 0) return null;
    if (count > 1) return group;
    if (group.objects.length) return { type: 'object', id: group.objects[0] };
    const key = group.points[0];
    if (key === 'source') return { type: 'source' };
    const [type, id] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)];
    return { type, id };
  }

  function toggled(item) {
    if (!item) return selection;
    const group = asGroup(selection);
    const toggle = (list, value) => (list.includes(value) ? list.filter(v => v !== value) : [...list, value]);
    if (item.type === 'object') group.objects = toggle(group.objects, item.id);
    else group.points = toggle(group.points, itemKey(item));
    return simplified(group);
  }

  function areaSelection(a, b, adding) {
    const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
    const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
    const inside = position => {
      const p = project(position);
      return Boolean(p) && p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1;
    };
    const group = adding ? asGroup(selection) : asGroup(null);
    space.objects.forEach(o => { if (inside(objectCentre(o)) && !group.objects.includes(o.id)) group.objects.push(o.id); });
    points().forEach(item => { if (inside(item.point.position) && !group.points.includes(item.key)) group.points.push(item.key); });
    return simplified(group);
  }

  const pointer = event => {
    const rect = canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

  const touches = new Map();
  let pinch = null;

  function pinchState() {
    const [a, b] = [...touches.values()];
    return { x: (a[0] + b[0]) / 2, y: (a[1] + b[1]) / 2, distance: Math.hypot(a[0] - b[0], a[1] - b[1]) };
  }

  function panBy(dx, dy) {
    const { right, up } = basis();
    const metres = pixelsToMetres(1, camera.target);
    camera.target = add(camera.target, add(scale(right, -dx * metres), scale(up, dy * metres)));
  }

  function zoomBy(factor) {
    camera.distance = Math.min(60, Math.max(0.1, camera.distance / factor));
  }

  canvas.addEventListener('pointerdown', event => {
    canvas.setPointerCapture(event.pointerId);
    const point = pointer(event);
    if (event.pointerType === 'touch') {
      touches.set(event.pointerId, point);
      if (touches.size === 2) {
        if (drag?.handle) onCommit();
        drag = null;
        readout = null;
        pinch = pinchState();
        return;
      }
      if (touches.size > 2) return;
    }
    const handle = event.button === 0 ? handleAt(point) : null;
    if (handle) {
      drag = startDrag(handle, point);
      if (handle.type === 'arrival') drag.start.tail = arrivalTail();
      if (handle.type !== 'arrival' && !drag.bound && !drag.group && selection?.id !== drag.objectId) onSelect({ type: 'object', id: drag.objectId });
      return;
    }
    if (areaArmed && event.button === 0) {
      drag = { marquee: true, from: point, to: point, moved: false };
      return;
    }
    drag = { orbit: true, pan: event.button !== 0 || event.shiftKey, x: event.clientX, y: event.clientY, yaw: camera.yaw, pitch: camera.pitch, target: [...camera.target], moved: false };
  });

  canvas.addEventListener('pointermove', event => {
    const point = pointer(event);
    if (event.pointerType === 'touch' && touches.has(event.pointerId)) {
      touches.set(event.pointerId, point);
      if (pinch && touches.size === 2) {
        const next = pinchState();
        if (pinch.distance > 0) zoomBy(next.distance / pinch.distance);
        panBy(next.x - pinch.x, next.y - pinch.y);
        pinch = next;
        draw();
        return;
      }
    }
    if (!drag) {
      const handle = space ? handleAt(point) : null;
      const item = handle ? null : space ? itemAt(point) : null;
      const next = handle ? { handle, type: hover?.type, id: hover?.id } : item;
      const key = value => JSON.stringify(value && { h: value.handle?.id, t: value.type, i: value.id });
      if (key(next) !== key(hover)) {
        hover = handle ? { ...(hover ?? {}), handle } : item;
        canvas.style.cursor = handle ? 'pointer' : areaArmed ? 'crosshair' : '';
        draw();
      }
      return;
    }
    if (drag.handle) {
      dragTo(point, { shift: event.shiftKey, alt: event.altKey, command: event.metaKey || event.ctrlKey });
      return;
    }
    if (drag.marquee) {
      drag.to = point;
      drag.moved = true;
      draw();
      return;
    }
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    if (Math.hypot(dx, dy) > CLICK_PIXELS) {
      drag.moved = true;
      canvas.classList.add('dragging');
    }
    if (!drag.moved) return;
    if (drag.pan) {
      const { right, up } = basis();
      const metres = pixelsToMetres(1, camera.target);
      camera.target = add(drag.target, add(scale(right, -dx * metres), scale(up, dy * metres)));
    } else {
      camera.yaw = drag.yaw + dx * 0.008;
      camera.pitch = Math.min(1.5, Math.max(-0.4, drag.pitch + dy * 0.008));
    }
    draw();
  });

  const forget = event => {
    if (event.pointerType !== 'touch') return;
    touches.delete(event.pointerId);
    if (pinch && touches.size < 2) pinch = null;
  };
  canvas.addEventListener('pointercancel', event => {
    forget(event);
    canvas.classList.remove('dragging');
    if (drag?.handle) onCommit();
    drag = null;
    readout = null;
    draw();
  });
  canvas.addEventListener('pointerup', event => {
    const pinching = Boolean(pinch);
    forget(event);
    if (pinching) {
      drag = null;
      return;
    }
    canvas.classList.remove('dragging');
    if (drag?.handle) {
      readout = null;
      onCommit();
    } else if (drag?.marquee) {
      if (drag.moved) onSelect(areaSelection(drag.from, drag.to, event.shiftKey));
      onAreaEnd();
    } else if (drag && !drag.moved && event.shiftKey) {
      onSelect(toggled(itemAt(pointer(event))));
    } else if (drag && !drag.moved) {
      onSelect(itemAt(pointer(event)));
    }
    drag = null;
    draw();
  });
  canvas.addEventListener('pointerleave', () => {
    if (!drag && hover) {
      hover = null;
      draw();
    }
  });
  canvas.addEventListener('contextmenu', event => event.preventDefault());
  canvas.addEventListener('wheel', event => {
    event.preventDefault();
    const { zoomPixels, panX, panY } = wheelGesture(event, scrollPans());
    if (zoomPixels === undefined) panBy(-panX, -panY);
    else zoomBy(Math.exp(-zoomPixels * ZOOM_PER_PIXEL));
    draw();
  }, { passive: false });
  let gestureScale = 1;
  canvas.addEventListener('gesturestart', event => { event.preventDefault(); gestureScale = 1; });
  canvas.addEventListener('gesturechange', event => {
    event.preventDefault();
    zoomBy(event.scale / gestureScale);
    gestureScale = event.scale;
    draw();
  });
  canvas.addEventListener('gestureend', event => event.preventDefault());
  canvas.addEventListener('dblclick', event => {
    const item = itemAt(pointer(event));
    if (item?.type === 'object') camera.target = objectCentre(space.objects.find(o => o.id === item.id));
    if (item?.type === 'microphone') camera.target = [...space.microphones.find(m => m.id === item.id).position];
    if (item?.type === 'source') camera.target = [...space.source.position];
    if (item?.type === 'rig') camera.target = [...space.rigs.find(r => r.id === item.id).position];
    draw();
  });
  new ResizeObserver(() => drawNow()).observe(canvas);

  return {
    update(nextSpace, nextSelection) {
      space = nextSpace;
      selection = nextSelection;
      draw();
    },
    reset() {
      Object.assign(camera, { yaw: -0.6, pitch: 0.45, distance: 6 });
      if (space) camera.target = [...listeningCentre()];
      draw();
    },
    centre(point) {
      camera.target = [...point];
      draw();
    },
    setReliefTime(seconds) {
      reliefTime = seconds;
      draw();
    },
    setAreaSelect(on) {
      areaArmed = on;
      canvas.style.cursor = on ? 'crosshair' : '';
    }
  };
}
