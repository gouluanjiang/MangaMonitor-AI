export type CoverPriority = "visible" | "nearby";
export interface CoverJob<T> {
  promise: Promise<T | null>;
  cancel(): boolean;
  setPriority(priority: CoverPriority): void;
}
type Task<T> = {
  run(): Promise<T>;
  resolve(value: T | null): void;
  reject(error: unknown): void;
  priority: CoverPriority;
};

/** The renderer owns ordering; native services enforce the same concurrency caps. */
export class CoverScheduler<T> {
  private waiting: Task<T>[] = [];
  private running = new Set<Task<T>>();
  private scheduled = false;
  private maximumActive: number;
  private maximumWaiting: number;
  constructor(maximumActive: number, maximumWaiting = 64) {
    this.maximumActive = maximumActive;
    this.maximumWaiting = maximumWaiting;
  }
  enqueue(
    run: () => Promise<T>,
    priority: CoverPriority = "visible",
  ): CoverJob<T> {
    let task: Task<T>;
    const promise = new Promise<T | null>((resolve, reject) => {
      task = { run, resolve, reject, priority };
      if (this.waiting.length >= this.maximumWaiting) {
        reject(new Error("COVER_QUEUE_FULL"));
        return;
      }
      this.waiting.push(task);
      this.schedule();
    });
    return {
      promise,
      cancel: () => {
        const index = this.waiting.indexOf(task);
        if (index < 0) return false;
        this.waiting.splice(index, 1);
        task.resolve(null);
        return true;
      },
      setPriority: (next) => {
        task.priority = next;
        this.schedule();
      },
    };
  }
  private schedule() {
    if (this.scheduled) return;
    this.scheduled = true;
    queueMicrotask(() => {
      this.scheduled = false;
      this.drain();
    });
  }
  private drain() {
    while (this.running.size < this.maximumActive && this.waiting.length) {
      let index = this.waiting.findIndex((task) => task.priority === "visible");
      if (index < 0) {
        // One speculative thumbnail leaves capacity for cards entering the viewport.
        if ([...this.running].some((task) => task.priority === "nearby"))
          return;
        index = 0;
      }
      const [task] = this.waiting.splice(index, 1);
      this.running.add(task);
      void (async () => task.run())()
        .then(task.resolve, task.reject)
        .finally(() => {
          this.running.delete(task);
          this.schedule();
        });
    }
  }
}
