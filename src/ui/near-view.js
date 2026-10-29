import { fitCanvas, palette } from '../shared/ui/canvas.js';
import { arrivalVector } from '../domain/space.js';
import { expandObject, objectBounds, objectFrame, referenceObject, toWorld } from '../domain/objects.js';
import { nearestSurface } from '../domain/placement.js';

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

export function createNearView(canvas, { onSelect, onEdit, onCommit, snapSurfaces = () => false }) {
  const camera = { yaw: -0.6, pitch: 0.45, distance: 6, target: [0, 0, 1] };
  let space = null;
  let selection = null;
  let hover = null;
  let faces = [];
  let handles = [];
  let size = { width: 1, height: 1 };
  let drag = null;
  let readout = null;

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
  const microphoneActive = () => selection?.type === 'microphone';
  const focusing = () => Boolean(drag?.handle) || selection?.type === 'object' || selection?.type === 'microphone';

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
      const highlighted = object.id === active || (hover?.type === 'object' && hover.id === object.id);
      const dim = focusing() && object.id !== active ? DIMMED : 1;
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
      context.lineWidth = object.id === active ? 2 : 1;
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
      for (let i = 0; i <= count; i += step) {
        const start = add(origin, scale(along, i * period + bar / 2));
        line(context, start, add(start, scale(other, otherSpan)));
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

    const microphone = space.microphone.position;
    const reach = pixelsToMetres(ARROW_PIXELS * 1.6, microphone);
    const arrival = arrivalVector(space.arrival);
    const tail = add(microphone, scale(arrival, reach));
    const from = project(tail);
    const to = project(add(microphone, scale(arrival, reach * 0.18)));
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

    const m = project(microphone);
    if (m) {
      const active = microphoneActive();
      const reference = referenceObject(space);
      context.globalAlpha = focusing() && !active ? 0.5 : 1;
      context.fillStyle = colors.receiver;
      context.beginPath();
      context.arc(m[0], m[1], 5, 0, 2 * Math.PI);
      context.fill();
      if (active) {
        context.strokeStyle = colors.accent;
        context.lineWidth = 2;
        context.stroke();
        context.lineWidth = 1;
      }
      if (reference) {
        const { front } = objectFrame(reference);
        const gap = space.microphone.offset[1];
        const foot = sub(microphone, scale(front, gap));
        context.setLineDash([3, 3]);
        context.strokeStyle = colors.muted;
        line(context, microphone, foot);
        context.setLineDash([]);
        context.fillStyle = colors.muted;
        context.font = '11px system-ui, sans-serif';
        context.fillText(`⟂ ${formatGap(gap)}`, m[0] + 9, m[1] + 14);
      } else {
        const nearest = nearestSurface(space);
        if (nearest) {
          context.fillStyle = colors.muted;
          context.font = '11px system-ui, sans-serif';
          context.fillText(formatGap(Math.abs(nearest.distance)), m[0] + 9, m[1] + 14);
        }
      }
      context.globalAlpha = 1;
      if (active) {
        if (reference) {
          const { across, front, up } = objectFrame(reference);
          moveHandles(context, colors, microphone, 'microphone', [across, front, up], OFFSET_NAMES);
        } else {
          moveHandles(context, colors, microphone, 'microphone');
        }
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

    if (readout) {
      context.font = '12px system-ui, sans-serif';
      const width = context.measureText(readout.text).width + 10;
      context.fillStyle = colors.panel;
      context.fillRect(readout.x + 12, readout.y - 26, width, 18);
      context.fillStyle = colors.ink;
      context.fillText(readout.text, readout.x + 17, readout.y - 13);
    }
  }

  function handleAt(point) {
    for (let i = handles.length - 1; i >= 0; i -= 1) {
      const h = handles[i];
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
    const m = space && project(space.microphone.position);
    if (m && Math.hypot(point[0] - m[0], point[1] - m[1]) < HANDLE_PIXELS * 1.5) return { type: 'microphone' };
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
    const object = space.objects.find(o => o.id === activeObjectId());
    const snapshot = JSON.parse(JSON.stringify(handle.target === 'microphone' || handle.type === 'arrival' ? { microphone: space.microphone, arrival: space.arrival } : object));
    const centre = handle.target === 'microphone' ? space.microphone.position : object ? objectCentre(object) : null;
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
    return { handle, snapshot, centre, start, objectId: object?.id };
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
    if (handle.type === 'move' && handle.target === 'microphone' && snapshot.microphone.reference) {
      const value = snap(snapshot.microphone.offset[handle.axis] + alongAxis(start, point), step, free);
      readout = { x: point[0], y: point[1], text: `${OFFSET_NAMES[handle.axis]} ${formatGap(value)}` };
      onEdit(next => { next.microphone.offset[handle.axis] = value; });
    } else if (handle.type === 'move') {
      const base = handle.target === 'microphone' ? snapshot.microphone.position : snapshot.position;
      let value = snap(base[handle.axis] + alongAxis(start, point), step, free);
      if (handle.target === 'object' && snapSurfaces() && !free) value = surfaceSnap(drag.objectId, handle.axis, value, space.objects.find(o => o.id === drag.objectId).position[handle.axis]);
      readout = { x: point[0], y: point[1], text: `${AXIS_NAMES[handle.axis]} ${formatGap(value)}` };
      onEdit(next => {
        const target = handle.target === 'microphone' ? next.microphone.position : next.objects.find(o => o.id === drag.objectId).position;
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
      const microphone = space.microphone.position;
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
    const microphone = space.microphone.position;
    return add(microphone, scale(arrivalVector(space.arrival), pixelsToMetres(ARROW_PIXELS * 1.6, microphone)));
  }

  const pointer = event => {
    const rect = canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

  canvas.addEventListener('pointerdown', event => {
    canvas.setPointerCapture(event.pointerId);
    const point = pointer(event);
    const handle = event.button === 0 ? handleAt(point) : null;
    if (handle) {
      drag = startDrag(handle, point);
      if (handle.type === 'arrival') drag.start.tail = arrivalTail();
      if (handle.target === 'microphone' && selection?.type !== 'microphone') onSelect({ type: 'microphone' });
      else if (handle.type !== 'arrival' && handle.target !== 'microphone' && selection?.id !== drag.objectId) onSelect({ type: 'object', id: drag.objectId });
      return;
    }
    drag = { orbit: true, pan: event.button !== 0 || event.shiftKey, x: event.clientX, y: event.clientY, yaw: camera.yaw, pitch: camera.pitch, target: [...camera.target], moved: false };
  });

  canvas.addEventListener('pointermove', event => {
    const point = pointer(event);
    if (!drag) {
      const handle = space ? handleAt(point) : null;
      const item = handle ? null : space ? itemAt(point) : null;
      const next = handle ? { handle, type: hover?.type, id: hover?.id } : item;
      const key = value => JSON.stringify(value && { h: value.handle?.id, t: value.type, i: value.id });
      if (key(next) !== key(hover)) {
        hover = handle ? { ...(hover ?? {}), handle } : item;
        canvas.style.cursor = handle ? 'pointer' : '';
        draw();
      }
      return;
    }
    if (drag.handle) {
      dragTo(point, event.shiftKey);
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

  canvas.addEventListener('pointerup', event => {
    canvas.classList.remove('dragging');
    if (drag?.handle) {
      readout = null;
      onCommit();
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
    camera.distance = Math.min(60, Math.max(0.1, camera.distance * Math.exp(event.deltaY * 0.001)));
    draw();
  }, { passive: false });
  canvas.addEventListener('dblclick', event => {
    const item = itemAt(pointer(event));
    if (item?.type === 'object') camera.target = objectCentre(space.objects.find(o => o.id === item.id));
    if (item?.type === 'microphone') camera.target = [...space.microphone.position];
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
      if (space) camera.target = [...space.microphone.position];
      draw();
    },
    centre(point) {
      camera.target = [...point];
      draw();
    }
  };
}
