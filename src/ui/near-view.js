import { fitCanvas, palette } from '../shared/ui/canvas.js';
import { arrivalVector } from '../domain/space.js';
import { aimAt, aimVector, expandObject, objectBounds, objectFrame, placePointAt, pointPosition, referenceObject, toWorld } from '../domain/objects.js';
import { nearestSurface } from '../domain/placement.js';
import { rigMembers } from '../domain/rigs.js';

const FIELD_OF_VIEW = 0.9;
const CLICK_PIXELS = 4;
const HANDLE_PIXELS = 8;
const ARROW_PIXELS = 70;
const RING_PIXELS = 58;
const RING_SEGMENTS = 48;
const MOST_BARS_DRAWN = 80;
const GRID_LINES_EACH_SIDE = 8;
const ARROWHEAD_PIXELS = 9;
const ANGLE_SNAP_DEGREES = 5;
const SNAP_PER_GRID = 20;
const WORLD_AXES = Object.freeze([[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
const AXIS_NAMES = Object.freeze(['E', 'N', 'Up']);
const SIZE_NAMES = Object.freeze(['W', 'D', 'H']);
const OFFSET_NAMES = Object.freeze(['↔', '⟂', '↕']);
const DIMMED = 0.3;
const SURFACE_SNAP_PIXELS = 10;
const WHEEL_LINE_PIXELS = 16;
const AIM_PIXELS = 26;

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

export function createNearView(canvas, { onSelect, onEdit, onCommit, onAreaEnd = () => {}, snapSurfaces = () => false }) {
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

  function line(context, a, b) {
    const p = project(a);
    const q = project(b);
    if (!p || !q) return null;
    context.beginPath();
    context.moveTo(p[0], p[1]);
    context.lineTo(q[0], q[1]);
    context.stroke();
    return [p, q];
  }

  function objectCentre(object) {
    const { min, max } = objectBounds(object);
    return toWorld(object, [0, 1, 2].map(i => (min[i] + max[i]) / 2));
  }

  const activeObjectId = () => (selection?.type === 'object' ? selection.id : !selection && hover?.type === 'object' ? hover.id : null);
  const focusing = () => Boolean(drag?.handle) || Boolean(selection?.type);

  // Microphones and a point source share one kind of marker and handles.
  function points() {
    const list = space.microphones.map((point, index) => ({ key: `microphone:${point.id}`, type: 'microphone', id: point.id, index, point }));
    (space.rigs ?? []).forEach((point, index) => list.push({ key: `rig:${point.id}`, type: 'rig', id: point.id, index: space.microphones.length + index, point }));
    if (space.source.kind === 'point') list.push({ key: 'source', type: 'source', point: space.source });
    return list;
  }

  const grouped = () => (selection?.type === 'group' ? selection : null);
  const inGroup = key => Boolean(grouped()?.points.includes(key));
  const objectInGroup = id => Boolean(grouped()?.objects.includes(id));
  const isSelected = item => (selection?.type === item.type && (item.type === 'source' || selection.id === item.id)) || inGroup(item.key);
  const itemKey = item => (item.type === 'source' ? 'source' : item.type === 'microphone' || item.type === 'rig' ? `${item.type}:${item.id}` : null);

  // A group moves and turns as one; points bound to an object in the group
  // follow it, other points keep their own binding at their new place.
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
    const chosen = space.microphones.find(m => selection?.type === 'microphone' && m.id === selection.id) ?? space.microphones[0];
    return chosen.position;
  }

  function drawFaces(context, colors, fill, edge) {
    faces = [];
    space.objects.forEach(object => {
      expandObject(object).forEach((face, faceIndex) => {
        const corners = face.vertices.map(project);
        if (corners.some(c => !c)) return;
        faces.push({ object, face, faceIndex, corners, depth: corners.reduce((s, c) => s + c[2], 0) / corners.length });
      });
    });
    faces.sort((a, b) => b.depth - a.depth);
    const active = activeObjectId();
    for (const { object, face, corners } of faces) {
      const member = object.id === active || objectInGroup(object.id);
      const highlighted = member || (hover?.type === 'object' && hover.id === object.id);
      const dim = focusing() && !member ? DIMMED : 1;
      context.globalAlpha = (object.strength > 0 ? 1 : 0.4) * dim;
      context.beginPath();
      corners.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
      context.closePath();
      const hole = face.pattern.kind === 'opening' ? openingOutline(face) : null;
      if (hole) {
        hole.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
        context.closePath();
      }
      if (face.pattern.kind !== 'grid') {
        context.fillStyle = fill;
        context.fill('evenodd');
      }
      context.strokeStyle = highlighted ? colors.accent : edge;
      context.lineWidth = member ? 2 : 1;
      context.stroke();
      context.lineWidth = 1;
      if (face.pattern.kind === 'grid') drawBars(context, face, highlighted ? colors.accent : edge);
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
    const projected = points.map(([a, b]) => project(add(origin, add(scale(u, a), scale(v, b)))));
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
        const from = Math.min(i * period, span);
        const to = Math.min(i * period + bar, span);
        const start = add(origin, scale(along, from));
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

  function moveHandles(context, colors, centre, target, axes = WORLD_AXES, names = AXIS_NAMES) {
    const length = pixelsToMetres(ARROW_PIXELS, centre);
    axes.forEach((axis, index) => {
      const handle = { id: `move-${target}-${index}`, type: 'move', target, axis: index, vector: axis, length, name: names[index] };
      const color = isHot(handle) ? colors.accent : colors.ink;
      context.strokeStyle = color;
      context.fillStyle = color;
      context.lineWidth = 2;
      const ends = line(context, centre, add(centre, scale(axis, length)));
      context.lineWidth = 1;
      if (!ends) return;
      arrowHead(context, ends[0], ends[1]);
      context.font = '10px system-ui, sans-serif';
      context.fillText(names[index], ends[1][0] + 5, ends[1][1] - 3);
      handles.push({ ...handle, segment: ends });
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
        const p = project(toWorld(object, local));
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

  function draw() {
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

    const spacing = gridSpacing();
    const half = GRID_LINES_EACH_SIDE * spacing;
    const cx = Math.round(camera.target[0] / spacing) * spacing;
    const cy = Math.round(camera.target[1] / spacing) * spacing;
    context.strokeStyle = colors.grid;
    for (let i = -GRID_LINES_EACH_SIDE; i <= GRID_LINES_EACH_SIDE; i += 1) {
      line(context, [cx + i * spacing, cy - half, 0], [cx + i * spacing, cy + half, 0]);
      line(context, [cx - half, cy + i * spacing, 0], [cx + half, cy + i * spacing, 0]);
    }
    context.fillStyle = colors.faint;
    context.font = '11px system-ui, sans-serif';
    context.fillText(`▦ ${formatGap(spacing)}`, 10, size.height - 10);

    drawFaces(context, colors, fill, edge);

    if (space.source.kind === 'far') {
      const centre = listeningCentre();
      const reach = pixelsToMetres(ARROW_PIXELS * 1.6, centre);
      const arrival = arrivalVector(space.arrival);
      const from = project(add(centre, scale(arrival, reach)));
      const to = project(add(centre, scale(arrival, reach * 0.18)));
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
      if (active || space.microphones.length === 1 && item.type === 'microphone') gapLabel(context, colors, item.point, p);
      context.globalAlpha = 1;
    }
    if (selectedPoint?.type === 'rig' || (selectedPoint?.type === 'microphone' && selectedPoint.point.pattern !== 'omni')) {
      const centre = selectedPoint.point.position;
      const ring = { id: 'ring-aim', type: 'aim', key: selectedPoint.key, points: ringPoints(centre, [1, 0, 0], [0, 1, 0], pixelsToMetres(RING_PIXELS * 0.75, centre)) };
      if (!ring.points.some(p => !p)) {
        context.strokeStyle = isHot(ring) ? colors.accent : colors.muted;
        context.lineWidth = isHot(ring) ? 2 : 1;
        context.setLineDash([1, 3]);
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
      if (reference) {
        const { across, front, up } = objectFrame(reference);
        moveHandles(context, colors, selectedPoint.point.position, selectedPoint.key, [across, front, up], OFFSET_NAMES);
      } else {
        moveHandles(context, colors, selectedPoint.point.position, selectedPoint.key);
      }
    }

    const activeId = activeObjectId();
    const object = space.objects.find(o => o.id === activeId);
    if (object) {
      const centre = objectCentre(object);
      rotationHandles(context, colors, object, centre);
      sizeHandles(context, colors, object);
      moveHandles(context, colors, centre, 'object');
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
        moveHandles(context, colors, centre, 'group');
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

  function drawRig(context, rig, color) {
    const centre = rig.position;
    for (const member of rigMembers(space, rig)) {
      context.strokeStyle = color;
      context.fillStyle = color;
      line(context, centre, member.position);
      const m = project(member.position);
      if (!m) continue;
      context.beginPath();
      context.arc(m[0], m[1], 3.5, 0, 2 * Math.PI);
      context.fill();
      const reach = pixelsToMetres(AIM_PIXELS * (member.ear ? 0.6 : 0.8), member.position);
      if (member.ear || member.pattern !== 'omni') {
        const ends = line(context, member.position, add(member.position, scale(member.facing, reach)));
        if (ends) arrowHead(context, ends[0], ends[1]);
      }
      context.font = '10px system-ui, sans-serif';
      context.fillText(member.name, m[0] + 5, m[1] - 5);
    }
  }

  function gapLabel(context, colors, point, p) {
    const reference = referenceObject(space, point);
    context.fillStyle = colors.muted;
    context.font = '11px system-ui, sans-serif';
    if (reference) {
      const { front } = objectFrame(reference);
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
      if (h.segment && distanceToSegment(point, h.segment[0], h.segment[1]) < HANDLE_PIXELS) return h;
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
      return { handle, snapshot: null, centre: [...microphone.position], start: { point }, bound: true, aim: true };
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

  function dragTo(point, free) {
    const { handle, snapshot, centre, start } = drag;
    const step = gridSpacing() / SNAP_PER_GRID;
    if (drag.aim) {
      const p = planePoint(centre, [0, 0, 1], point);
      if (!p) return;
      const azimuth = wrap(snap(degrees(Math.atan2(p[0] - centre[0], p[1] - centre[1])), ANGLE_SNAP_DEGREES, free));
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
      const size = Math.max(0.01, snap(snapshot.size[handle.axis] + delta, step, free));
      const change = size - snapshot.size[handle.axis];
      readout = { x: point[0], y: point[1], text: `${SIZE_NAMES[handle.axis]} ${formatGap(size)}` };
      onEdit(next => {
        const object = next.objects.find(o => o.id === drag.objectId);
        object.size[handle.axis] = size;
        if (object.kind === 'pipe' && handle.axis === 0) object.size[2] = size;
        if (handle.symmetric) {
          object.position = snapshot.position.map((p, i) => p + start.axis[i] * change / 2 * handle.side);
        }
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
        const value = Math.max(-90, Math.min(90, snap(snapshot.tiltDegrees + delta, ANGLE_SNAP_DEGREES, free)));
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
      dragTo(point, event.shiftKey);
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
    const lines = event.deltaMode === 1 ? WHEEL_LINE_PIXELS : 1;
    const deltaX = event.deltaX * lines;
    const deltaY = event.deltaY * lines;
    const mouseWheel = event.deltaMode === 1 || (event.deltaX === 0 && Math.abs(event.deltaY) >= 40 && Number.isInteger(event.deltaY));
    if (event.ctrlKey || mouseWheel) zoomBy(Math.exp(-deltaY * (event.ctrlKey ? 0.01 : 0.001)));
    else panBy(-deltaX, -deltaY);
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
  new ResizeObserver(() => draw()).observe(canvas);

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
    setAreaSelect(on) {
      areaArmed = on;
      canvas.style.cursor = on ? 'crosshair' : '';
    }
  };
}
