import { stereo } from './audio/preview.js';

export const inPlugin = typeof window.sendToPlugin === 'function';

export function createPluginHost({ onSpace, onOpen, onBypass, onRate }) {
  if (!inPlugin) return null;
  const send = message => window.sendToPlugin(message);
  let rate = null;
  window.onPluginMessage = message => {
    if (message.type === 'init') {
      rate = message.sampleRateHz;
      if (message.space) onSpace(message.space);
      onBypass(message.bypass);
      onRate(rate);
    }
    if (message.type === 'opened') onOpen(message.text);
    if (message.type === 'bypass') onBypass(message.on);
  };
  window.addEventListener('error', event => send({ type: 'log', text: `${event.message} ${event.filename}:${event.lineno}` }));
  window.addEventListener('unhandledrejection', event => send({ type: 'log', text: String(event.reason?.stack ?? event.reason) }));
  document.body.classList.add('in-plugin');
  addGrip(send);
  return {
    space(space) {
      send({ type: 'space', json: JSON.stringify(space) });
    },
    filter(result) {
      if (result.sampleRateHz !== rate) return;
      const [left, right] = stereo(result.channels, result.pans);
      send({ type: 'filter', sampleRateHz: result.sampleRateHz, left: Array.from(left), right: Array.from(right) });
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
