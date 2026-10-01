import type {
  SourceAdapter,
  SourcePage,
  SourceScope,
  SourceWork,
  RecentHistoryResult,
} from "./source-types.ts";
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
  displayItems: SourceWork[];
  phase: "idle" | "reading" | "ready" | "complete" | "limited" | "error";
  error: unknown;
  retainedItems?: SourceWork[];
  retainedCoverage?: RecentHistoryResult["coverage"];
  historyError?: boolean;
  historyErrorCode?: string | null;
  observationErrorCode?: string | null;
  uncommittedIds?: string[];
}

/** Metadata may change without promoting a saved work when its live page arrives. */
function mergeDisplayItems(
  order: SourceWork[],
  live: SourceWork[],
  retained: SourceWork[],
): SourceWork[] {
  // Keep the same metadata precedence as the source lists: lightweight live
  // rows inherit explicit labels and dates already present in saved records.
  const remaining = new Map(
    mergeSourceWorks(retained, live).map((work) => [sourceWorkKey(work), work]),
  );
  const items: SourceWork[] = [];
  for (const group of [order, live, retained]) {
    for (const work of group) {
      const key = sourceWorkKey(work);
      const current = remaining.get(key);
      if (!current) continue;
      items.push(current);
      remaining.delete(key);
    }
  }
  return items;
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

/** Live pagination and persisted supplementary records are deliberately separate. */
export class RecentUpdatesReader {
  state: RecentUpdatesState = {
    snapshot: null,
    displayItems: [],
    phase: "idle",
    error: null,
  };
  private disposed = false;
  private task: Promise<void> | null = null;
  private retryPage = 1;
  private historyTask: Promise<void> | null = null;
  private refreshOrder: SourceWork[] | null = null;
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
  private publish(change: Partial<RecentUpdatesState>, order?: SourceWork[]) {
    if (this.disposed) return;
    const next = { ...this.state, ...change };
    if (change.snapshot !== undefined || change.retainedItems !== undefined) {
      next.displayItems = mergeDisplayItems(
        order ?? this.state.displayItems,
        next.snapshot?.items ?? [],
        next.retainedItems ?? [],
      );
    }
    this.state = next;
    for (const listener of this.listeners) listener(this.state);
  }
  async start(): Promise<void> {
    if (this.state.phase !== "idle") return;
    // Establish the current source head once even if history wins the race;
    // otherwise brand-new works would be buried behind the entire saved list.
    this.refreshOrder = [];
    // Saved history is supplementary. A large local catalog must not delay the
    // first live page; either result can become visible independently.
    await Promise.all([this.read(1), this.refreshHistory()]);
  }
  refreshHistory(): Promise<void> {
    if (!this.adapter.recentHistory || this.disposed) return Promise.resolve();
    if (this.historyTask) return this.historyTask;
    this.historyTask = this.adapter
      .recentHistory(this.scope)
      .then((history) => {
        if (
          history.source !== this.scope.source ||
          history.sessionId !== this.scope.sessionId
        )
          throw new SourceError("STALE_SESSION");
        this.publish({
          retainedItems: history.items,
          retainedCoverage: history.coverage,
          historyError: false,
          historyErrorCode: null,
        });
      })
      .catch((error: unknown) => {
        this.publish({
          historyError: true,
          historyErrorCode:
            error instanceof SourceError && /^[A-Z0-9_]{1,80}$/.test(error.code)
              ? error.code
              : "SOURCE_UNAVAILABLE",
        });
      })
      .finally(() => {
        this.historyTask = null;
      });
    return this.historyTask;
  }
  async refresh(): Promise<void> {
    if (this.disposed) return;
    if (this.task) return this.task;
    // After initial loading, only a successful user refresh may rebuild order.
    // Retain the last known live order for saved records beyond that first page;
    // a late supplementary history response must not reorder them a second time.
    this.refreshOrder = this.state.snapshot?.items ?? [];
    await Promise.all([this.refreshHistory(), this.read(1)]);
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
      const uncommitted = new Set(this.state.uncommittedIds ?? []);
      for (const work of page.items) {
        if (page.observationErrorCode) uncommitted.add(sourceWorkKey(work));
        else if (page.discoveryRevision != null)
          uncommitted.delete(sourceWorkKey(work));
      }
      const order =
        number === 1 && this.refreshOrder !== null
          ? [...snapshot.items, ...this.refreshOrder]
          : undefined;
      if (number === 1) this.refreshOrder = null;
      this.publish(
        {
          snapshot,
          observationErrorCode: page.observationErrorCode ?? null,
          uncommittedIds: [...uncommitted],
          phase: snapshot.limited
            ? "limited"
            : snapshot.hasMore === false
              ? "complete"
              : "ready",
        },
        order,
      );
    } catch (error) {
      this.publish({ phase: "error", error });
    }
  }
}
