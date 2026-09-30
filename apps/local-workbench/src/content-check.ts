import type { SourceAdapter, SourceScope, SourceWork } from "./source-types.ts";
import { sourceWorkKey } from "./source-types.ts";
import { compactWork } from "./source-memory.ts";
import { isContentHidden, rememberContentWork } from "./content-filter.ts";
import { SourceError } from "./source-runtime.ts";

export type ContentCheckState =
  | { phase: "waiting" | "checking" }
  | { phase: "ready"; work: SourceWork }
  | { phase: "error"; error: unknown };
interface Entry {
  version: string | null;
  generation: number;
  expiresAt: number;
  state: ContentCheckState;
  work: SourceWork;
  listeners: Set<(state: ContentCheckState) => void>;
  queued: boolean;
}

/** Only visible subscribers schedule requests. No catalog-wide detail fanout. */
export class ContentCheckPool {
  private entries = new Map<string, Entry>();
  private running = 0;
  private disposed = false;
  private adapter: SourceAdapter;
  readonly scope: SourceScope;
  private onResolved: (work: SourceWork) => void;
  private now: () => number;
  constructor(
    adapter: SourceAdapter,
    scope: SourceScope,
    onResolved: (work: SourceWork) => void = () => {},
    now: () => number = Date.now,
  ) {
    this.adapter = adapter;
    this.scope = scope;
    this.onResolved = onResolved;
    this.now = now;
  }
  private entry(work: SourceWork): Entry {
    const key = sourceWorkKey(work),
      version = work.sourceUpdatedAt ?? null;
    let entry = this.entries.get(key);
    if (
      entry &&
      (entry.version !== version ||
        (entry.state.phase === "ready" && entry.expiresAt <= this.now()))
    ) {
      entry.generation++;
      entry.version = version;
      entry.work = work;
      entry.state = { phase: "waiting" };
      entry.queued = entry.listeners.size > 0;
    }
    if (!entry) {
      entry = {
        version,
        generation: 0,
        expiresAt: 0,
        work,
        state: { phase: "waiting" },
        listeners: new Set(),
        queued: false,
      };
      this.entries.set(key, entry);
    }
    return entry;
  }
  state(work: SourceWork): ContentCheckState {
    return this.entry(work).state;
  }
  verified(work: SourceWork): boolean {
    const entry = this.entries.get(sourceWorkKey(work));
    return (
      !!entry &&
      entry.version === (work.sourceUpdatedAt ?? null) &&
      entry.state.phase === "ready" &&
      entry.expiresAt > this.now() &&
      !isContentHidden(entry.state.work)
    );
  }
  seed(
    works: SourceWork[],
    verifiedIds: string[],
    expiresAt = this.now() + 24 * 60 * 60 * 1000,
  ) {
    const verified = new Set(verifiedIds);
    for (const work of works) {
      rememberContentWork(work);
      if (!verified.has(work.workId) || expiresAt <= this.now()) continue;
      const entry = this.entry(work);
      if (entry.state.phase === "checking") continue;
      entry.work = compactWork(work);
      entry.state = { phase: "ready", work: entry.work };
      entry.expiresAt = Math.min(expiresAt, this.now() + 24 * 60 * 60 * 1000);
      entry.queued = false;
      this.notify(entry);
    }
    this.prune();
  }
  watch(
    work: SourceWork,
    listener: (state: ContentCheckState) => void,
  ): () => void {
    const entry = this.entry(work);
    entry.listeners.add(listener);
    listener(entry.state);
    if (entry.state.phase === "waiting" && !isContentHidden(work))
      entry.queued = true;
    this.pump();
    return () => {
      entry.listeners.delete(listener);
      if (!entry.listeners.size) entry.queued = false;
    };
  }
  retry(work: SourceWork) {
    const entry = this.entry(work);
    if (entry.state.phase !== "error") return;
    entry.state = { phase: "waiting" };
    entry.queued = entry.listeners.size > 0;
    this.notify(entry);
    this.pump();
  }
  private notify(entry: Entry) {
    if (!this.disposed)
      for (const listener of entry.listeners) listener(entry.state);
  }
  private pump() {
    if (this.disposed) return;
    for (const entry of this.entries.values()) {
      if (this.running >= 2) break;
      if (
        !entry.queued ||
        !entry.listeners.size ||
        entry.state.phase !== "waiting"
      )
        continue;
      entry.queued = false;
      entry.state = { phase: "checking" };
      this.running++;
      this.notify(entry);
      void this.read(entry, entry.generation, entry.work);
    }
  }
  private async read(entry: Entry, generation: number, requested: SourceWork) {
    try {
      const result = await this.adapter.query(this.scope, {
        kind: "detail",
        query: requested.workId,
        folderId: null,
        page: 1,
      });
      const work = result.items[0];
      if (
        result.source !== this.scope.source ||
        result.sessionId !== this.scope.sessionId ||
        result.items.length !== 1 ||
        result.issues?.length ||
        !work ||
        sourceWorkKey(work) !== sourceWorkKey(requested)
      )
        throw new SourceError("INVALID_RESPONSE");
      if (this.disposed || entry.generation !== generation) return;
      entry.work = compactWork(work);
      // Source detail dates can differ from list dates; the cache belongs to the
      // list version that requested it and is invalidated when that list changes.
      entry.state = { phase: "ready", work: entry.work };
      entry.expiresAt = this.now() + 24 * 60 * 60 * 1000;
      rememberContentWork(entry.work);
      this.onResolved(entry.work);
    } catch (error) {
      if (!this.disposed && entry.generation === generation)
        entry.state = { phase: "error", error };
    } finally {
      this.running--;
      if (entry.generation === generation) this.notify(entry);
      this.prune();
      this.pump();
    }
  }
  private prune() {
    if (this.entries.size <= 4096) return;
    for (const [key, entry] of this.entries) {
      if (!entry.listeners.size && entry.state.phase !== "checking")
        this.entries.delete(key);
      if (this.entries.size <= 4096) break;
    }
  }
  dispose() {
    this.disposed = true;
    for (const entry of this.entries.values()) entry.listeners.clear();
    this.entries.clear();
  }
}
