import { createFields } from './shared/ui/fields.js';
import { attachSplitters } from './shared/ui/splitters.js';
import { encodeFloat32Wav } from './shared/domain/wav.js';
import { arrivalVector, defaultMicrophone, defaultObject, defaultRig, defaultSpace, MOST_MICROPHONES, MOST_RIGS, PATTERNS, readSpace, SpaceError } from './domain/space.js';
import { RIG_ANGLE_DEGREES, RIG_KINDS, rigMembers } from './domain/rigs.js';
import { aimAt, firstListener, FLAT_KINDS, placePoints, pointPosition, referTo, referenceObject, releaseObject, RELIEF_SHAPES } from './domain/objects.js';
import { BUILD_KINDS, headIcon, objectIcon, patternIcon, reliefIcon, situationIcon } from './ui/icons.js';
import { SITUATIONS, situationSpace } from './domain/situations.js';
import { MATERIAL_ATTRIBUTION, MATERIALS } from './domain/materials.js';
import { SMALLEST_GAP_METRES, spaceCentre } from './domain/placement.js';
import { DISPLAY, PARAMETERS } from './domain/parameters.js';
import { TEST_SIGNALS } from './domain/signals.js';
import { createNearView, formatGap } from './ui/near-view.js';
import { drawImpulse, drawResponse } from './ui/response-plots.js';
import { createPreview, stereo } from './audio/preview.js';
import { listeners } from './domain/near-filter.js';

const STORAGE_KEY = 'neartect.space';
const PREFERENCES_KEY = 'neartect.preferences';
const GAP_RANGE_METRES = [SMALLEST_GAP_METRES, 5];
const HORIZONTAL_RANGE_METRES = [-5, 5];
const HEIGHT_RANGE_METRES = [0, 5];
const SLIDER_STEPS = 1000;
const MOST_GLIDE_SECONDS = 2;
const SHEET_TITLES = Object.freeze({ emitters: 'Objects', receivers: 'Microphones and source', output: 'Air and output' });

const $ = id => document.getElementById(id);
const element = (tag, className, text) => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
};
const clone = value => structuredClone(value);
const fields = createFields(PARAMETERS, DISPLAY);

function readStored(key) {
  try { return JSON.parse(localStorage.getItem(key)); } catch { return null; }
}
function writeStored(key, value) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch {}
}
function initialSpace() {
  try { return readSpace(readStored(STORAGE_KEY)); } catch { return defaultSpace(); }
}

const state = {
  space: initialSpace(),
  selection: null,
  signal: 'pink',
  file: null,
  filter: null,
  audioRate: null,
  status: ''
};

const preview = createPreview();
const view = createNearView($('near-view'), {
  onSelect: selection => select(selection),
  onEdit: apply => { apply(state.space); changed({ save: false }); },
  onCommit: () => changed({ panel: true }),
  onAreaEnd: () => setAreaSelect(false),
  snapSurfaces: () => preferences.snapSurfaces,
  showGrid: () => preferences.showGrid
});

// Each channel is computed on its own worker, so several microphones and
// layouts take about as long as one.
const WORKERS = Math.max(1, Math.min(6, (navigator.hardwareConcurrency || 2) - 1));
const workers = Array.from({ length: WORKERS }, () => new Worker(new URL('./filter-worker.js', import.meta.url), { type: 'module' }));
let requestId = 0;
let busy = false;
let dirty = true;
const waiting = new Map();

workers.forEach(worker => worker.addEventListener('message', ({ data }) => {
  const job = waiting.get(data.id);
  waiting.delete(data.id);
  if (data.error) job?.reject(new Error(data.error)); else job?.resolve(data.samples);
}));

function channelJob(worker, space, index) {
  requestId += 1;
  const id = requestId;
  return new Promise((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    worker.postMessage({ id, space, index });
  });
}

async function computeFilter(space) {
  const started = performance.now();
  const list = listeners(space);
  const channels = new Array(list.length);
  let next = 0;
  await Promise.all(workers.map(async worker => {
    while (next < list.length) {
      const index = next;
      next += 1;
      channels[index] = await channelJob(worker, space, index);
    }
  }));
  return { sampleRateHz: space.render.sampleRateHz, channels, names: list.map(l => l.name), pans: list.map(l => l.pan), milliseconds: performance.now() - started };
}

// Relief with a speed moves while the preview plays: each new filter takes
// the relief where it is now and glides in over the time since the last one.
function moving(space) {
  return space.objects.some(o => o.relief.speedMetresPerSecond !== 0 && (o.relief.shape !== 'flat' || o.relief.roughMetres > 0));
}

function liveSpace() {
  const space = clone(state.space);
  if (state.audioRate) space.render.sampleRateHz = state.audioRate;
  if (preview.playing && moving(space)) {
    const t = preview.time;
    space.objects.forEach(o => { o.relief.offsetMetres += o.relief.speedMetresPerSecond * t; });
  }
  return space;
}

let lastGlide = 0;

async function refreshFilter() {
  if (busy) { dirty = true; return; }
  busy = true;
  dirty = false;
  const space = liveSpace();
  if (!listeners(space).length) {
    state.filter = null;
    preview.silence();
    setStatus('No microphone: add one from Layouts');
    drawPlots();
    busy = false;
    return;
  }
  try {
    const result = await computeFilter(space);
    state.filter = result;
    setStatus(`${formatMilliseconds(result.milliseconds)} · order ${space.render.order}`);
    const now = performance.now();
    const gliding = preview.playing && moving(space);
    preview.setFilter(result, gliding ? Math.min(MOST_GLIDE_SECONDS, (now - lastGlide) / 1000) : undefined);
    lastGlide = now;
    drawPlots();
    if (gliding) {
      dirty = true;
      if (!animating) animateRelief();
    }
  } catch (error) {
    setStatus(error.message, true);
  }
  busy = false;
  if (dirty) refreshFilter();
}

const formatMilliseconds = ms => `${Math.round(ms)} ms`;

function setStatus(text, error = false) {
  const status = $('status');
  status.textContent = text;
  status.classList.toggle('error', error);
}

function shownMicrophone() {
  const index = state.space.microphones.findIndex(m => state.selection?.type === 'microphone' && m.id === state.selection.id);
  return Math.max(0, index);
}

function shownChannel() {
  if (state.selection?.type !== 'rig') return shownMicrophone();
  let channel = state.space.microphones.length;
  for (const rig of state.space.rigs) {
    if (rig.id === state.selection.id) return channel;
    channel += rigMembers(state.space, rig).length;
  }
  return 0;
}

function drawPlots() {
  const filter = state.filter && { sampleRateHz: state.filter.sampleRateHz, samples: state.filter.channels[shownChannel()] ?? state.filter.channels[0] };
  drawImpulse($('impulse'), filter);
  drawResponse($('response'), filter);
}

function activeListener() {
  const chosen = state.selection?.type === 'microphone' ? state.space.microphones.find(m => m.id === state.selection.id)
    : state.selection?.type === 'rig' ? state.space.rigs.find(r => r.id === state.selection.id) : null;
  return chosen ?? firstListener(state.space) ?? { position: spaceCentre(state.space), reference: null, offset: [0, 0, 0] };
}


const UNDO_DEPTH = 50;
const history = [];
let committed = JSON.stringify(state.space);

function remember() {
  const now = JSON.stringify(state.space);
  if (now === committed) return;
  history.push(committed);
  if (history.length > UNDO_DEPTH) history.shift();
  committed = now;
  $('undo').disabled = false;
}

function undo() {
  if (!history.length) return;
  committed = history.pop();
  state.space = readSpace(JSON.parse(committed));
  if (state.selection?.type === 'object' && !state.space.objects.some(o => o.id === state.selection.id)) state.selection = null;
  if (state.selection?.type === 'microphone' && !state.space.microphones.some(m => m.id === state.selection.id)) state.selection = null;
  if (state.selection?.type === 'group') state.selection = null;
  if (state.selection?.type === 'rig' && !state.space.rigs.some(r => r.id === state.selection.id)) state.selection = null;
  $('undo').disabled = !history.length;
  writeStored(STORAGE_KEY, state.space);
  view.update(state.space, state.selection);
  refreshPanel();
  refreshFilter();
}

function changed({ save = true, panel = false } = {}) {
  placePoints(state.space);
  if (save) {
    writeStored(STORAGE_KEY, state.space);
    remember();
  }
  view.update(state.space, state.selection);
  if (panel) refreshPanel(); else syncLive();
  refreshFilter();
}

function select(selection) {
  state.selection = selection;
  view.update(state.space, state.selection);
  refreshPanel();
  drawPlots();
}

const live = [];
function syncLive() {
  live.forEach(sync => sync());
}

function paint(input) {
  const t = Number(input.value) / SLIDER_STEPS;
  input.style.setProperty('--from', '0%');
  input.style.setProperty('--to', `calc(6px + (100% - 12px) * ${t})`);
}

// `zero` keeps the leftmost few steps for an exact zero on a log slider.
function macro(container, { label, title, read, write, range: [low, high], log = false, signed = false, zero = false, format, fallback }) {
  const span = Math.log(high / low);
  const toSlider = value => {
    if (zero) return value < low ? 0 : Math.round(SLIDER_STEPS * (0.03 + 0.97 * Math.min(1, Math.log(value / low) / span)));
    if (signed) {
      if (Math.abs(value) < low) return SLIDER_STEPS / 2;
      const t = Math.min(1, Math.log(Math.abs(value) / low) / span);
      return Math.round(SLIDER_STEPS * (0.5 + Math.sign(value) * (0.02 + 0.48 * t)));
    }
    return Math.round(SLIDER_STEPS * (log ? Math.log(value / low) / span : (value - low) / (high - low)));
  };
  const fromSlider = position => {
    const t = position / SLIDER_STEPS;
    if (zero) return t < 0.03 ? 0 : low * (high / low) ** ((t - 0.03) / 0.97);
    if (signed) {
      const offset = t - 0.5;
      if (Math.abs(offset) < 0.02) return 0;
      return Math.sign(offset) * low * (high / low) ** ((Math.abs(offset) - 0.02) / 0.48);
    }
    return log ? low * (high / low) ** t : low + t * (high - low);
  };
  const name = element('label', null, label);
  name.title = title ?? label;
  const input = element('input');
  input.setAttribute('aria-label', title ?? label);
  input.type = 'range';
  input.min = '0';
  input.max = String(SLIDER_STEPS);
  const value = element('span', 'macro-value');
  const sync = () => {
    const current = read();
    if (document.activeElement !== input) input.value = String(Math.min(SLIDER_STEPS, Math.max(0, toSlider(current))));
    value.textContent = format(current);
    paint(input);
  };
  input.addEventListener('input', () => {
    write(fromSlider(Number(input.value)));
    paint(input);
    value.textContent = format(read());
    changed({ save: false });
  });
  input.addEventListener('change', () => changed());
  input.addEventListener('dblclick', () => {
    if (fallback === undefined) return;
    write(fallback);
    sync();
    changed();
  });
  container.append(name, input, value);
  live.push(sync);
  sync();
}

const degrees = value => `${Math.round(value)}°`;
const metres = formatGap;

const SIZE_LABELS = Object.freeze({
  plane: ['W', null, 'H'], disc: ['⌀', null, '⌀↕'], grating: ['W', null, 'H'], wall: ['W', null, 'H'],
  box: ['W', 'D', 'H'], corner: ['W', 'D', 'H'], recess: ['W', 'D', 'H'], pipe: ['⌀', 'L', null]
});
const SIZE_TITLES = Object.freeze({ W: 'Width', D: 'Depth', H: 'Height', L: 'Length', '⌀': 'Diameter', '⌀↕': 'Diameter up' });
const RELIEF_LABELS = Object.freeze({ flat: 'Flat', waves: 'Waves', arches: 'Arches', scallops: 'Scallops', egg: 'Egg crate' });
const PATTERN_LABELS = Object.freeze({ omni: 'Omni', wide: 'Wide cardioid', cardioid: 'Cardioid', supercardioid: 'Supercardioid', hypercardioid: 'Hypercardioid', figure8: 'Figure of eight' });
const removeIcon = '<i class="fa-solid fa-xmark" aria-hidden="true"></i>';
const RIG_COLOUR_START = 3;

// A card is a header that opens it and, when open, captioned groups of
// controls, so that place, shape, surface, and pickup read apart.
function cardHeader(card, { lead, name, kind, selected, onToggle, onRemove, removeLabel }) {
  const header = element('div', 'card-header');
  header.tabIndex = 0;
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', String(selected));
  header.append(...lead, element('span', 'name', name), element('span', 'kind', kind));
  if (onRemove) {
    const remove = element('button', 'icon-button');
    remove.type = 'button';
    remove.title = 'Remove';
    remove.setAttribute('aria-label', removeLabel);
    remove.innerHTML = removeIcon;
    remove.addEventListener('click', event => {
      event.stopPropagation();
      onRemove();
    });
    header.append(remove);
  }
  header.addEventListener('click', onToggle);
  header.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onToggle(); } });
  card.append(header);
}

function cardGroup(card, title) {
  const group = element('div', 'card-group');
  group.append(element('p', 'caption', title));
  const macros = element('div', 'macros');
  group.append(macros);
  card.append(group);
  return { group, macros };
}

function tileRow(group, macros, { label, items, chosen, onPick }) {
  const row = element('div', 'build-menu tile-row');
  row.setAttribute('role', 'radiogroup');
  row.setAttribute('aria-label', label);
  items.forEach(([value, title, html]) => {
    const button = element('button', 'build-button');
    button.type = 'button';
    button.setAttribute('role', 'radio');
    button.setAttribute('aria-checked', String(value === chosen));
    button.title = title;
    button.setAttribute('aria-label', title);
    button.innerHTML = html;
    button.addEventListener('click', () => onPick(value));
    row.append(button);
  });
  group.insertBefore(row, macros);
}

function selectRow(macros, { label, title, value, options, onChange, note = '' }) {
  const name = element('label', null);
  name.innerHTML = label;
  name.title = title;
  const select = element('select', 'row-select');
  select.setAttribute('aria-label', title);
  options.forEach(([key, text]) => {
    const option = element('option', null, text);
    option.value = key;
    select.append(option);
  });
  select.value = value;
  select.addEventListener('change', () => onChange(select.value));
  macros.append(name, select, element('span', 'macro-value', note));
}

function objectCard(container, object) {
  const selected = state.selection?.type === 'object' && state.selection.id === object.id;
  const card = element('div', 'card');
  card.setAttribute('aria-selected', String(selected || Boolean(state.selection?.objects?.includes(object.id))));
  const enabled = element('input');
  enabled.type = 'checkbox';
  enabled.checked = object.strength > 0;
  enabled.title = 'Enabled';
  enabled.addEventListener('click', event => event.stopPropagation());
  enabled.addEventListener('change', () => {
    object.strength = enabled.checked ? 1 : 0;
    changed({ panel: true });
  });
  const icon = element('span', 'object-icon');
  icon.innerHTML = objectIcon(object.kind);
  cardHeader(card, {
    lead: [enabled, icon], name: object.name, kind: MATERIALS[object.material].label, selected,
    onToggle: () => select(selected ? null : { type: 'object', id: object.id }),
    onRemove: () => removeObject(object.id), removeLabel: `Remove ${object.name}`
  });
  if (selected) {
    const current = () => state.space.objects.find(o => o.id === object.id);
    const { macros: shape } = cardGroup(card, 'Shape and place');
    SIZE_LABELS[object.kind].forEach((label, axis) => {
      if (!label) return;
      macro(shape, {
        label: SIZE_TITLES[label], range: [0.01, 20], log: true, format: formatGap, fallback: defaultObject(object.kind).size[axis],
        read: () => current().size[axis],
        write: v => { current().size[axis] = v; if (object.kind === 'pipe' && axis === 0) current().size[2] = v; }
      });
    });
    macro(shape, { label: 'Facing', title: 'Compass direction the front faces', range: [0, 360], format: degrees, fallback: 0, read: () => current().yawDegrees, write: v => { current().yawDegrees = v % 360; } });
    macro(shape, { label: 'Tilt', title: 'Tilt (90° faces up)', range: [-90, 90], format: degrees, fallback: FLAT_KINDS.includes(object.kind) ? current().tiltDegrees : 0, read: () => current().tiltDegrees, write: v => { current().tiltDegrees = v; } });
    if (object.kind === 'grating') {
      macro(shape, { label: 'Bar spacing', range: [0.002, 0.5], log: true, format: formatGap, fallback: 0.034, read: () => current().pattern.periodMetres[0], write: v => { const p = current().pattern; p.periodMetres = p.periodMetres.map(x => (x > 0 ? v : 0)); p.barMetres = p.barMetres.map(b => Math.min(b, v)); } });
      macro(shape, { label: 'Bar width', range: [0.001, 0.2], log: true, format: formatGap, fallback: 0.004, read: () => current().pattern.barMetres[0], write: v => { const p = current().pattern; p.barMetres = p.barMetres.map((b, i) => (p.periodMetres[i] > 0 ? Math.min(v, p.periodMetres[i]) : 0)); } });
    }
    if (object.kind === 'wall') {
      macro(shape, { label: 'Opening', title: 'Opening size', range: [0.02, 5], log: true, format: formatGap, fallback: 0.4, read: () => current().opening.sizeMetres[0], write: v => { const o = current().opening; o.sizeMetres = [Math.min(v, current().size[0]), Math.min(v, current().size[2])]; } });
    }
    const surface = cardGroup(card, 'Surface');
    selectRow(surface.macros, {
      label: 'Material', title: 'Material', value: object.material, note: MATERIALS[object.material].reliability,
      options: Object.entries(MATERIALS).map(([id, m]) => [id, m.label]),
      onChange: v => { current().material = v; changed({ panel: true }); }
    });
    macro(surface.macros, { label: 'Strength', title: 'Effect strength (100 % is physical)', range: [0, 2], fallback: 1, format: v => `${Math.round(v * 100)} %`, read: () => current().strength, write: v => { current().strength = v; } });
    reliefControls(card, current);
    const actions = element('div', 'card-actions');
    const button = (html, title, action) => {
      const b = element('button');
      b.type = 'button';
      b.innerHTML = html;
      b.title = title;
      b.setAttribute('aria-label', title);
      b.addEventListener('click', action);
      actions.append(b);
    };
    button('<i class="fa-solid fa-sliders" aria-hidden="true"></i>', 'Details', () => openDetails(object.id));
    button('<i class="fa-solid fa-clone" aria-hidden="true"></i>', 'Duplicate', () => {
      const copy = clone(object);
      copy.id = defaultObject(object.kind).id;
      copy.name = `${object.name} copy`;
      copy.position = copy.position.map((v, i) => v + (i === 0 ? 0.2 : 0));
      state.space.objects.push(copy);
      state.selection = { type: 'object', id: copy.id };
      changed({ panel: true });
    });
    card.append(actions);
  }
  container.append(card);
}

function reliefControls(card, current) {
  const relief = () => current().relief;
  const { group, macros } = cardGroup(card, 'Relief');
  tileRow(group, macros, {
    label: 'Relief', chosen: relief().shape,
    items: RELIEF_SHAPES.map(shape => [shape, RELIEF_LABELS[shape], reliefIcon(shape)]),
    onPick: shape => { relief().shape = shape; changed({ panel: true }); }
  });
  const shaped = relief().shape !== 'flat';
  if (shaped) {
    macro(macros, { label: 'Height', title: 'Relief height', range: [0.001, 0.5], log: true, format: formatGap, fallback: 0.02, read: () => relief().heightMetres, write: v => { relief().heightMetres = v; } });
    macro(macros, { label: 'Period', title: 'Relief period', range: [0.01, 5], log: true, format: formatGap, fallback: 0.2, read: () => relief().periodMetres, write: v => { relief().periodMetres = v; } });
    macro(macros, { label: 'Direction', title: 'Relief direction (0° along the width)', range: [0, 180], format: degrees, fallback: 0, read: () => relief().directionDegrees % 180, write: v => { relief().directionDegrees = v; } });
  }
  macro(macros, { label: 'Rough', title: 'Roughness height (rms)', range: [0.0005, 0.05], zero: true, format: v => (v ? formatGap(v) : 'smooth'), fallback: 0, read: () => relief().roughMetres, write: v => { relief().roughMetres = v; } });
  if (relief().roughMetres > 0) {
    macro(macros, { label: 'Grain', title: 'Roughness grain', range: [0.01, 1], log: true, format: formatGap, fallback: 0.05, read: () => relief().grainMetres, write: v => { relief().grainMetres = v; } });
  }
  if (shaped || relief().roughMetres > 0) {
    macro(macros, { label: 'Motion', title: 'Moves while the preview plays', range: [0.001, 5], signed: true, format: v => (v ? `${formatGap(Math.abs(v))}/s` : 'still'), fallback: 0, read: () => relief().speedMetresPerSecond, write: v => { relief().speedMetresPerSecond = v; } });
    macro(macros, { label: 'Shift', title: 'Relief position along its direction', range: [0.001, 5], signed: true, format: formatGap, fallback: 0, read: () => relief().offsetMetres, write: v => { relief().offsetMetres = v; } });
  }
}

function removeObject(id) {
  releaseObject(state.space, id);
  state.space.objects = state.space.objects.filter(o => o.id !== id);
  if (state.selection?.id === id) state.selection = null;
  changed({ panel: true });
}


function refreshPanel() {
  live.length = 0;
  const list = $('object-list');
  list.replaceChildren();
  state.space.objects.forEach(object => objectCard(list, object));

  const listeners = $('listener-list');
  listeners.replaceChildren();
  state.space.microphones.forEach((microphone, index) => microphoneCard(listeners, microphone, index));
  state.space.rigs.forEach((rig, index) => rigCard(listeners, rig, index));
  document.querySelectorAll('#listener-menu button').forEach(b => {
    b.disabled = b.dataset.kind === 'mono' ? state.space.microphones.length >= MOST_MICROPHONES : state.space.rigs.length >= MOST_RIGS;
  });

  const source = state.space.source;
  document.querySelectorAll('#source-kind [data-kind]').forEach(button => button.setAttribute('aria-checked', String(button.dataset.kind === source.kind)));
  $('source-distance').hidden = source.kind !== 'point';
  $('source-distance').setAttribute('aria-pressed', String(source.distance));
  const sourceMacros = $('source-macros');
  sourceMacros.replaceChildren();
  if (source.kind === 'point') {
    pointControls(sourceMacros, () => state.space.source, 1);
  } else {
    macro(sourceMacros, { label: 'From', title: 'Compass direction the sound comes from', range: [0, 360], fallback: 0, format: degrees, read: () => state.space.arrival.azimuthDegrees, write: v => { state.space.arrival.azimuthDegrees = v % 360; } });
    macro(sourceMacros, { label: 'Elevation', range: [-90, 90], fallback: 10, format: degrees, read: () => state.space.arrival.elevationDegrees, write: v => { state.space.arrival.elevationDegrees = v; } });
  }

  const output = $('output-fields');
  output.replaceChildren();
  fields.numberField(output, { key: 'temperatureCelsius', value: state.space.air.temperatureCelsius, onCommit: v => { state.space.air.temperatureCelsius = v; changed(); } });
  fields.selectField(output, {
    label: 'Sample rate', value: String(state.space.render.sampleRateHz),
    options: [['44100', '44.1 kHz'], ['48000', '48 kHz'], ['96000', '96 kHz']],
    onCommit: v => { state.space.render.sampleRateHz = Number(v); changed(); }
  });
  fields.numberField(output, { key: 'lengthMilliseconds', value: state.space.render.lengthMilliseconds, onCommit: v => { state.space.render.lengthMilliseconds = v; changed(); } });
  fields.numberField(output, { key: 'order', value: state.space.render.order, reliability: 'A', onCommit: v => { state.space.render.order = v; changed(); } });
}

function pointControls(macros, current, gapFallback) {
  const reference = referenceObject(state.space, current());
  selectRow(macros, {
    label: '<i class="fa-solid fa-anchor" aria-hidden="true"></i> Bound to', title: 'Bound to an object, or free in the world',
    value: reference?.id ?? '',
    options: [['', 'World'], ...state.space.objects.map(o => [o.id, o.name])],
    onChange: id => { referTo(state.space, current(), id || null); changed({ panel: true }); }
  });
  if (reference) {
    [['↔ Across', 'Across the object'], ['⟂ Gap', 'Out from the object (negative is behind or inside)'], ['↕ Up', 'Up along the object']].forEach(([label, title], axis) => macro(macros, {
      label, title, signed: true, range: GAP_RANGE_METRES, format: formatGap, fallback: axis === 1 ? gapFallback : 0,
      read: () => current().offset[axis], write: v => { current().offset[axis] = v; }
    }));
  } else {
    const position = () => current().position;
    macro(macros, { label: 'East', range: HORIZONTAL_RANGE_METRES, format: formatGap, fallback: 0, read: () => position()[0], write: v => { position()[0] = v; } });
    macro(macros, { label: 'North', range: HORIZONTAL_RANGE_METRES, format: formatGap, fallback: 0, read: () => position()[1], write: v => { position()[1] = v; } });
    macro(macros, { label: 'Up', title: 'Height', range: HEIGHT_RANGE_METRES, format: formatGap, fallback: 1.2, read: () => position()[2], write: v => { position()[2] = v; } });
  }
}

function microphoneCard(container, microphone, index) {
  const selected = state.selection?.type === 'microphone' && state.selection.id === microphone.id;
  const card = element('div', 'card');
  card.setAttribute('aria-selected', String(selected || Boolean(state.selection?.points?.includes(`microphone:${microphone.id}`))));
  const dot = element('span', 'mic-dot');
  dot.style.background = `var(--mic-${index % MOST_MICROPHONES + 1})`;
  cardHeader(card, {
    lead: [dot], name: microphone.name, kind: PATTERN_LABELS[microphone.pattern], selected,
    onToggle: () => select(selected ? null : { type: 'microphone', id: microphone.id }),
    onRemove: () => removeMicrophone(microphone.id), removeLabel: `Remove ${microphone.name}`
  });
  if (selected) {
    const current = () => state.space.microphones.find(m => m.id === microphone.id);
    pointControls(cardGroup(card, 'Place').macros, current, 0.3);
    const { group, macros } = cardGroup(card, 'Pickup');
    tileRow(group, macros, {
      label: 'Pickup pattern', chosen: microphone.pattern,
      items: Object.entries(PATTERNS).map(([name, a]) => [name, PATTERN_LABELS[name], patternIcon(a)]),
      onPick: name => {
        const mic = current();
        if (mic.pattern === 'omni' && name !== 'omni') aimAt(state.space, mic, towardsSource(mic));
        mic.pattern = name;
        changed({ panel: true });
      }
    });
    if (microphone.pattern !== 'omni') {
      macro(macros, { label: 'Aim', title: 'Aim around', range: [0, 360], format: degrees, fallback: 0, read: () => current().aim.azimuthDegrees, write: v => { current().aim.azimuthDegrees = v % 360; } });
      macro(macros, { label: 'Tilt', title: 'Aim tilted up or down', range: [-90, 90], format: degrees, fallback: 0, read: () => current().aim.elevationDegrees, write: v => { current().aim.elevationDegrees = v; } });
    }
  }
  container.append(card);
}

function removeGroup(group) {
  group.objects.forEach(id => releaseObject(state.space, id));
  state.space.objects = state.space.objects.filter(o => !group.objects.includes(o.id));
  const microphones = state.space.microphones.filter(m => !group.points.includes(`microphone:${m.id}`));
  state.space.microphones = microphones;
  state.space.rigs = state.space.rigs.filter(r => !group.points.includes(`rig:${r.id}`));
  state.selection = null;
  changed({ panel: true });
}

function setAreaSelect(on) {
  $('select-area').setAttribute('aria-pressed', String(on));
  view.setAreaSelect(on);
}
$('select-area').addEventListener('click', () => setAreaSelect($('select-area').getAttribute('aria-pressed') !== 'true'));

function towardsSource(microphone) {
  if (state.space.source.kind === 'far') return arrivalVector(state.space.arrival);
  const from = pointPosition(state.space, microphone);
  const to = pointPosition(state.space, state.space.source);
  const d = to.map((v, i) => v - from[i]);
  const n = Math.hypot(...d) || 1;
  return d.map(v => v / n);
}

function rigBadge(kind, index) {
  const badge = element('span', 'rig-badge');
  badge.style.color = `var(--mic-${(RIG_COLOUR_START + index) % MOST_MICROPHONES + 1})`;
  if (kind === 'binaural') badge.innerHTML = headIcon(); else badge.textContent = RIG_KINDS[kind].short;
  return badge;
}

function rigCard(container, rig, index) {
  const selected = state.selection?.type === 'rig' && state.selection.id === rig.id;
  const card = element('div', 'card');
  card.setAttribute('aria-selected', String(selected || Boolean(state.selection?.points?.includes(`rig:${rig.id}`))));
  cardHeader(card, {
    lead: [rigBadge(rig.kind, index)], name: rig.name, kind: rigMembers(state.space, rig).map(m => m.name).join(' '), selected,
    onToggle: () => select(selected ? null : { type: 'rig', id: rig.id }),
    onRemove: () => removeRig(rig.id), removeLabel: `Remove ${rig.name}`
  });
  if (selected) {
    const current = () => state.space.rigs.find(r => r.id === rig.id);
    pointControls(cardGroup(card, 'Place').macros, current, 0.5);
    const { macros } = cardGroup(card, rig.kind === 'binaural' ? 'Head' : 'Layout');
    macro(macros, { label: 'Facing', title: 'Facing around', range: [0, 360], format: degrees, fallback: 0, read: () => current().aim.azimuthDegrees, write: v => { current().aim.azimuthDegrees = v % 360; } });
    macro(macros, { label: 'Tilt', title: 'Tilt up or down', range: [-90, 90], format: degrees, fallback: 0, read: () => current().aim.elevationDegrees, write: v => { current().aim.elevationDegrees = v; } });
    const defaults = RIG_KINDS[rig.kind];
    if (defaults.spacingMetres > 0) {
      const binaural = rig.kind === 'binaural';
      macro(macros, { label: binaural ? 'Head width' : 'Spacing', range: binaural ? [0.1, 0.3] : [0.01, 5], log: true, format: formatGap, fallback: defaults.spacingMetres, read: () => current().spacingMetres, write: v => { current().spacingMetres = v; } });
    }
    if (defaults.angleDegrees !== undefined && rig.kind !== 'ab' && rig.kind !== 'decca') {
      macro(macros, { label: 'Angle', title: 'Angle between the capsules', range: RIG_ANGLE_DEGREES, format: degrees, fallback: defaults.angleDegrees, read: () => current().angleDegrees, write: v => { current().angleDegrees = v; } });
    }
  }
  container.append(card);
}

function removeRig(id) {
  state.space.rigs = state.space.rigs.filter(r => r.id !== id);
  if (state.selection?.id === id) state.selection = null;
  changed({ panel: true });
}

function removeMicrophone(id) {
  state.space.microphones = state.space.microphones.filter(m => m.id !== id);
  if (state.selection?.id === id) state.selection = null;
  changed({ panel: true });
}

// Adding a layout while a lone microphone is all there is puts the layout
// in its place: same binding, same spot, and its aim if it had one.
function addListener(kind) {
  const space = state.space;
  const model = activeListener();
  if (kind === 'mono') {
    if (space.microphones.length >= MOST_MICROPHONES) return;
    const copy = defaultMicrophone(space.microphones.length, { position: model.position.map((v, i) => v + (i === 0 ? 0.2 : 0)), reference: model.reference, offset: model.offset.map((v, i) => v + (i === 0 ? 0.2 : 0)) });
    space.microphones.push(copy);
    state.selection = { type: 'microphone', id: copy.id };
    changed({ panel: true });
    return;
  }
  if (space.rigs.length >= MOST_RIGS) return;
  const lone = space.microphones.length === 1 && space.rigs.length === 0 ? space.microphones[0] : null;
  const rig = lone
    ? defaultRig(kind, { position: [...lone.position], reference: lone.reference, offset: [...lone.offset] })
    : defaultRig(kind, { position: model.position.map((v, i) => v + (i === 0 ? 0.4 * (space.rigs.length + 1) : 0)) });
  if (lone && lone.pattern !== 'omni') rig.aim = { ...lone.aim };
  else aimAt(space, rig, towardsSource({ ...rig }));
  if (lone) space.microphones = [];
  space.rigs.push(rig);
  state.selection = { type: 'rig', id: rig.id };
  changed({ panel: true });
}

{
  const menu = $('listener-menu');
  const tile = (kind, title, html) => {
    const button = element('button', 'build-button rig-button');
    button.type = 'button';
    button.dataset.kind = kind;
    button.title = title;
    button.setAttribute('aria-label', `Add ${title.toLowerCase()}`);
    button.innerHTML = html;
    button.addEventListener('click', () => addListener(kind));
    menu.append(button);
  };
  tile('mono', 'Microphone', '<i class="fa-solid fa-microphone" aria-hidden="true"></i>');
  for (const [kind, { label, short }] of Object.entries(RIG_KINDS)) tile(kind, label, kind === 'binaural' ? headIcon() : short);
}

document.querySelectorAll('#source-kind [data-kind]').forEach(button => button.addEventListener('click', () => {
  const source = state.space.source;
  if (source.kind === button.dataset.kind) return;
  source.kind = button.dataset.kind;
  if (source.kind === 'point' && !source.reference) {
    const at = activeListener().position;
    const toward = [Math.sin(state.space.arrival.azimuthDegrees * Math.PI / 180), Math.cos(state.space.arrival.azimuthDegrees * Math.PI / 180)];
    source.position = [at[0] + toward[0], at[1] + toward[1], at[2]];
  }
  state.selection = source.kind === 'point' ? { type: 'source' } : null;
  changed({ panel: true });
}));
$('source-distance').addEventListener('click', () => {
  state.space.source.distance = !state.space.source.distance;
  changed({ panel: true });
});

function openDetails(id) {
  const object = state.space.objects.find(o => o.id === id);
  if (!object) return;
  const dialog = $('details');
  $('details-title').textContent = object.name;
  const body = $('details-body');
  body.replaceChildren();
  const group = title => {
    const section = element('section');
    section.append(element('h3', null, title));
    const grid = element('div', 'fields');
    section.append(grid);
    body.append(section);
    return grid;
  };
  const update = (apply, rebuild = false) => {
    apply(state.space.objects.find(o => o.id === id));
    changed({ panel: true });
    if (rebuild) openDetails(id);
  };
  const place = group('Place');
  fields.textField(place, { label: 'Name', value: object.name, onCommit: v => update(o => { o.name = v; $('details-title').textContent = v; }) });
  ['East', 'North', 'Up'].forEach((label, axis) => fields.numberField(place, {
    key: 'positionMetres', label, value: object.position[axis], onCommit: v => update(o => { o.position[axis] = v; })
  }));
  fields.numberField(place, { key: 'yawDegrees', value: object.yawDegrees, onCommit: v => update(o => { o.yawDegrees = v % 360; }) });
  fields.numberField(place, { key: 'tiltDegrees', value: object.tiltDegrees, onCommit: v => update(o => { o.tiltDegrees = v; }) });
  const shape = group('Size');
  const sizeNames = { W: 'Width', D: 'Depth', H: 'Height', L: 'Length', '⌀': 'Diameter', '⌀↕': 'Diameter up' };
  SIZE_LABELS[object.kind].forEach((label, axis) => {
    if (!label) return;
    fields.numberField(shape, { key: 'sizeMetres', label: sizeNames[label], value: object.size[axis], onCommit: v => update(o => { o.size[axis] = v; if (o.kind === 'pipe' && axis === 0) o.size[2] = v; }) });
  });
  const acoustics = group('Acoustics');
  fields.selectField(acoustics, {
    label: 'Material', value: object.material, reliability: MATERIALS[object.material].reliability,
    options: Object.entries(MATERIALS).map(([key, m]) => [key, m.label]),
    onCommit: v => update(o => { o.material = v; }, true)
  });
  fields.numberField(acoustics, { key: 'strength', value: object.strength, onCommit: v => update(o => { o.strength = v; }) });
  if (object.kind === 'grating') {
    const pattern = group('Grating');
    ['across', 'up'].forEach((axis, i) => {
      fields.numberField(pattern, { key: 'periodMetres', label: `Period ${axis}`, reliability: 'A', value: object.pattern.periodMetres[i], onCommit: v => update(o => { o.pattern.periodMetres[i] = v; o.pattern.barMetres[i] = Math.min(o.pattern.barMetres[i], v); }) });
      fields.numberField(pattern, { key: 'barMetres', label: `Bar ${axis}`, reliability: 'A', value: object.pattern.barMetres[i], onCommit: v => update(o => { o.pattern.barMetres[i] = Math.min(v, o.pattern.periodMetres[i]); }) });
    });
    fields.numberField(pattern, { key: 'thicknessMetres', value: object.pattern.thicknessMetres, onCommit: v => update(o => { o.pattern.thicknessMetres = v; }) });
  }
  if (object.kind === 'wall') {
    const opening = group('Opening');
    fields.selectField(opening, { label: 'Shape', value: object.opening.shape, options: [['round', 'Round'], ['rectangle', 'Rectangle']], onCommit: v => update(o => { o.opening.shape = v; }) });
    ['Width', 'Height'].forEach((label, i) => fields.numberField(opening, { key: 'sizeMetres', label, value: object.opening.sizeMetres[i], onCommit: v => update(o => { o.opening.sizeMetres[i] = v; }) }));
    ['Across', 'Up'].forEach((label, i) => fields.numberField(opening, { key: 'positionMetres', label: `Offset ${label.toLowerCase()}`, value: object.opening.offsetMetres[i], onCommit: v => update(o => { o.opening.offsetMetres[i] = v; }) }));
  }
  if (!dialog.open) {
    dialog.showModal();
    dialog.focus();
  }
}
$('details-close').addEventListener('click', () => $('details').close());

function placeNew(kind) {
  const at = activeListener().position;
  const placements = {
    plane: { position: [at[0], at[1] + 0.5, at[2]], yawDegrees: 180 },
    disc: { position: [at[0], at[1] + 0.5, at[2]], yawDegrees: 180 },
    grating: { position: [at[0], at[1], Math.max(0.01, at[2] - 0.3)], tiltDegrees: 90, material: 'rigid' },
    wall: { position: [at[0], at[1] + 0.5, 1.25], yawDegrees: 180, material: 'brick' },
    box: { position: [at[0] + 0.6, at[1] + 0.3, 0.3], material: 'woodPanel' },
    corner: { position: [at[0] - 0.4, at[1] - 0.4, 0], material: 'plaster' },
    recess: { position: [at[0], at[1] + 0.3, at[2]], yawDegrees: 180, material: 'plaster' },
    pipe: { position: [at[0] + 1.1, at[1], at[2]], yawDegrees: 90, material: 'concrete' }
  };
  const object = defaultObject(kind, placements[kind]);
  state.space.objects.push(object);
  state.selection = { type: 'object', id: object.id };
  changed({ panel: true });
}

for (const [kind, label] of BUILD_KINDS) {
  const button = element('button', 'build-button');
  button.type = 'button';
  button.title = label;
  button.setAttribute('aria-label', `Add ${label.toLowerCase()}`);
  button.innerHTML = objectIcon(kind);
  button.addEventListener('click', () => placeNew(kind));
  $('build-menu').append(button);
}

for (const [id, { label }] of Object.entries(SITUATIONS)) {
  const button = element('button', 'build-button');
  button.type = 'button';
  button.title = label;
  button.setAttribute('aria-label', label);
  button.innerHTML = situationIcon(id);
  button.addEventListener('click', () => {
    state.space = situationSpace(id, state.space);
    state.selection = null;
    view.centre(activeListener().position);
    changed({ panel: true });
  });
  $('situations').append(button);
}
$('undo').addEventListener('click', undo);

document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z' && !event.target.closest('input, textarea')) {
    event.preventDefault();
    undo();
    return;
  }
  if (event.target.closest('input, select, textarea, dialog')) return;
  if ((event.key === 'Delete' || event.key === 'Backspace') && state.selection?.type === 'object') {
    event.preventDefault();
    removeObject(state.selection.id);
  }
  if ((event.key === 'Delete' || event.key === 'Backspace') && state.selection?.type === 'group') {
    event.preventDefault();
    removeGroup(state.selection);
  }
  if ((event.key === 'Delete' || event.key === 'Backspace') && state.selection?.type === 'rig') {
    event.preventDefault();
    removeRig(state.selection.id);
  }
  if ((event.key === 'Delete' || event.key === 'Backspace') && state.selection?.type === 'microphone') {
    event.preventDefault();
    removeMicrophone(state.selection.id);
  }
  if (event.key === 'Escape' && $('select-area').getAttribute('aria-pressed') === 'true') setAreaSelect(false);
  else if (event.key === 'Escape' && state.selection) select(null);
});

async function signalSamples(sampleRate) {
  if (state.signal === 'file' && state.file) return state.file;
  return { samples: TEST_SIGNALS[state.signal === 'file' ? 'pink' : state.signal](sampleRate), sampleRate };
}

async function startPlayback() {
  const rate = preview.sampleRate;
  if (state.audioRate !== rate) {
    state.audioRate = rate;
    await refreshFilter();
  }
  const { samples, sampleRate } = await signalSamples(rate);
  await preview.play(samples, sampleRate);
  $('play').setAttribute('aria-pressed', 'true');
  $('play').textContent = 'Stop';
  if (moving(state.space)) refreshFilter();
}

let animating = false;
function animateRelief() {
  if (preview.playing && moving(state.space)) {
    animating = true;
    view.setReliefTime(preview.time);
    requestAnimationFrame(animateRelief);
  } else {
    animating = false;
    view.setReliefTime(0);
  }
}

function stopPlayback() {
  preview.stop();
  $('play').setAttribute('aria-pressed', 'false');
  $('play').textContent = 'Play';
}

$('play').addEventListener('click', () => (preview.playing ? stopPlayback() : startPlayback()));
document.addEventListener('keydown', event => {
  if (event.code !== 'Space' || event.target.closest('input, select, textarea, button, dialog')) return;
  event.preventDefault();
  $('play').click();
});
$('signal').addEventListener('change', () => {
  state.signal = $('signal').value;
  if (preview.playing) startPlayback();
});
$('load-signal').addEventListener('click', () => $('signal-file').click());
$('signal-file').addEventListener('change', async () => {
  const file = $('signal-file').files[0];
  $('signal-file').value = '';
  if (!file) { $('signal').value = state.signal; return; }
  try {
    const audio = await preview.decode(await file.arrayBuffer());
    const samples = new Float32Array(audio.length);
    for (let c = 0; c < audio.numberOfChannels; c += 1) {
      const channel = audio.getChannelData(c);
      for (let i = 0; i < samples.length; i += 1) samples[i] += channel[i] / audio.numberOfChannels;
    }
    state.file = { samples, sampleRate: audio.sampleRate, name: file.name };
    state.signal = 'file';
    const option = $('signal').options[3];
    option.textContent = file.name;
    option.hidden = false;
    $('signal').value = 'file';
    if (preview.playing) startPlayback();
  } catch {
    setStatus('The file could not be read as audio.', true);
    $('signal').value = state.signal;
  }
});
$('bypass').addEventListener('click', () => {
  const on = $('bypass').getAttribute('aria-pressed') !== 'true';
  $('bypass').setAttribute('aria-pressed', String(on));
  preview.setBypass(on);
});

function download(name, blob) {
  const url = URL.createObjectURL(blob);
  const link = Object.assign(document.createElement('a'), { href: url, download: name });
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

$('save').addEventListener('click', () => download('neartect-space.json', new Blob([`${JSON.stringify(state.space, null, 2)}\n`], { type: 'application/json' })));
$('open').addEventListener('click', () => $('open-file').click());
$('open-file').addEventListener('change', async () => {
  const file = $('open-file').files[0];
  $('open-file').value = '';
  if (!file) return;
  try {
    state.space = readSpace(JSON.parse(await file.text()));
    state.selection = null;
    view.centre(spaceCentre(state.space));
    changed({ panel: true });
  } catch (error) {
    setStatus(error instanceof SpaceError ? error.message : 'This file is not a Neartect space.', true);
  }
});

const exportDialog = $('export-dialog');
$('export').addEventListener('click', () => {
  $('export-summary').textContent = `${state.space.render.sampleRateHz / 1000} kHz · 32-bit float · ${state.space.render.lengthMilliseconds} ms filter · ${state.filter?.channels.length ?? 1} ${state.filter?.channels.length === 1 ? 'channel' : 'channels'}`;
  $('export-error').hidden = true;
  exportDialog.showModal();
  exportDialog.focus();
});
$('export-filter').addEventListener('click', async () => {
  if (!listeners(state.space).length) return;
  const result = await computeFilter(clone(state.space));
  const channels = $('export-mix').checked ? stereo(result.channels, result.pans) : result.channels;
  if ($('export-normalise').checked) {
    const peak = Math.max(1e-9, ...channels.map(c => c.reduce((m, x) => Math.max(m, Math.abs(x)), 0)));
    channels.forEach(c => c.forEach((x, i) => { c[i] = x / peak; }));
  }
  download('neartect-ir.wav', new Blob([encodeFloat32Wav(channels, result.sampleRateHz)], { type: 'audio/wav' }));
});
$('export-processed').addEventListener('click', async () => {
  if (!listeners(state.space).length) return;
  try {
    const { samples, sampleRate } = await signalSamples(state.space.render.sampleRateHz);
    const space = clone(state.space);
    space.render.sampleRateHz = sampleRate;
    const filter = await computeFilter(space);
    const frames = samples.length + filter.channels[0].length - 1;
    const offline = new OfflineAudioContext(filter.channels.length, frames, sampleRate);
    const input = offline.createBuffer(1, samples.length, sampleRate);
    input.copyToChannel(samples, 0);
    const source = offline.createBufferSource();
    source.buffer = input;
    const merger = offline.createChannelMerger(filter.channels.length);
    filter.channels.forEach((channel, index) => {
      const response = offline.createBuffer(1, channel.length, sampleRate);
      response.copyToChannel(channel, 0);
      const convolver = offline.createConvolver();
      convolver.normalize = false;
      convolver.buffer = response;
      source.connect(convolver).connect(merger, 0, index);
    });
    offline.destination.channelInterpretation = 'discrete';
    merger.connect(offline.destination);
    source.start();
    const rendered = await offline.startRendering();
    const raw = filter.channels.map((_, index) => rendered.getChannelData(index).slice());
    const out = $('export-mix').checked ? stereo(raw, filter.pans) : raw;
    download('neartect-filtered.wav', new Blob([encodeFloat32Wav(out, sampleRate)], { type: 'audio/wav' }));
  } catch (error) {
    $('export-error').textContent = error.message;
    $('export-error').hidden = false;
  }
});

$('attribution-text').textContent = MATERIAL_ATTRIBUTION;
$('attribution-button').addEventListener('click', () => {
  const text = $('attribution-text');
  text.hidden = !text.hidden;
  $('attribution-button').setAttribute('aria-expanded', String(!text.hidden));
});
$('view-reset').addEventListener('click', () => view.reset());

const preferences = { panelWidth: 340, resultsHeight: 220, snapSurfaces: false, showGrid: true, ...(readStored(PREFERENCES_KEY) ?? {}) };
$('show-grid').setAttribute('aria-pressed', String(preferences.showGrid));
$('show-grid').addEventListener('click', () => {
  preferences.showGrid = !preferences.showGrid;
  $('show-grid').setAttribute('aria-pressed', String(preferences.showGrid));
  writeStored(PREFERENCES_KEY, preferences);
  view.update(state.space, state.selection);
});
$('snap-surfaces').setAttribute('aria-pressed', String(preferences.snapSurfaces));
$('snap-surfaces').addEventListener('click', () => {
  preferences.snapSurfaces = !preferences.snapSurfaces;
  $('snap-surfaces').setAttribute('aria-pressed', String(preferences.snapSurfaces));
  writeStored(PREFERENCES_KEY, preferences);
});
attachSplitters(document.querySelector('.layout'), { column: $('split-column'), row: $('split-row') }, preferences, sizes => {
  Object.assign(preferences, sizes);
  writeStored(PREFERENCES_KEY, preferences);
  drawPlots();
});
new ResizeObserver(() => drawPlots()).observe($('results'));

function openSheet(name) {
  const body = document.body;
  if (body.dataset.sheet === name) delete body.dataset.sheet;
  else {
    body.dataset.sheet = name;
    $('sheet-title').textContent = SHEET_TITLES[name];
    const target = document.querySelector(`.panel [data-sheet="${name}"]`);
    if (target instanceof HTMLDetailsElement) target.open = true;
  }
  document.querySelectorAll('.sheet-tabs [data-sheet]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.sheet === body.dataset.sheet)));
}
document.querySelectorAll('.sheet-tabs [data-sheet]').forEach(button => button.addEventListener('click', () => openSheet(button.dataset.sheet)));
$('sheet-close').addEventListener('click', () => openSheet(document.body.dataset.sheet));
$('results-toggle').addEventListener('click', () => {
  const open = !document.body.classList.contains('results-open');
  document.body.classList.toggle('results-open', open);
  $('results-toggle').setAttribute('aria-expanded', String(open));
  requestAnimationFrame(drawPlots);
});

view.centre(spaceCentre(state.space));
changed({ save: false, panel: true });
