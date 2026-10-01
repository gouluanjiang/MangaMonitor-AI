import type { Source } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";

interface Job {
  source: Source;
  foreground(): boolean;
  current(): boolean;
  run(): Promise<void>;
  cancel(): void;
}

/** One request per source, two overall, at most one background request.
 * Native source ordering/rate limits remain authoritative. */
export class AuthorSearchScheduler {
  private waiting: Job[] = [];
  private running = new Set<Job>();
  async run<T>(
    source: Source,
    foreground: () => boolean,
    current: () => boolean,
    task: (queuedMs: number) => Promise<T>,
  ): Promise<T> {
    const queuedAt = performance.now();
    return new Promise<T>((resolve, reject) => {
      this.waiting.push({
        source,
        foreground,
        current,
        cancel: () => reject(new SourceError("SEARCH_CANCELLED")),
        run: async () => {
          try {
            if (!current()) throw new SourceError("SEARCH_CANCELLED");
            resolve(await task(performance.now() - queuedAt));
          } catch (cause) {
            reject(cause);
          }
        },
      });
      this.wake();
    });
  }
  wake() {
    this.waiting = this.waiting.filter((job) => {
      if (job.current()) return true;
      job.cancel();
      return false;
    });
    while (this.running.size < 2) {
      const available = this.waiting.filter(
        (job) =>
          ![...this.running].some((active) => active.source === job.source) &&
          (job.foreground() ||
            ![...this.running].some((active) => !active.foreground())),
      );
      const job = available.find((job) => job.foreground()) ?? available[0];
      if (!job) return;
      this.waiting.splice(this.waiting.indexOf(job), 1);
      this.running.add(job);
      void job.run().finally(() => {
        this.running.delete(job);
        this.wake();
      });
    }
  }
}
export const authorSearchScheduler = new AuthorSearchScheduler();
