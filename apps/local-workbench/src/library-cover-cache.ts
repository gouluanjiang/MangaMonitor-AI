import type { LibraryAdapter, LibraryCover } from "./library-types.ts";
import { CoverScheduler } from "./cover-scheduler.ts";
import type { CoverJob, CoverPriority } from "./cover-scheduler.ts";

export type LibraryCoverResult =
  | { status: "ready"; url: string }
  | { status: "error" }
  | { status: "cancelled" };
export interface LibraryCoverLease {
  promise: Promise<LibraryCoverResult>;
  release(): void;
  setPriority(priority: CoverPriority): void;
}
type Ready = {
  result: Extract<LibraryCoverResult, { status: "ready" }>;
  bytes: number;
  users: number;
  token: symbol;
};
type Job = {
  key: string;
  epoch: number;
  users: number;
  task: CoverJob<LibraryCover>;
  priorities: Map<symbol, CoverPriority>;
  token: symbol;
  promise: Promise<LibraryCoverResult>;
};
const keyOf = (rootId: string, generation: number, entryId: string) =>
  JSON.stringify([rootId, generation, entryId]);
/** Compressed blobs last for this process; image elements release decoded pixels offscreen. */
export class LibraryCoverCache {
  private maxBytes: number;
  private ready = new Map<string, Ready>();
  private pending = new Map<string, Job>();
  private failed = new Set<string>();
  private queue = new CoverScheduler<LibraryCover>(2, 128);
  private bytes = 0;
  private epoch = 0;
  private scope = "";
  constructor(maxBytes = 64 * 1024 * 1024) {
    this.maxBytes = maxBytes;
  }
  setScope(rootId: string | null, generation: number) {
    const scope = JSON.stringify([rootId, generation]);
    if (scope !== this.scope) {
      this.clear();
      this.scope = scope;
    }
  }
  peek(
    rootId: string,
    generation: number,
    entryId: string,
  ): LibraryCoverResult | undefined {
    const key = keyOf(rootId, generation, entryId);
    return (
      this.ready.get(key)?.result ??
      (this.failed.has(key) ? { status: "error" } : undefined)
    );
  }
  retry(rootId: string, generation: number, entryId: string) {
    this.failed.delete(keyOf(rootId, generation, entryId));
  }
  invalidate(rootId: string, generation: number, entryId: string, url: string) {
    const key = keyOf(rootId, generation, entryId),
      entry = this.ready.get(key);
    // A late error from a revoked image must not invalidate its replacement.
    if (!entry || entry.result.url !== url) return false;
    URL.revokeObjectURL(entry.result.url);
    this.bytes -= entry.bytes;
    this.ready.delete(key);
    this.failed.add(key);
    return true;
  }
  acquire(
    rootId: string,
    generation: number,
    entryId: string,
    load: () => Promise<LibraryCover>,
    priority: CoverPriority = "visible",
  ): LibraryCoverLease {
    const key = keyOf(rootId, generation, entryId);
    const cached = this.ready.get(key);
    if (cached) {
      cached.users++;
      this.ready.delete(key);
      this.ready.set(key, cached);
      let released = false;
      return {
        promise: Promise.resolve(cached.result),
        setPriority() {},
        release: () => {
          if (!released) {
            released = true;
            cached.users--;
          }
        },
      };
    }
    if (this.failed.has(key))
      return {
        promise: Promise.resolve({ status: "error" }),
        setPriority() {},
        release: () => {},
      };
    let job = this.pending.get(key);
    if (!job) {
      job = {
        key,
        epoch: this.epoch,
        users: 0,
        task: this.queue.enqueue(load, priority),
        priorities: new Map(),
        token: Symbol(),
        promise: Promise.resolve({ status: "cancelled" }),
      };
      this.pending.set(key, job);
      job.promise = this.finish(job);
    }
    job.users++;
    const leaseJob = job;
    const consumer = Symbol();
    leaseJob.priorities.set(consumer, priority);
    const reprioritize = () =>
      leaseJob.task.setPriority(
        [...leaseJob.priorities.values()].includes("visible")
          ? "visible"
          : "nearby",
      );
    reprioritize();
    let released = false;
    return {
      promise: job.promise,
      setPriority: (next) => {
        if (released) return;
        leaseJob.priorities.set(consumer, next);
        reprioritize();
      },
      release: () => {
        if (!released) {
          released = true;
          leaseJob.users--;
          leaseJob.priorities.delete(consumer);
          reprioritize();
          const entry = this.ready.get(key);
          if (entry?.token === leaseJob.token) entry.users--;
          if (
            leaseJob.users === 0 &&
            leaseJob.task.cancel() &&
            this.pending.get(key) === leaseJob
          )
            this.pending.delete(key);
        }
      },
    };
  }
  private async finish(job: Job): Promise<LibraryCoverResult> {
    try {
      const response = await job.task.promise;
      if (job.epoch !== this.epoch || response === null)
        return { status: "cancelled" };
      const match = /^data:image\/jpeg;base64,([A-Za-z0-9+/]+={0,2})$/.exec(
        response.dataUrl ?? "",
      );
      if (!match) throw new Error("cover");
      const raw = atob(match[1]);
      if (raw.length > 256 * 1024) throw new Error("cover");
      const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
      if (bytes.length > this.maxBytes) throw new Error("cache");
      for (const [key, entry] of this.ready) {
        if (this.bytes + bytes.length <= this.maxBytes) break;
        if (entry.users === 0) {
          URL.revokeObjectURL(entry.result.url);
          this.bytes -= entry.bytes;
          this.ready.delete(key);
        }
      }
      if (this.bytes + bytes.length > this.maxBytes)
        return { status: "cancelled" };
      const result = {
        status: "ready",
        url: URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })),
      } as const;
      this.ready.set(job.key, {
        result,
        bytes: bytes.length,
        users: job.users,
        token: job.token,
      });
      this.bytes += bytes.length;
      return result;
    } catch (cause) {
      if (
        job.epoch !== this.epoch ||
        (cause instanceof Error && cause.message === "COVER_QUEUE_FULL")
      )
        return { status: "cancelled" };
      if (job.epoch === this.epoch) {
        this.failed.add(job.key);
        if (this.failed.size > 20000)
          this.failed.delete(this.failed.values().next().value!);
      }
      return { status: "error" };
    } finally {
      if (this.pending.get(job.key) === job) this.pending.delete(job.key);
    }
  }
  clear() {
    this.epoch++;
    for (const entry of this.ready.values())
      URL.revokeObjectURL(entry.result.url);
    for (const job of this.pending.values()) job.task.cancel();
    this.ready.clear();
    this.failed.clear();
    this.pending.clear();
    this.bytes = 0;
  }
}
const caches = new WeakMap<LibraryAdapter, LibraryCoverCache>();
export function getLibraryCoverCache(adapter: LibraryAdapter) {
  let cache = caches.get(adapter);
  if (!cache) {
    cache = new LibraryCoverCache();
    caches.set(adapter, cache);
  }
  return cache;
}
