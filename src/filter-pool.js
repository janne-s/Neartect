export class FilterCancelled extends Error {}

// What the jobs running at once may hold, as in Rumbletect's tail budget: a
// phone tab is stopped at about 1.5 GB. A job larger than this runs alone.
const RUNNING_BYTES_BUDGET = 512 * 2 ** 20;
// A WebAssembly memory never shrinks, so a worker that grew past this is
// replaced by a fresh one once its job is done.
const RETAINED_BYTES = 64 * 2 ** 20;
const STALL_SECONDS = 60;
const WATCH_MILLISECONDS = 5000;

// Jobs wait in one queue, in order, and go to whichever worker is free while
// their estimated memory fits the budget. A build's jobs that have not
// started can be dropped. A worker that fails, traps, or stops answering is
// replaced, and its job fails.
export function createPool(url, count, bytesOf) {
  const spawn = () => new Worker(url, { type: 'module' });
  const idle = Array.from({ length: count }, spawn);
  const queue = [];
  const running = new Map();
  let lastId = 0;
  let heard = performance.now();

  const runningBytes = () => [...running.values()].reduce((sum, task) => sum + task.bytes, 0);

  function dispatch() {
    while (idle.length && queue.length) {
      if (running.size && runningBytes() + queue[0].bytes > RUNNING_BYTES_BUDGET) return;
      const worker = idle.pop();
      const task = queue.shift();
      if (!running.size) heard = performance.now();
      lastId += 1;
      running.set(lastId, { ...task, worker });
      listen(worker);
      worker.postMessage({ id: lastId, job: task.job });
    }
  }

  function finish(id, { replace, settleTask }) {
    const task = running.get(id);
    if (!task) return;
    running.delete(id);
    heard = performance.now();
    if (replace) task.worker.terminate();
    idle.push(replace ? spawn() : task.worker);
    settleTask(task);
    dispatch();
  }

  const listening = new WeakSet();
  function listen(worker) {
    if (listening.has(worker)) return;
    listening.add(worker);
    worker.addEventListener('message', ({ data }) => finish(data.id, {
      replace: data.trapped || data.memoryBytes > RETAINED_BYTES,
      settleTask: task => (data.error ? task.reject(new Error(data.error)) : task.resolve(data.result))
    }));
    worker.addEventListener('error', event => {
      event.preventDefault();
      const failed = [...running].find(([, task]) => task.worker === worker);
      if (failed) finish(failed[0], { replace: true, settleTask: task => task.reject(new Error(event.message || 'The filter worker failed')) });
    });
  }

  setInterval(() => {
    if (!running.size || performance.now() - heard < STALL_SECONDS * 1000) return;
    for (const id of [...running.keys()]) {
      finish(id, { replace: true, settleTask: task => task.reject(new Error('The filter stopped responding, likely out of memory')) });
    }
  }, WATCH_MILLISECONDS);

  return {
    runner(build) {
      return job => new Promise((resolve, reject) => {
        queue.push({ job, build, bytes: bytesOf(job), resolve, reject });
        dispatch();
      });
    },
    cancel(build) {
      for (let index = queue.length - 1; index >= 0; index -= 1) {
        if (queue[index].build !== build) continue;
        queue[index].reject(new FilterCancelled());
        queue.splice(index, 1);
      }
      dispatch();
    }
  };
}
