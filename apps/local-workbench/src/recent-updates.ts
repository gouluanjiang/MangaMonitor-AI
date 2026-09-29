import type { SourceAdapter, SourcePage, SourceScope } from "./source-types.ts";
import { mergeSourceWorks, sourceWorkKey } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";
import {
  compactWork,
  jsonBytes,
  SOURCE_MEMORY_BYTES,
} from "./source-memory.ts";

export const MAX_RECENT_ITEMS = 20000;
export const MAX_RECENT_PAGES = 1000;
export interface RecentUpdatesSnapshot extends SourcePage {
  updatedAt: number;
  duplicates: number;
  rawRecords: number;
  limited: boolean;
}
export interface RecentUpdatesState {
  snapshot: RecentUpdatesSnapshot | null;
  phase: "idle" | "reading" | "ready" | "complete" | "limited" | "error";
  error: unknown;
}

/** A live feed can move between requests: retain first-seen order and merge IDs. */
export function appendRecentUpdates(
  previous: RecentUpdatesSnapshot | null,
  page: SourcePage,
  now = Date.now(),
  maxBytes = SOURCE_MEMORY_BYTES,
): RecentUpdatesSnapshot {
  if (
    page.page !== (previous?.page ?? 0) + 1 ||
    page.page > MAX_RECENT_PAGES ||
    page.items.length + (page.issues?.length ?? 0) > 1000 ||
    (page.pages !== null && page.page > Math.max(1, page.pages))
  )
    throw new SourceError("INVALID_RESPONSE");
  const rawRecords =
    (previous?.rawRecords ?? 0) +
    page.items.length +
    (page.issues?.length ?? 0);
  const known = new Set(previous?.items.map(sourceWorkKey) ?? []);
  let duplicates = previous?.duplicates ?? 0;
  let added = 0;
  for (const work of page.items) {
    const key = sourceWorkKey(work);
    if (known.has(key)) duplicates++;
    else {
      known.add(key);
      added++;
    }
  }
  const items = mergeSourceWorks(
    previous?.items ?? [],
    page.items.map(compactWork),
  );
  const issues = [...(previous?.issues ?? []), ...(page.issues ?? [])];
  const records = items.length + issues.length;
  // Do not count overlapping IDs twice when deriving a missing page boundary.
  const hasMore =
    page.hasMore ??
    (page.pages === null ? null : page.page < Math.max(1, page.pages)) ??
    (page.total !== null ? records < page.total : null);
  if (
    (!page.items.length || (previous && !added)) &&
    !page.issues?.length &&
    hasMore !== false
  )
    throw new SourceError("RECENT_PAGE_STALLED");
  if (records > MAX_RECENT_ITEMS) throw new SourceError("RECENT_LIMIT");
  const snapshot: RecentUpdatesSnapshot = {
    items,
    ...(issues.length ? { issues } : {}),
    page: page.page,
    total: page.total,
    pages: page.pages,
    hasMore,
    folders: [],
    updatedAt: now,
    duplicates,
    rawRecords,
    limited:
      hasMore !== false &&
      (records === MAX_RECENT_ITEMS || page.page === MAX_RECENT_PAGES),
  };
  if (
    jsonBytes({ ...snapshot, items: [] }) +
      items.reduce((sum, work) => sum + jsonBytes(work), 0) >
    maxBytes
  )
    throw new SourceError("RECENT_LIMIT");
  return snapshot;
}

/** One explicit page at a time. No source writes, catalog persistence or loops. */
export class RecentUpdatesReader {
  state: RecentUpdatesState = { snapshot: null, phase: "idle", error: null };
  private disposed = false;
  private task: Promise<void> | null = null;
  private retryPage = 1;
  private listeners = new Set<(state: RecentUpdatesState) => void>();
  private adapter: SourceAdapter;
  readonly scope: SourceScope;
  constructor(adapter: SourceAdapter, scope: SourceScope) {
    this.adapter = adapter;
    this.scope = scope;
  }
  subscribe(listener: (state: RecentUpdatesState) => void) {
    this.listeners.add(listener);
    listener(this.state);
    return () => {
      this.listeners.delete(listener);
    };
  }
  dispose() {
    this.disposed = true;
    this.listeners.clear();
  }
  private publish(change: Partial<RecentUpdatesState>) {
    if (this.disposed) return;
    this.state = { ...this.state, ...change };
    for (const listener of this.listeners) listener(this.state);
  }
  start(): Promise<void> {
    return this.state.phase === "idle" ? this.read(1) : Promise.resolve();
  }
  refresh(): Promise<void> {
    return this.read(1);
  }
  retry(): Promise<void> {
    return this.state.phase === "error"
      ? this.read(this.retryPage)
      : Promise.resolve();
  }
  loadNext(): Promise<void> {
    if (this.state.phase !== "ready") return this.task ?? Promise.resolve();
    return this.read((this.state.snapshot?.page ?? 0) + 1);
  }
  private read(page: number): Promise<void> {
    if (this.disposed) return Promise.resolve();
    if (this.task) return this.task;
    this.retryPage = page;
    this.publish({ phase: "reading", error: null });
    this.task = this.run(page).finally(() => {
      this.task = null;
    });
    return this.task;
  }
  private async run(number: number) {
    try {
      const page = await this.adapter.query(this.scope, {
        kind: "recent",
        query: "",
        folderId: null,
        page: number,
      });
      if (this.disposed) return;
      if (
        page.source !== this.scope.source ||
        page.sessionId !== this.scope.sessionId
      )
        throw new SourceError("STALE_SESSION");
      const snapshot = appendRecentUpdates(
        number === 1 ? null : this.state.snapshot,
        page,
      );
      this.publish({
        snapshot,
        phase: snapshot.limited
          ? "limited"
          : snapshot.hasMore === false
            ? "complete"
            : "ready",
      });
    } catch (error) {
      this.publish({ phase: "error", error });
    }
  }
}
