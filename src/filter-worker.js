import { channelFilter } from './domain/near-filter.js';

self.addEventListener('message', async ({ data: { id, space, index } }) => {
  try {
    const samples = await channelFilter(space, index);
    self.postMessage({ id, samples }, [samples.buffer]);
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
});
