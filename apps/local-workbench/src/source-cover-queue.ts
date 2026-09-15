type Task = {
  run(): Promise<string | null>;
  resolve(value: string | null): void;
  reject(error: unknown): void;
  cancelled: boolean;
};
const queue: Task[] = [];
let active = 0;
const MAX_ACTIVE = 2;
const MAX_WAITING = 64;
function drain() {
  while (active < MAX_ACTIVE && queue.length) {
    const task = queue.shift()!;
    if (task.cancelled) continue;
    active++;
    void Promise.resolve()
      .then(task.run)
      .then(task.resolve, task.reject)
      .finally(() => {
        active--;
        drain();
      });
  }
}
export function queueCover(run: () => Promise<string | null>) {
  let task: Task;
  const promise = new Promise<string | null>((resolve, reject) => {
    task = { run, resolve, reject, cancelled: false };
    if (queue.length >= MAX_WAITING) {
      reject(new Error("COVER_QUEUE_FULL"));
      return;
    }
    queue.push(task);
    drain();
  });
  return {
    promise,
    cancel() {
      if (!task || task.cancelled) return false;
      const index = queue.indexOf(task);
      if (index >= 0) {
        queue.splice(index, 1);
        task.cancelled = true;
        task.resolve(null);
        return true;
      }
      return false;
    },
  };
}
