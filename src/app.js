import { createFields } from './shared/ui/fields.js';
import { attachSplitters } from './shared/ui/splitters.js';
import { encodeFloat32Wav } from './shared/domain/wav.js';
import { defaultObject, defaultSpace, readSpace, SpaceError } from './domain/space.js';
import { FLAT_KINDS } from './domain/objects.js';
import { BUILD_KINDS, objectIcon, situationIcon } from './ui/icons.js';
import { SITUATIONS, situationSpace } from './domain/situations.js';
import { MATERIAL_ATTRIBUTION, MATERIALS } from './domain/materials.js';
import { nearestSurface, SMALLEST_GAP_METRES, spaceCentre, withGap } from './domain/placement.js';
import { DISPLAY, PARAMETERS } from './domain/parameters.js';
import { TEST_SIGNALS } from './domain/signals.js';
import { createNearView, formatGap } from './ui/near-view.js';
import { drawImpulse, drawResponse } from './ui/response-plots.js';
import { createPreview } from './audio/preview.js';

const STORAGE_KEY = 'neartect.space';
const PREFERENCES_KEY = 'neartect.preferences';
const GAP_RANGE_METRES = [SMALLEST_GAP_METRES, 3];
const HORIZONTAL_RANGE_METRES = [-5, 5];
const HEIGHT_RANGE_METRES = [0, 5];
const SLIDER_STEPS = 1000;
const SHEET_TITLES = Object.freeze({ emitters: 'Objects', receivers: 'Microphone', output: 'Air and output' });

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
  onCommit: () => changed({ panel: true })
});

const worker = new Worker(new URL('./filter-worker.js', import.meta.url), { type: 'module' });
let requestId = 0;
let busy = false;
let dirty = true;
const waiting = new Map();

function computeFilter(space) {
  requestId += 1;
  const id = requestId;
  return new Promise((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    worker.postMessage({ id, space });
  });
}
worker.addEventListener('message', ({ data }) => {
  const job = waiting.get(data.id);
  waiting.delete(data.id);
  if (data.error) job?.reject(new Error(data.error)); else job?.resolve(data);
});

async function refreshFilter() {
  if (busy) { dirty = true; return; }
  busy = true;
  dirty = false;
  const space = clone(state.space);
  try {
    const result = await computeFilter(space);
    state.filter = result;
    setStatus(`${formatMilliseconds(result.milliseconds)} · order ${space.render.order}`);
    if (state.audioRate && state.audioRate !== result.sampleRateHz) {
      const live = await computeFilter({ ...space, render: { ...space.render, sampleRateHz: state.audioRate } });
      preview.setFilter(live);
    } else {
      preview.setFilter(result);
    }
    drawPlots();
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

function drawPlots() {
  drawImpulse($('impulse'), state.filter);
  drawResponse($('response'), state.filter);
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
  $('undo').disabled = !history.length;
  writeStored(STORAGE_KEY, state.space);
  view.update(state.space, state.selection);
  refreshPanel();
  refreshFilter();
}

function changed({ save = true, panel = false } = {}) {
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

function macro(container, { label, title, read, write, range: [low, high], log = false, format, fallback }) {
  const toSlider = value => Math.round(SLIDER_STEPS * (log ? Math.log(value / low) / Math.log(high / low) : (value - low) / (high - low)));
  const fromSlider = position => {
    const t = position / SLIDER_STEPS;
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
  box: ['W', 'D', 'H'], corner: ['W', 'D', 'H'], recess: ['W', 'D', 'H'], pipe: ['L', '⌀', null]
});

function objectCard(container, object) {
  const selected = state.selection?.type === 'object' && state.selection.id === object.id;
  const card = element('div', 'card');
  card.setAttribute('aria-selected', String(selected));
  const header = element('div', 'card-header');
  header.tabIndex = 0;
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', String(selected));
  const enabled = element('input');
  enabled.type = 'checkbox';
  enabled.checked = object.strength > 0;
  enabled.title = 'Enabled';
  enabled.addEventListener('click', event => event.stopPropagation());
  enabled.addEventListener('change', () => {
    object.strength = enabled.checked ? 1 : 0;
    changed({ panel: true });
  });
  const remove = element('button', 'icon-button');
  remove.type = 'button';
  remove.title = 'Remove';
  remove.setAttribute('aria-label', `Remove ${object.name}`);
  remove.innerHTML = '<i class="fa-solid fa-xmark" aria-hidden="true"></i>';
  remove.addEventListener('click', event => {
    event.stopPropagation();
    removeObject(object.id);
  });
  const toggle = () => select(selected ? null : { type: 'object', id: object.id });
  header.addEventListener('click', toggle);
  header.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); toggle(); } });
  const icon = element('span', 'object-icon');
  icon.innerHTML = objectIcon(object.kind);
  header.append(enabled, icon, element('span', 'name', object.name), element('span', 'kind', MATERIALS[object.material].label), remove);
  card.append(header);
  if (selected) {
    const current = () => state.space.objects.find(o => o.id === object.id);
    const macros = element('div', 'macros');
    SIZE_LABELS[object.kind].forEach((label, axis) => {
      if (!label) return;
      macro(macros, {
        label, title: { W: 'Width', D: 'Depth', H: 'Height', L: 'Length', '⌀': 'Diameter', '⌀↕': 'Diameter up' }[label], range: [0.01, 20], log: true, format: formatGap, fallback: defaultObject(object.kind).size[axis],
        read: () => current().size[axis],
        write: v => { current().size[axis] = v; if (object.kind === 'pipe' && axis === 1) current().size[2] = v; }
      });
    });
    macro(macros, { label: '↻', title: 'Facing', range: [0, 360], format: degrees, fallback: 0, read: () => current().yawDegrees, write: v => { current().yawDegrees = v % 360; } });
    macro(macros, { label: '⤢', title: 'Tilt', range: [-90, 90], format: degrees, fallback: FLAT_KINDS.includes(object.kind) ? current().tiltDegrees : 0, read: () => current().tiltDegrees, write: v => { current().tiltDegrees = v; } });
    if (object.kind === 'grating') {
      macro(macros, { label: '▥', title: 'Bar spacing', range: [0.002, 0.5], log: true, format: formatGap, fallback: 0.034, read: () => current().pattern.periodMetres[0], write: v => { const p = current().pattern; p.periodMetres = p.periodMetres.map(x => (x > 0 ? v : 0)); p.barMetres = p.barMetres.map(b => Math.min(b, v)); } });
      macro(macros, { label: '▮', title: 'Bar width', range: [0.001, 0.2], log: true, format: formatGap, fallback: 0.004, read: () => current().pattern.barMetres[0], write: v => { const p = current().pattern; p.barMetres = p.barMetres.map((b, i) => (p.periodMetres[i] > 0 ? Math.min(v, p.periodMetres[i]) : 0)); } });
    }
    if (object.kind === 'wall') {
      macro(macros, { label: '◯', title: 'Opening', range: [0.02, 5], log: true, format: formatGap, fallback: 0.4, read: () => current().opening.sizeMetres[0], write: v => { const o = current().opening; o.sizeMetres = [Math.min(v, current().size[0]), Math.min(v, current().size[2])]; } });
    }
    macro(macros, { label: '%', title: 'Strength', range: [0, 2], fallback: 1, format: v => `${Math.round(v * 100)} %`, read: () => current().strength, write: v => { current().strength = v; } });
    card.append(macros);
    const material = element('div', 'fields');
    fields.selectField(material, {
      label: 'Material', value: object.material, reliability: MATERIALS[object.material].reliability,
      options: Object.entries(MATERIALS).map(([id, m]) => [id, m.label]),
      onCommit: v => { current().material = v; changed({ panel: true }); }
    });
    card.append(material);
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

function removeObject(id) {
  state.space.objects = state.space.objects.filter(o => o.id !== id);
  if (state.selection?.id === id) state.selection = null;
  changed({ panel: true });
}

function refreshPanel() {
  live.length = 0;
  const list = $('object-list');
  list.replaceChildren();
  state.space.objects.forEach(object => objectCard(list, object));

  const microphone = $('microphone-macros');
  microphone.replaceChildren();
  const position = () => state.space.microphone.position;
  const nearest = nearestSurface(state.space);
  if (nearest) {
    const name = state.space.objects[nearest.index].name;
    macro(microphone, {
      label: `Gap to ${name}`, range: GAP_RANGE_METRES, log: true, fallback: 0.3, format: formatGap,
      read: () => Math.abs(nearestSurface(state.space)?.distance ?? GAP_RANGE_METRES[1]),
      write: v => { state.space.microphone.position = withGap(state.space, nearestSurface(state.space) ?? nearest, v); }
    });
  }
  macro(microphone, { label: 'East', range: HORIZONTAL_RANGE_METRES, fallback: 0, format: metres, read: () => position()[0], write: v => { position()[0] = v; } });
  macro(microphone, { label: 'North', range: HORIZONTAL_RANGE_METRES, fallback: 0, format: metres, read: () => position()[1], write: v => { position()[1] = v; } });
  macro(microphone, { label: 'Height', range: HEIGHT_RANGE_METRES, fallback: 1.2, format: metres, read: () => position()[2], write: v => { position()[2] = v; } });

  const arrival = $('arrival-macros');
  arrival.replaceChildren();
  macro(arrival, { label: 'From', range: [0, 360], fallback: 0, format: degrees, read: () => state.space.arrival.azimuthDegrees, write: v => { state.space.arrival.azimuthDegrees = v % 360; } });
  macro(arrival, { label: 'Elevation', range: [-90, 90], fallback: 10, format: degrees, read: () => state.space.arrival.elevationDegrees, write: v => { state.space.arrival.elevationDegrees = v; } });

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
    fields.numberField(shape, { key: 'sizeMetres', label: sizeNames[label], value: object.size[axis], onCommit: v => update(o => { o.size[axis] = v; if (o.kind === 'pipe' && axis === 1) o.size[2] = v; }) });
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
  const at = state.space.microphone.position;
  const placements = {
    plane: { position: [at[0], at[1] + 0.5, at[2]], yawDegrees: 180 },
    disc: { position: [at[0], at[1] + 0.5, at[2]], yawDegrees: 180 },
    grating: { position: [at[0], at[1], Math.max(0.01, at[2] - 0.3)], tiltDegrees: 90, material: 'rigid' },
    wall: { position: [at[0], at[1] + 0.5, 1.25], yawDegrees: 180, material: 'brick' },
    box: { position: [at[0] + 0.6, at[1] + 0.3, 0.3], material: 'woodPanel' },
    corner: { position: [at[0] - 0.4, at[1] - 0.4, 0], material: 'plaster' },
    recess: { position: [at[0], at[1] + 0.3, at[2]], yawDegrees: 180, material: 'plaster' },
    pipe: { position: [at[0], at[1] + 0.4, at[2]], material: 'concrete' }
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
    view.centre(state.space.microphone.position);
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
  if (event.key === 'Escape' && state.selection) select(null);
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
  $('export-summary').textContent = `${state.space.render.sampleRateHz / 1000} kHz · 32-bit float · ${state.space.render.lengthMilliseconds} ms filter`;
  $('export-error').hidden = true;
  exportDialog.showModal();
  exportDialog.focus();
});
$('export-filter').addEventListener('click', async () => {
  const result = await computeFilter(clone(state.space));
  download('neartect-filter.wav', new Blob([encodeFloat32Wav([result.samples], result.sampleRateHz)], { type: 'audio/wav' }));
});
$('export-processed').addEventListener('click', async () => {
  try {
    const { samples, sampleRate } = await signalSamples(state.space.render.sampleRateHz);
    const space = clone(state.space);
    space.render.sampleRateHz = sampleRate;
    const filter = await computeFilter(space);
    const out = new Float32Array(samples.length + filter.samples.length - 1);
    const offline = new OfflineAudioContext(1, out.length, sampleRate);
    const input = offline.createBuffer(1, samples.length, sampleRate);
    input.copyToChannel(samples, 0);
    const response = offline.createBuffer(1, filter.samples.length, sampleRate);
    response.copyToChannel(filter.samples, 0);
    const source = offline.createBufferSource();
    source.buffer = input;
    const convolver = offline.createConvolver();
    convolver.normalize = false;
    convolver.buffer = response;
    source.connect(convolver).connect(offline.destination);
    source.start();
    const rendered = await offline.startRendering();
    rendered.copyFromChannel(out, 0);
    download('neartect-filtered.wav', new Blob([encodeFloat32Wav([out], sampleRate)], { type: 'audio/wav' }));
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

const preferences = { panelWidth: 340, resultsHeight: 220, ...(readStored(PREFERENCES_KEY) ?? {}) };
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
