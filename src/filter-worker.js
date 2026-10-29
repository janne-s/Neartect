import { loadCore } from './domain/core.js';
import { runJob } from './domain/near-filter.js';

self.addEventListener('message', async ({ data: { id, job } }) => {
  try {
    const result = runJob(await loadCore(), job);
    self.postMessage({ id, result }, [result.buffer]);
  } catch (error) {
    self.postMessage({ id, error: error.message });
  }
});
