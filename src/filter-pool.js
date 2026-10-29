export class FilterCancelled extends Error {}

// Jobs wait in one queue and go to whichever worker is free. A build's jobs
// that have not started can be dropped; a worker that fails takes its job's
// error and returns to the pool.
export function createPool(url, count) {
  const workers = Array.from({ length: count }, () => new Worker(url, { type: 'module' }));
  const idle = [...workers];
  const queue = [];
  const running = new Map();
  let lastId = 0;

  function dispatch() {
    while (idle.length && queue.length) {
      const worker = idle.pop();
      const task = queue.shift();
      lastId += 1;
      running.set(lastId, { ...task, worker });
      worker.postMessage({ id: lastId, job: task.job });
    }
  }

  function settle(id, settleTask) {
    const task = running.get(id);
    if (!task) return;
    running.delete(id);
    idle.push(task.worker);
    settleTask(task);
    dispatch();
  }

  workers.forEach(worker => {
    worker.addEventListener('message', ({ data }) => settle(data.id, task => (data.error ? task.reject(new Error(data.error)) : task.resolve(data.result))));
    worker.addEventListener('error', event => {
      event.preventDefault();
      const failed = [...running].find(([, task]) => task.worker === worker);
      if (failed) settle(failed[0], task => task.reject(new Error(event.message || 'The filter worker failed.')));
    });
  });

  return {
    runner(build) {
      return job => new Promise((resolve, reject) => {
        queue.push({ job, build, resolve, reject });
        dispatch();
      });
    },
    cancel(build) {
      for (let index = queue.length - 1; index >= 0; index -= 1) {
        if (queue[index].build !== build) continue;
        queue[index].reject(new FilterCancelled());
        queue.splice(index, 1);
      }
    }
  };
}
