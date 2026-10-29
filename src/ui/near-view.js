import { fitCanvas, palette } from '../shared/ui/canvas.js';
import { arrivalVector, surfaceFrame, surfaceVertices } from '../domain/space.js';
import { nearestSurface } from '../domain/placement.js';

const FIELD_OF_VIEW = 0.9;
const CLICK_PIXELS = 4;
const MICROPHONE_PICK_PIXELS = 12;
const MOST_BARS_DRAWN = 80;
const GRID_LINES_EACH_SIDE = 8;
const ARROWHEAD_PIXELS = 9;

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const scale = (a, s) => [a[0] * s, a[1] * s, a[2] * s];

export function createNearView(canvas, { onSelect }) {
  const camera = { yaw: -0.6, pitch: 0.45, distance: 6, target: [0, 0, 1] };
  let space = null;
  let selection = null;
  let projected = [];
  let microphoneOnScreen = null;

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

  function projector(width, height) {
    const { forward, right, up, eye } = basis();
    const focal = height / 2 / Math.tan(FIELD_OF_VIEW / 2);
    return point => {
      const d = sub(point, eye);
      const depth = dot(d, forward);
      if (depth < 0.01) return null;
      return [width / 2 + focal * dot(d, right) / depth, height / 2 - focal * dot(d, up) / depth, depth];
    };
  }

  function line(context, project, a, b) {
    const p = project(a);
    const q = project(b);
    if (!p || !q) return;
    context.beginPath();
    context.moveTo(p[0], p[1]);
    context.lineTo(q[0], q[1]);
    context.stroke();
  }

  function drawBars(context, project, surface) {
    const pattern = surface.pattern;
    if (pattern.kind !== 'grid') return;
    const [corner] = surfaceVertices(surface);
    const { across, up } = surfaceFrame(surface);
    const spans = [surface.widthMetres, surface.heightMetres];
    const axes = [across, up];
    for (let axis = 0; axis < 2; axis += 1) {
      const period = pattern.periodMetres[axis];
      if (!(period > 0) || !(pattern.barMetres[axis] > 0)) continue;
      const count = Math.floor(spans[axis] / period);
      const step = Math.max(1, Math.ceil(count / MOST_BARS_DRAWN));
      const other = axes[1 - axis];
      for (let i = 0; i <= count; i += step) {
        const start = add(corner, scale(axes[axis], i * period + pattern.barMetres[axis] / 2));
        line(context, project, start, add(start, scale(other, spans[1 - axis])));
      }
    }
  }

  function draw() {
    const { context, width, height } = fitCanvas(canvas);
    const colors = palette();
    const style = getComputedStyle(document.documentElement);
    const surfaceFill = style.getPropertyValue('--surface').trim();
    const surfaceEdge = style.getPropertyValue('--surface-edge').trim();
    if (!space) return;
    const project = projector(width, height);

    const spacing = 10 ** Math.floor(Math.log10(camera.distance / 2));
    const half = GRID_LINES_EACH_SIDE * spacing;
    const cx = Math.round(camera.target[0] / spacing) * spacing;
    const cy = Math.round(camera.target[1] / spacing) * spacing;
    context.strokeStyle = colors.grid;
    context.lineWidth = 1;
    for (let i = -GRID_LINES_EACH_SIDE; i <= GRID_LINES_EACH_SIDE; i += 1) {
      line(context, project, [cx + i * spacing, cy - half, 0], [cx + i * spacing, cy + half, 0]);
      line(context, project, [cx - half, cy + i * spacing, 0], [cx + half, cy + i * spacing, 0]);
    }
    context.fillStyle = colors.faint;
    context.font = '11px system-ui, sans-serif';
    context.fillText(`Floor grid ${formatGap(spacing)}`, 10, height - 10);

    projected = space.surfaces.map((surface, index) => {
      const corners = surfaceVertices(surface).map(project);
      if (corners.some(c => !c)) return null;
      return { index, corners, depth: corners.reduce((sum, c) => sum + c[2], 0) / corners.length };
    }).filter(Boolean).sort((a, b) => b.depth - a.depth);

    for (const { index, corners } of projected) {
      const surface = space.surfaces[index];
      const selected = selection?.type === 'surface' && selection.index === index;
      context.beginPath();
      corners.forEach(([x, y], i) => (i ? context.lineTo(x, y) : context.moveTo(x, y)));
      context.closePath();
      context.globalAlpha = surface.strength > 0 ? 1 : 0.4;
      if (surface.pattern.kind === 'solid') {
        context.fillStyle = surfaceFill;
        context.fill();
      }
      context.strokeStyle = selected ? colors.accent : surfaceEdge;
      context.lineWidth = selected ? 2 : 1;
      context.stroke();
      context.lineWidth = 1;
      drawBars(context, project, surface);
      context.globalAlpha = 1;
    }

    const microphone = space.microphone.position;
    const reach = Math.max(0.3, camera.distance * 0.15);
    const arrival = arrivalVector(space.arrival);
    const from = project(add(microphone, scale(arrival, reach * 2)));
    const to = project(add(microphone, scale(arrival, reach * 0.25)));
    if (from && to) {
      context.strokeStyle = colors.direct;
      context.fillStyle = colors.direct;
      context.lineWidth = 2;
      context.beginPath();
      context.moveTo(from[0], from[1]);
      context.lineTo(to[0], to[1]);
      context.stroke();
      const angle = Math.atan2(to[1] - from[1], to[0] - from[0]);
      context.beginPath();
      context.moveTo(to[0], to[1]);
      context.lineTo(to[0] - ARROWHEAD_PIXELS * Math.cos(angle - 0.4), to[1] - ARROWHEAD_PIXELS * Math.sin(angle - 0.4));
      context.lineTo(to[0] - ARROWHEAD_PIXELS * Math.cos(angle + 0.4), to[1] - ARROWHEAD_PIXELS * Math.sin(angle + 0.4));
      context.closePath();
      context.fill();
      context.lineWidth = 1;
      context.font = '11px system-ui, sans-serif';
      context.fillText('Sound', from[0] + 6, from[1] - 6);
    }

    const m = project(microphone);
    microphoneOnScreen = m;
    if (m) {
      const selected = selection?.type === 'microphone';
      context.fillStyle = colors.receiver;
      context.beginPath();
      context.arc(m[0], m[1], selected ? 6 : 5, 0, 2 * Math.PI);
      context.fill();
      if (selected) {
        context.strokeStyle = colors.accent;
        context.lineWidth = 2;
        context.stroke();
        context.lineWidth = 1;
      }
      const nearest = nearestSurface(space);
      if (nearest) {
        const foot = sub(microphone, scale(surfaceFrame(space.surfaces[nearest.index]).normal, nearest.distance));
        context.setLineDash([3, 3]);
        context.strokeStyle = colors.muted;
        line(context, project, microphone, foot);
        context.setLineDash([]);
        context.fillStyle = colors.muted;
        context.font = '11px system-ui, sans-serif';
        context.fillText(formatGap(Math.abs(nearest.distance)), m[0] + 9, m[1] - 8);
      }
    }
  }

  function pick(x, y) {
    if (microphoneOnScreen && Math.hypot(x - microphoneOnScreen[0], y - microphoneOnScreen[1]) < MICROPHONE_PICK_PIXELS) {
      return { type: 'microphone' };
    }
    for (let i = projected.length - 1; i >= 0; i -= 1) {
      const { index, corners } = projected[i];
      let inside = false;
      for (let a = 0, b = corners.length - 1; a < corners.length; b = a, a += 1) {
        const [xa, ya] = corners[a];
        const [xb, yb] = corners[b];
        if ((ya > y) !== (yb > y) && x < (xb - xa) * (y - ya) / (yb - ya) + xa) inside = !inside;
      }
      if (inside) return { type: 'surface', index };
    }
    return null;
  }

  let drag = null;
  canvas.addEventListener('pointerdown', event => {
    canvas.setPointerCapture(event.pointerId);
    drag = { x: event.clientX, y: event.clientY, yaw: camera.yaw, pitch: camera.pitch, moved: false };
  });
  canvas.addEventListener('pointermove', event => {
    if (!drag) return;
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    if (Math.hypot(dx, dy) > CLICK_PIXELS) {
      drag.moved = true;
      canvas.classList.add('dragging');
    }
    if (!drag.moved) return;
    camera.yaw = drag.yaw + dx * 0.008;
    camera.pitch = Math.min(1.5, Math.max(-0.2, drag.pitch + dy * 0.008));
    draw();
  });
  canvas.addEventListener('pointerup', event => {
    canvas.classList.remove('dragging');
    if (drag && !drag.moved) {
      const rect = canvas.getBoundingClientRect();
      onSelect(pick(event.clientX - rect.left, event.clientY - rect.top));
    }
    drag = null;
  });
  canvas.addEventListener('wheel', event => {
    event.preventDefault();
    camera.distance = Math.min(60, Math.max(0.3, camera.distance * Math.exp(event.deltaY * 0.001)));
    draw();
  }, { passive: false });
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

export function formatGap(metres) {
  if (metres < 0.01) return `${+(metres * 1000).toFixed(1)} mm`;
  if (metres < 1) return `${+(metres * 100).toFixed(1)} cm`;
  return `${+metres.toFixed(2)} m`;
}
