import { createFields } from './shared/ui/fields.js';
import { attachSplitters } from './shared/ui/splitters.js';
import { encodeFloat32Wav } from './shared/domain/wav.js';
import { defaultSpace, defaultSurface, readSpace, SpaceError } from './domain/space.js';
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
const SHEET_TITLES = Object.freeze({ emitters: 'Surfaces', receivers: 'Microphone', output: 'Air and output' });

const ADD_SURFACES = Object.freeze({
  wall: { label: 'Wall', make: at => defaultSurface({ name: 'Wall', centre: [at[0], at[1] - 0.5, 1.25], widthMetres: 3, heightMetres: 2.5, material: 'brick' }) },
  floor: { label: 'Floor', make: at => defaultSurface({ name: 'Floor', centre: [at[0], at[1], 0], widthMetres: 4, heightMetres: 4, tiltDegrees: 90, material: 'concrete' }) },
  panel: { label: 'Panel', make: at => defaultSurface({ name: 'Panel', centre: [at[0], at[1] + 0.4, at[2]], widthMetres: 0.4, heightMetres: 0.4, yawDegrees: 180, material: 'woodPanel' }) },
  grating: {
    label: 'Grating',
    make: at => defaultSurface({
      name: 'Grating', centre: [at[0], at[1], Math.max(0.01, at[2] - 0.3)], widthMetres: 1, heightMetres: 1, tiltDegrees: 90, material: 'rigid',
      pattern: { kind: 'grid', periodMetres: [0.034, 0.034], barMetres: [0.004, 0.004], thicknessMetres: 0.03 }
    })
  }
});

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
const view = createNearView($('near-view'), { onSelect: selection => select(selection) });

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

function changed({ save = true, panel = false } = {}) {
  if (save) writeStored(STORAGE_KEY, state.space);
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

function macro(container, { label, read, write, range: [low, high], log = false, format }) {
  const toSlider = value => Math.round(SLIDER_STEPS * (log ? Math.log(value / low) / Math.log(high / low) : (value - low) / (high - low)));
  const fromSlider = position => {
    const t = position / SLIDER_STEPS;
    return log ? low * (high / low) ** t : low + t * (high - low);
  };
  const name = element('label', null, label);
  const input = element('input');
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
  container.append(name, input, value);
  live.push(sync);
  sync();
}

const degrees = value => `${Math.round(value)}°`;
const metres = value => formatGap(Math.abs(value)).replace(/^/, value < 0 ? '−' : '');

function surfaceCard(container, surface, index) {
  const selected = state.selection?.type === 'surface' && state.selection.index === index;
  const card = element('div', 'card');
  card.setAttribute('aria-selected', String(selected));
  const header = element('div', 'card-header');
  header.tabIndex = 0;
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', String(selected));
  const enabled = element('input');
  enabled.type = 'checkbox';
  enabled.checked = surface.strength > 0;
  enabled.title = 'Enabled';
  enabled.addEventListener('click', event => event.stopPropagation());
  enabled.addEventListener('change', () => {
    surface.strength = enabled.checked ? 1 : 0;
    changed({ panel: true });
  });
  const remove = element('button', 'icon-button');
  remove.type = 'button';
  remove.title = 'Remove';
  remove.setAttribute('aria-label', `Remove ${surface.name}`);
  remove.innerHTML = '<i class="fa-solid fa-xmark" aria-hidden="true"></i>';
  remove.addEventListener('click', event => {
    event.stopPropagation();
    state.space.surfaces.splice(index, 1);
    state.selection = null;
    changed({ panel: true });
  });
  const toggle = () => select(selected ? null : { type: 'surface', index });
  header.addEventListener('click', toggle);
  header.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); toggle(); } });
  const kind = surface.pattern.kind === 'grid' ? 'Grating' : MATERIALS[surface.material].label;
  header.append(enabled, element('span', 'name', surface.name), element('span', 'kind', kind), remove);
  card.append(header);
  if (selected) {
    card.append(element('p', 'card-summary', `${formatGap(surface.widthMetres)} × ${formatGap(surface.heightMetres)} · ${MATERIALS[surface.material].label}`));
    const macros = element('div', 'macros');
    const current = () => state.space.surfaces[index];
    macro(macros, { label: 'Width', range: [0.05, 20], log: true, format: formatGap, read: () => current().widthMetres, write: v => { current().widthMetres = v; } });
    macro(macros, { label: 'Height', range: [0.05, 20], log: true, format: formatGap, read: () => current().heightMetres, write: v => { current().heightMetres = v; } });
    macro(macros, { label: 'Facing', range: [0, 360], format: degrees, read: () => current().yawDegrees, write: v => { current().yawDegrees = v % 360; } });
    macro(macros, { label: 'Tilt', range: [-90, 90], format: degrees, read: () => current().tiltDegrees, write: v => { current().tiltDegrees = v; } });
    macro(macros, { label: 'Strength', range: [0, 2], format: v => `${Math.round(v * 100)} %`, read: () => current().strength, write: v => { current().strength = v; } });
    card.append(macros);
    const actions = element('div', 'card-actions');
    const button = (text, action) => {
      const b = element('button', null, text);
      b.type = 'button';
      b.addEventListener('click', action);
      actions.append(b);
    };
    button('Details…', () => openDetails(index));
    button('Duplicate', () => {
      const copy = clone(surface);
      copy.name = `${surface.name} copy`;
      copy.centre = copy.centre.map((v, i) => v + (i === 0 ? 0.2 : 0));
      state.space.surfaces.push(copy);
      state.selection = { type: 'surface', index: state.space.surfaces.length - 1 };
      changed({ panel: true });
    });
    card.append(actions);
  }
  container.append(card);
}

function refreshPanel() {
  live.length = 0;
  const list = $('surface-list');
  list.replaceChildren();
  state.space.surfaces.forEach((surface, index) => surfaceCard(list, surface, index));

  const microphone = $('microphone-macros');
  microphone.replaceChildren();
  const position = () => state.space.microphone.position;
  const nearest = nearestSurface(state.space);
  if (nearest) {
    const name = state.space.surfaces[nearest.index].name;
    macro(microphone, {
      label: `Gap to ${name}`, range: GAP_RANGE_METRES, log: true, format: formatGap,
      read: () => Math.abs(nearestSurface(state.space)?.distance ?? GAP_RANGE_METRES[1]),
      write: v => { state.space.microphone.position = withGap(state.space, nearest.index, v); }
    });
  }
  macro(microphone, { label: 'East', range: HORIZONTAL_RANGE_METRES, format: metres, read: () => position()[0], write: v => { position()[0] = v; } });
  macro(microphone, { label: 'North', range: HORIZONTAL_RANGE_METRES, format: metres, read: () => position()[1], write: v => { position()[1] = v; } });
  macro(microphone, { label: 'Height', range: HEIGHT_RANGE_METRES, format: metres, read: () => position()[2], write: v => { position()[2] = v; } });

  const arrival = $('arrival-macros');
  arrival.replaceChildren();
  macro(arrival, { label: 'From', range: [0, 360], format: degrees, read: () => state.space.arrival.azimuthDegrees, write: v => { state.space.arrival.azimuthDegrees = v % 360; } });
  macro(arrival, { label: 'Elevation', range: [-90, 90], format: degrees, read: () => state.space.arrival.elevationDegrees, write: v => { state.space.arrival.elevationDegrees = v; } });

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

function openDetails(index) {
  const surface = state.space.surfaces[index];
  const dialog = $('details');
  $('details-title').textContent = surface.name;
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
  const update = (apply, rebuild = false) => { apply(state.space.surfaces[index]); changed({ panel: true }); if (rebuild) openDetails(index); };
  const shape = group('Shape');
  fields.textField(shape, { label: 'Name', value: surface.name, onCommit: v => update(s => { s.name = v; $('details-title').textContent = v; }) });
  ['East', 'North', 'Height'].forEach((label, axis) => fields.numberField(shape, {
    key: 'positionMetres', label: `Centre ${label.toLowerCase()}`, value: surface.centre[axis], onCommit: v => update(s => { s.centre[axis] = v; })
  }));
  fields.numberField(shape, { key: 'sizeMetres', label: 'Width', value: surface.widthMetres, onCommit: v => update(s => { s.widthMetres = v; }) });
  fields.numberField(shape, { key: 'sizeMetres', label: 'Height', value: surface.heightMetres, onCommit: v => update(s => { s.heightMetres = v; }) });
  fields.numberField(shape, { key: 'yawDegrees', value: surface.yawDegrees, onCommit: v => update(s => { s.yawDegrees = v % 360; }) });
  fields.numberField(shape, { key: 'tiltDegrees', value: surface.tiltDegrees, onCommit: v => update(s => { s.tiltDegrees = v; }) });

  const acoustics = group('Acoustics');
  fields.selectField(acoustics, {
    label: 'Material', value: surface.material, reliability: MATERIALS[surface.material].reliability,
    options: Object.entries(MATERIALS).map(([id, m]) => [id, m.label]),
    onCommit: v => update(s => { s.material = v; }, true)
  });
  fields.numberField(acoustics, { key: 'strength', value: surface.strength, onCommit: v => update(s => { s.strength = v; }) });

  const pattern = group('Pattern');
  fields.selectField(pattern, {
    label: 'Pattern', value: surface.pattern.kind, reliability: surface.pattern.kind === 'grid' ? 'B' : 'A',
    options: [['solid', 'Solid'], ['grid', 'Grating']],
    onCommit: v => update(s => {
      s.pattern = v === 'grid' ? { kind: 'grid', periodMetres: [0.034, 0.034], barMetres: [0.004, 0.004], thicknessMetres: 0.03 } : { kind: 'solid' };
    }, true)
  });
  if (surface.pattern.kind === 'grid') {
    ['across', 'up'].forEach((axis, i) => {
      fields.numberField(pattern, { key: 'periodMetres', label: `Period ${axis}`, reliability: 'A', value: surface.pattern.periodMetres[i], onCommit: v => update(s => { s.pattern.periodMetres[i] = v; s.pattern.barMetres[i] = Math.min(s.pattern.barMetres[i], v); }) });
      fields.numberField(pattern, { key: 'barMetres', label: `Bar ${axis}`, reliability: 'A', value: surface.pattern.barMetres[i], onCommit: v => update(s => { s.pattern.barMetres[i] = Math.min(v, s.pattern.periodMetres[i]); }) });
    });
    fields.numberField(pattern, { key: 'thicknessMetres', value: surface.pattern.thicknessMetres, onCommit: v => update(s => { s.pattern.thicknessMetres = v; }) });
  }
  if (!dialog.open) {
    dialog.showModal();
    dialog.focus();
  }
}
$('details-close').addEventListener('click', () => $('details').close());

for (const [id, { label, make }] of Object.entries(ADD_SURFACES)) {
  const button = element('button', null, `+ ${label}`);
  button.type = 'button';
  button.addEventListener('click', () => {
    state.space.surfaces.push(make(state.space.microphone.position));
    state.selection = { type: 'surface', index: state.space.surfaces.length - 1 };
    changed({ panel: true });
  });
  button.id = `add-${id}`;
  $('add-surfaces').append(button);
}

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
  if ($('signal').value === 'file') { $('signal-file').click(); return; }
  state.signal = $('signal').value;
  if (preview.playing) startPlayback();
});
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
    $('signal').options[3].textContent = file.name;
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
