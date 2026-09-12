import type {
  CatalogSnapshot,
  SourceAdapter,
  SourcePage,
  SourceScope,
} from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";
import {
  compactWork,
  catalogBytes,
  sameSourceWork,
  SOURCE_MEMORY_BYTES,
} from "./source-memory.ts";

export const MAX_COLLECTION_ITEMS = 20000;
export const MAX_COLLECTION_PAGES = 1000;
const changed = (): never => {
  throw new SourceError("CATALOG_CHANGED");
};
export function appendCatalog(
  previous: CatalogSnapshot | null,
  page: SourcePage,
  now = Date.now(),
  maxBytes = SOURCE_MEMORY_BYTES,
): CatalogSnapshot {
  if (
    page.page !== (previous?.page ?? 0) + 1 ||
    page.page > MAX_COLLECTION_PAGES ||
    page.items.length > 1000
  )
    changed();
  const previousKeys = new Set(
    previous?.items.map((work) => work.workId) ?? [],
  );
  const pageWorks = new Map<string, (typeof page.items)[number]>();
  for (const work of page.items) {
    if (previousKeys.has(work.workId)) changed();
    const samePage = pageWorks.get(work.workId);
    if (
      samePage &&
      (work.source !== "Pica" ||
        !sameSourceWork(samePage, work) ||
        (previous !== null && previous.pageEnds === undefined))
    )
      changed();
    pageWorks.set(work.workId, work);
  }
  if (
    !page.items.length &&
    !(page.page === 1 && page.total === 0 && page.hasMore !== true)
  )
    changed();
  if (
    previous?.total !== null &&
    previous?.total !== undefined &&
    page.total !== previous.total
  )
    changed();
  if (
    previous?.pages !== null &&
    previous?.pages !== undefined &&
    page.pages !== previous.pages
  )
    changed();
  const items = [...(previous?.items ?? []), ...page.items.map(compactWork)];
  if (items.length > MAX_COLLECTION_ITEMS)
    throw new SourceError("CATALOG_LIMIT");
  const terminalPage =
    page.pages !== null && page.page === Math.max(1, page.pages);
  if (
    page.total !== null &&
    (items.length > page.total ||
      ((page.hasMore === false || terminalPage) &&
        items.length !== page.total) ||
      (page.hasMore === true && items.length >= page.total))
  )
    changed();
  const complete =
    page.hasMore === false ||
    (terminalPage && page.hasMore !== true) ||
    (page.total !== null && items.length === page.total);
  if (
    page.pages !== null &&
    (page.page > Math.max(1, page.pages) ||
      (complete && page.pages > page.page) ||
      (page.page === page.pages && page.hasMore === true) ||
      (page.pages === 0 && items.length > 0))
  )
    changed();
  if (
    !complete &&
    (items.length === MAX_COLLECTION_ITEMS ||
      page.page === MAX_COLLECTION_PAGES)
  )
    throw new SourceError("CATALOG_LIMIT");
  const snapshot: CatalogSnapshot = {
    items,
    page: page.page,
    total: page.total,
    pages: page.pages,
    hasMore: complete ? false : page.hasMore,
    folders: page.folders.length ? page.folders : (previous?.folders ?? []),
    complete,
    updatedAt: now,
    firstPageIds:
      previous?.firstPageIds ?? page.items.map((work) => work.workId),
    ...(!previous
      ? { pageEnds: [items.length] }
      : previous.pageEnds
        ? { pageEnds: [...previous.pageEnds, items.length] }
        : {}),
  };
  if (catalogBytes(snapshot) > maxBytes) throw new SourceError("CATALOG_LIMIT");
  return snapshot;
}
export function firstPageMatches(snapshot: CatalogSnapshot, page: SourcePage) {
  return (
    page.page === 1 &&
    snapshot.total === page.total &&
    snapshot.pages === page.pages &&
    snapshot.firstPageIds.length === page.items.length &&
    snapshot.firstPageIds.every((id, index) => id === page.items[index].workId)
  );
}
export type CollectionPhase =
  | "idle"
  | "restoring"
  | "verifying"
  | "reading"
  | "ready"
  | "paused"
  | "complete"
  | "error";
export interface CollectionState {
  snapshot: CatalogSnapshot | null;
  displaySnapshot: CatalogSnapshot | null;
  completeSnapshot: CatalogSnapshot | null;
  phase: CollectionPhase;
  freshness: "none" | "cached" | "verified-cache" | "live";
  error: unknown;
  cacheWarning: string;
}
/** One sequential, cancellable metadata job. No cover or remote write operations. */
export class CollectionReader {
  state: CollectionState = {
    snapshot: null,
    displaySnapshot: null,
    completeSnapshot: null,
    phase: "idle",
    freshness: "none",
    error: null,
    cacheWarning: "",
  };
  private restored = false;
  private verified = false;
  private paused = false;
  private disposed = false;
  private task: Promise<void> | null = null;
  private revalidation: Promise<void> | null = null;
  private resumeAfterRevalidation = false;
  private dirty = false;
  private cacheWritable = true;
  private all = false;
  private budget = 0;
  private clock = 0;
  private listeners = new Set<(state: CollectionState) => void>();
  private adapter: SourceAdapter;
  readonly scope: SourceScope;
  readonly folderId: string | null;
  readonly reverse: boolean;
  private intervalMs: number;
  constructor(
    adapter: SourceAdapter,
    scope: SourceScope,
    folderId: string | null,
    reverse: boolean,
    intervalMs = 250,
  ) {
    this.adapter = adapter;
    this.scope = scope;
    this.folderId = folderId;
    this.reverse = reverse;
    this.intervalMs = intervalMs;
  }
  subscribe(listener: (state: CollectionState) => void) {
    this.listeners.add(listener);
    listener(this.state);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private publish(change: Partial<CollectionState>) {
    if (this.disposed) return;
    this.state = { ...this.state, ...change };
    for (const listener of this.listeners) listener(this.state);
  }
  pause() {
    this.paused = true;
    this.resumeAfterRevalidation = false;
    if (!["complete", "error"].includes(this.state.phase))
      this.publish({ phase: "paused" });
  }
  dispose() {
    this.paused = true;
    void this.checkpoint();
    this.disposed = true;
    this.listeners.clear();
  }
  private async checkpoint() {
    if (
      !this.dirty ||
      !this.state.snapshot ||
      this.disposed ||
      !this.cacheWritable
    )
      return;
    const snapshot = this.state.snapshot;
    let result;
    try {
      result = await this.adapter.catalog(this.scope, {
        action: "write",
        folderId: this.folderId,
        reverse: this.reverse,
        snapshot,
      });
    } catch {
      this.cacheWritable = false;
      this.publish({
        cacheWarning: "本机缓存未能保存，本次继续在内存中浏览，原缓存保留。",
      });
      return;
    }
    if (this.disposed) return;
    this.dirty = false;
    this.publish({
      completeSnapshot: result.completeSnapshot ?? this.state.completeSnapshot,
    });
  }
  private accept(snapshot: CatalogSnapshot) {
    snapshot = { ...snapshot, updatedAt: Math.max(Date.now(), this.clock + 1) };
    this.clock = snapshot.updatedAt;
    this.dirty = true;
    this.publish({
      snapshot,
      displaySnapshot: snapshot,
      freshness: "live",
      completeSnapshot: snapshot.complete
        ? snapshot
        : this.state.completeSnapshot,
    });
  }
  private page(number: number) {
    return this.adapter.query(this.scope, {
      kind: "favorites",
      query: "",
      folderId: this.folderId,
      page: number,
      reverse: this.reverse,
    });
  }
  async refresh() {
    this.pause();
    if (this.task) await this.task;
    if (this.disposed) return;
    this.restored = true;
    this.verified = false;
    this.dirty = false;
    this.budget = 0;
    this.publish({ snapshot: null, error: null });
    return this.resume();
  }
  loadNext(): Promise<void> {
    if (
      this.paused ||
      this.task ||
      this.state.phase === "error" ||
      this.state.snapshot?.complete
    )
      return this.task ?? Promise.resolve();
    this.budget = 1;
    return this.resume();
  }
  readAll(): Promise<void> {
    this.all = true;
    return this.resume();
  }
  retry(): Promise<void> {
    // A user retry must reattempt the failed continuation even if a later sort
    // change cleared full-reading mode. A failed head still retries only page 1.
    if (this.state.phase === "error" && this.verified && !this.all)
      this.budget = Math.max(1, this.budget);
    return this.resume();
  }
  stopReadAll() {
    this.all = false;
    this.budget = 0;
  }
  revalidate(): Promise<void> {
    this.pause();
    this.resumeAfterRevalidation = true;
    if (this.revalidation) return this.revalidation;
    const previous = this.task;
    const verification: Promise<void> = (async () => {
      // Let an outstanding page settle while paused before starting a new head check.
      await previous;
      this.revalidation = null;
      this.verified = false;
      if (!this.disposed && this.resumeAfterRevalidation) return this.resume();
    })().finally(() => {
      if (this.revalidation === verification) this.revalidation = null;
    });
    this.revalidation = verification;
    return verification;
  }
  resume(): Promise<void> {
    if (this.revalidation) {
      this.resumeAfterRevalidation = true;
      return this.revalidation;
    }
    this.paused = false;
    if (this.disposed) return Promise.resolve();
    if (this.task) return this.task;
    this.task = this.run().finally(() => {
      this.task = null;
    });
    return this.task;
  }
  private async run() {
    try {
      this.publish({ error: null });
      if (!this.restored) {
        this.publish({ phase: "restoring" });
        let cached;
        try {
          cached = await this.adapter.catalog(this.scope, {
            action: "read",
            folderId: this.folderId,
            reverse: this.reverse,
          });
        } catch {
          this.cacheWritable = false;
          this.publish({
            cacheWarning: "本机缓存暂不可用，本次直接读取来源，原缓存保留。",
          });
        }
        if (this.disposed) return;
        this.restored = true;
        if (cached) {
          this.clock = Math.max(
            cached.snapshot?.updatedAt ?? 0,
            cached.completeSnapshot?.updatedAt ?? 0,
          );
          this.publish({
            snapshot: cached.snapshot,
            displaySnapshot: cached.snapshot ?? cached.completeSnapshot,
            completeSnapshot: cached.completeSnapshot,
            freshness:
              cached.snapshot || cached.completeSnapshot ? "cached" : "none",
          });
        }
      }
      if (this.paused || this.disposed) return;
      if (!this.verified) {
        this.publish({ phase: "verifying" });
        const first = await this.page(1);
        if (this.disposed) return;
        const freshHead = appendCatalog(null, first);
        const cached = this.state.snapshot;
        if (
          cached &&
          firstPageMatches(cached, first) &&
          !(
            this.scope.source === "Pica" &&
            !cached.complete &&
            cached.pageEnds === undefined
          )
        )
          this.publish({ freshness: "verified-cache" });
        else this.accept(freshHead);
        this.verified = true;
        await this.checkpoint();
      }
      while (!this.disposed) {
        const snapshot = this.state.snapshot;
        if (!snapshot) throw new SourceError("CATALOG_CHANGED");
        if (snapshot.complete) {
          await this.checkpoint();
          this.publish({ phase: "complete" });
          return;
        }
        if (this.paused) {
          await this.checkpoint();
          this.publish({ phase: "paused" });
          return;
        }
        if (!this.all && this.budget === 0) {
          this.publish({ phase: "ready" });
          return;
        }
        this.publish({ phase: "reading" });
        await new Promise<void>((resolve) =>
          setTimeout(resolve, this.intervalMs),
        );
        if (this.paused || this.disposed) continue;
        if (!this.all && this.budget === 0) continue;
        const page = await this.page(snapshot.page + 1);
        if (this.disposed) return;
        const next = appendCatalog(snapshot, page);
        this.accept(next);
        this.budget = Math.max(0, this.budget - 1);
        if (!this.all || next.complete || next.page % 5 === 0 || this.paused)
          await this.checkpoint();
      }
    } catch (error) {
      if (this.disposed) return;
      try {
        await this.checkpoint();
      } catch {
        /* Keep the readable in-memory snapshot; next retry reattempts persistence. */
      }
      this.publish({ phase: "error", error });
    } finally {
      if (
        !this.disposed &&
        this.paused &&
        !["error", "complete"].includes(this.state.phase)
      ) {
        try {
          await this.checkpoint();
        } catch (error) {
          this.publish({ phase: "error", error });
          return;
        }
        this.publish({ phase: "paused" });
      }
    }
  }
}
