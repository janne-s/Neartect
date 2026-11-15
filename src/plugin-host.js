import { FilterCancelled } from './filter-pool.js';
import { createParameterSync } from './plugin-parameters.js';

export const inPlugin = typeof window.sendToPlugin === 'function';

export function createPluginHost({ onSpace, onOpen, onBypass, onRate, onParameters, currentSpace }) {
  if (!inPlugin) return null;
  const send = message => window.sendToPlugin(message);
  let rate = null;
  let asked = 0;
  const parameters = createParameterSync(message => send(message));
  const waiting = new Map();
  window.onPluginMessage = message => {
    if (message.type === 'init') {
      rate = message.sampleRateHz;
      parameters.start(message.parameters ?? [], message.values ?? {});
      if (message.space) onSpace(message.space);
      onBypass(message.bypass);
      onRate(rate);
      parameters.push(currentSpace(), true);
      send({ type: 'synced' });
    }
    if (message.type === 'opened') onOpen(message.text);
    if (message.type === 'params') onParameters(message.values);
    if (message.type === 'bypass') onBypass(message.on);
    if (message.type === 'computed') {
      const wait = waiting.get(message.id);
      if (!wait) return;
      waiting.delete(message.id);
      if (message.cancelled) wait.reject(new FilterCancelled());
      else if (message.error) wait.reject(new Error(message.error));
      else wait.resolve({ sampleRateHz: message.sampleRateHz, channels: message.channels.map(c => Float32Array.from(c)), names: message.names, pans: message.pans });
    }
  };
  window.addEventListener('error', event => send({ type: 'log', text: `${event.message} ${event.filename}:${event.lineno}` }));
  window.addEventListener('unhandledrejection', event => send({ type: 'log', text: String(event.reason?.stack ?? event.reason) }));
  document.body.classList.add('in-plugin');
  addGrip(send);
  return {
    parameters,
    space(space) {
      send({ type: 'space', json: JSON.stringify(space) });
    },
    compute(space, purpose) {
      asked += 1;
      const id = asked;
      send({ type: 'compute', id, json: JSON.stringify(space), purpose });
      return new Promise((resolve, reject) => waiting.set(id, { resolve, reject }));
    },
    ready() {
      send({ type: 'ready' });
    },
    bypass(on) {
      send({ type: 'bypass', on });
    },
    open() {
      send({ type: 'open' });
    },
    save(name, blob) {
      const reader = new FileReader();
      reader.addEventListener('load', () => send({ type: 'save', name, base64: reader.result.slice(reader.result.indexOf(',') + 1) }));
      reader.readAsDataURL(blob);
    }
  };
}

function addGrip(send) {
  const grip = document.createElement('div');
  grip.className = 'plugin-grip';
  document.body.append(grip);
  grip.addEventListener('pointerdown', event => {
    event.preventDefault();
    grip.setPointerCapture(event.pointerId);
    const start = [event.screenX, event.screenY, window.innerWidth, window.innerHeight];
    const move = moved => send({ type: 'resize', width: Math.round(start[2] + moved.screenX - start[0]), height: Math.round(start[3] + moved.screenY - start[1]) });
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', () => grip.removeEventListener('pointermove', move), { once: true });
  });
}
