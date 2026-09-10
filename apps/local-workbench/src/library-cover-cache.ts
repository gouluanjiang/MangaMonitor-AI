import type { LibraryAdapter, LibraryCover } from "./library-types.ts";

export type LibraryCoverResult =
  | { status: "ready"; url: string }
  | { status: "error" }
  | { status: "cancelled" };
export interface LibraryCoverLease {
  promise: Promise<LibraryCoverResult>;
  release(): void;
}
type Ready = {
  result: Extract<LibraryCoverResult, { status: "ready" }>;
  bytes: number;
  users: number;
};
type Job = {
  key: string;
  epoch: number;
  users: number;
  load: () => Promise<LibraryCover>;
  resolve: (result: LibraryCoverResult) => void;
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
  private queue: Job[] = [];
  private busy = false;
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
  invalidate(rootId: string, generation: number, entryId: string) {
    const key = keyOf(rootId, generation, entryId),
      entry = this.ready.get(key);
    if (entry) {
      URL.revokeObjectURL(entry.result.url);
      this.bytes -= entry.bytes;
      this.ready.delete(key);
    }
    this.failed.add(key);
  }
  acquire(
    rootId: string,
    generation: number,
    entryId: string,
    load: () => Promise<LibraryCover>,
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
        release: () => {},
      };
    let job = this.pending.get(key);
    if (!job) {
      if (this.queue.length >= 128)
        return {
          promise: Promise.resolve({ status: "cancelled" }),
          release: () => {},
        };
      let resolve!: (result: LibraryCoverResult) => void;
      const promise = new Promise<LibraryCoverResult>((r) => {
        resolve = r;
      });
      job = { key, epoch: this.epoch, users: 0, load, resolve, promise };
      this.pending.set(key, job);
      this.queue.push(job);
    }
    job.users++;
    const leaseJob = job;
    let released = false;
    void this.pump();
    return {
      promise: job.promise,
      release: () => {
        if (!released) {
          released = true;
          leaseJob.users--;
          const entry = this.ready.get(key);
          if (entry) entry.users = Math.max(0, entry.users - 1);
        }
      },
    };
  }
  private async pump() {
    if (this.busy) return;
    this.busy = true;
    try {
      while (this.queue.length) {
        const job = this.queue.shift()!;
        if (job.epoch !== this.epoch || job.users === 0) {
          this.pending.delete(job.key);
          job.resolve({ status: "cancelled" });
          continue;
        }
        let result: LibraryCoverResult = { status: "error" };
        try {
          const response = await job.load();
          if (job.epoch !== this.epoch) {
            job.resolve({ status: "cancelled" });
            continue;
          }
          const match = /^data:image\/jpeg;base64,([A-Za-z0-9+/]+={0,2})$/.exec(
            response.dataUrl ?? "",
          );
          if (!match) throw new Error("cover");
          const raw = atob(match[1]);
          if (raw.length > 256 * 1024) throw new Error("cover");
          const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
          for (const [key, entry] of this.ready) {
            if (this.bytes + bytes.length <= this.maxBytes) break;
            if (entry.users === 0) {
              URL.revokeObjectURL(entry.result.url);
              this.bytes -= entry.bytes;
              this.ready.delete(key);
            }
          }
          if (this.bytes + bytes.length > this.maxBytes)
            throw new Error("cache");
          result = {
            status: "ready",
            url: URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })),
          };
          this.ready.set(job.key, {
            result,
            bytes: bytes.length,
            users: job.users,
          });
          this.bytes += bytes.length;
        } catch {
          if (job.epoch === this.epoch) {
            this.failed.add(job.key);
            if (this.failed.size > 20000)
              this.failed.delete(this.failed.values().next().value!);
          }
        }
        if (job.epoch === this.epoch) this.pending.delete(job.key);
        job.resolve(result);
      }
    } finally {
      this.busy = false;
    }
  }
  clear() {
    this.epoch++;
    for (const entry of this.ready.values())
      URL.revokeObjectURL(entry.result.url);
    for (const job of this.queue) job.resolve({ status: "cancelled" });
    this.ready.clear();
    this.failed.clear();
    this.pending.clear();
    this.queue = [];
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
