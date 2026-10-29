import { nearFilter } from './domain/near-filter.js';

self.addEventListener('message', async ({ data: { id, space } }) => {
  try {
    const started = performance.now();
    const { samples, sampleRateHz } = await nearFilter(space);
    self.postMessage({ id, samples, sampleRateHz, milliseconds: performance.now() - started }, [samples.buffer]);
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
});
