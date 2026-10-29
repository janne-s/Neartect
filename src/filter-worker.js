import { nearFilter } from './domain/near-filter.js';

self.addEventListener('message', async ({ data: { id, space } }) => {
  try {
    const started = performance.now();
    const { channels, names, pans, sampleRateHz } = await nearFilter(space);
    self.postMessage({ id, channels, names, pans, sampleRateHz, milliseconds: performance.now() - started }, channels.map(c => c.buffer));
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
});
