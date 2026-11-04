import { stereo } from './audio/preview.js';

export function createPluginHost({ onSpace, onRate }) {
  if (typeof window.sendToPlugin !== 'function') return null;
  const send = message => window.sendToPlugin(message);
  let rate = null;
  window.onPluginMessage = message => {
    if (message.type !== 'init') return;
    rate = message.sampleRateHz;
    if (message.space) onSpace(message.space);
    onRate(rate);
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
