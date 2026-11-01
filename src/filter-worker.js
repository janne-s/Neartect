import { loadCore } from './domain/core.js';
import { runJob } from './domain/near-filter.js';

// A trap leaves the core’s memory unknown, so the pool replaces this worker.
self.addEventListener('message', async ({ data: { id, job } }) => {
  const core = await loadCore();
  try {
    const result = runJob(core, job);
    self.postMessage({ id, result, memoryBytes: core.memoryBytes() }, [result.buffer]);
  } catch (error) {
    const trapped = error instanceof WebAssembly.RuntimeError;
    self.postMessage({ id, error: trapped ? 'This device ran out of memory computing the filter' : error.message, memoryBytes: Infinity, trapped });
  }
});
