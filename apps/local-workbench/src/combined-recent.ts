import type { Source, SourceWork } from "./source-types.ts";
import { mergeSourceWorks, sourceWorkKey } from "./source-types.ts";
import { sortByWorkDate } from "./work-dates.ts";
import type {
  RecentUpdatesReader,
  RecentUpdatesSnapshot,
  RecentUpdatesState,
} from "./recent-updates.ts";

export type RecentSourceChoice = Source | "both";
export interface RecentSourceState {
  source: Source;
  state: RecentUpdatesState | null;
}
export interface RecentViewState extends RecentUpdatesState {
  sources: RecentSourceState[];
  reading: boolean;
  canLoadNext: boolean;
}

function combinedSnapshot(
  sources: RecentSourceState[],
): RecentUpdatesSnapshot | null {
  const snapshots = sources.flatMap(({ state }) =>
    state?.snapshot ? [state.snapshot] : [],
  );
  if (!snapshots.length) return null;
  const all = snapshots.length === sources.length;
  const sum = (value: (snapshot: RecentUpdatesSnapshot) => number) =>
    snapshots.reduce((total, snapshot) => total + value(snapshot), 0);
  return {
    items: mergeSourceWorks(
      [],
      snapshots.flatMap((value) => value.items),
    ),
    issues: snapshots.flatMap((value) => value.issues ?? []),
    page: sum((value) => value.page),
    total:
      all && snapshots.every((value) => value.total !== null)
        ? sum((value) => value.total!)
        : null,
    pages:
      all && snapshots.every((value) => value.pages !== null)
        ? sum((value) => value.pages!)
        : null,
    hasMore: snapshots.some((value) => value.hasMore === true)
      ? true
      : all && snapshots.every((value) => value.hasMore === false)
        ? false
        : null,
    folders: [],
    updatedAt: Math.max(...snapshots.map((value) => value.updatedAt)),
    duplicates: sum((value) => value.duplicates),
    rawRecords: sum((value) => value.rawRecords),
    limited: snapshots.some((value) => value.limited),
  };
}

/** Keep displayed identities fixed while replacing their current metadata. */
export function mergeCombinedRecent(
  previous: SourceWork[],
  sources: readonly SourceWork[][],
  reorder: boolean,
): SourceWork[] {
  const latest = mergeSourceWorks([], sources.flat());
  if (reorder)
    return sortByWorkDate(
      latest,
      (work) => work.sourceUpdatedAt,
      "updated-desc",
    );
  const remaining = new Map(latest.map((work) => [sourceWorkKey(work), work]));
  const kept: SourceWork[] = [];
  for (const work of previous) {
    const current = remaining.get(sourceWorkKey(work));
    if (!current) continue;
    kept.push(current);
    remaining.delete(sourceWorkKey(work));
  }
  return [
    ...kept,
    ...sortByWorkDate(
      [...remaining.values()],
      (work) => work.sourceUpdatedAt,
      "updated-desc",
    ),
  ];
}

/** A view over the same per-account readers used by both single-source tabs. */
export class RecentUpdatesView {
  state: RecentViewState;
  private readers: Map<Source, RecentUpdatesReader>;
  private sources: Source[];
  private stops: (() => void)[] = [];
  private listeners = new Set<(state: RecentViewState) => void>();
  private headSeen = new Set<Source>();
  private reorderHeads = new Set<Source>();
  private snapshots = new Map<Source, RecentUpdatesSnapshot | null>();
  private disposed = false;

  constructor(choice: RecentSourceChoice, readers: RecentUpdatesReader[]) {
    this.sources = choice === "both" ? ["JM", "Pica"] : [choice];
    this.readers = new Map(
      readers
        .filter((reader) => this.sources.includes(reader.scope.source))
        .map((reader) => [reader.scope.source, reader]),
    );
    for (const [source, reader] of this.readers) {
      this.snapshots.set(source, reader.state.snapshot);
      if (reader.state.snapshot) this.headSeen.add(source);
    }
    this.state = this.project([], true);
    for (const [source, reader] of this.readers) {
      this.stops.push(
        reader.subscribe((state) => {
          if (this.disposed) return;
          const changed = this.snapshots.get(source) !== state.snapshot;
          this.snapshots.set(source, state.snapshot);
          const newHead =
            changed &&
            state.snapshot?.page === 1 &&
            (!this.headSeen.has(source) || this.reorderHeads.has(source));
          if (state.snapshot) this.headSeen.add(source);
          if (newHead) this.reorderHeads.delete(source);
          this.state = this.project(this.state.displayItems, Boolean(newHead));
          for (const listener of this.listeners) listener(this.state);
        }),
      );
    }
  }

  private project(previous: SourceWork[], reorder: boolean): RecentViewState {
    const sources = this.sources.map((source) => ({
      source,
      state: this.readers.get(source)?.state ?? null,
    }));
    const reading = sources.some(({ state }) => state?.phase === "reading");
    const canLoadNext = sources.some(({ state }) => state?.phase === "ready");
    if (sources.length === 1 && sources[0].state)
      return { ...sources[0].state, sources, reading, canLoadNext };
    const complete = sources.every(({ state }) => state?.phase === "complete");
    const error =
      sources.find(({ state }) => state?.phase === "error")?.state?.error ??
      null;
    return {
      sources,
      reading,
      canLoadNext,
      snapshot: combinedSnapshot(sources),
      displayItems: mergeCombinedRecent(
        previous,
        sources.map(({ state }) => state?.displayItems ?? []),
        reorder,
      ),
      phase: canLoadNext
        ? "ready"
        : reading
          ? "reading"
          : complete
            ? "complete"
            : error || sources.some(({ state }) => !state)
              ? "error"
              : sources.some(({ state }) => state?.phase === "limited")
                ? "limited"
                : "idle",
      error,
      retainedItems: sources.flatMap(({ state }) => state?.retainedItems ?? []),
      uncommittedIds: [
        ...new Set(sources.flatMap(({ state }) => state?.uncommittedIds ?? [])),
      ],
      historyError: sources.some(({ state }) => state?.historyError),
      historyErrorCode: sources.find(({ state }) => state?.historyErrorCode)
        ?.state?.historyErrorCode,
      observationErrorCode: sources.find(
        ({ state }) => state?.observationErrorCode,
      )?.state?.observationErrorCode,
    };
  }

  subscribe(listener: (state: RecentViewState) => void) {
    this.listeners.add(listener);
    listener(this.state);
    return () => {
      this.listeners.delete(listener);
    };
  }
  dispose() {
    this.disposed = true;
    this.stops.forEach((stop) => stop());
    this.listeners.clear();
  }
  private each(
    action: (reader: RecentUpdatesReader) => Promise<void>,
    source?: Source,
  ) {
    if (this.disposed) return Promise.resolve();
    return Promise.all(
      [...this.readers.values()]
        .filter(
          (reader) => source === undefined || reader.scope.source === source,
        )
        .map(action),
    ).then(() => {});
  }
  start() {
    return this.each((reader) => reader.start());
  }
  refresh() {
    if (this.state.reading) return Promise.resolve();
    for (const source of this.readers.keys()) this.reorderHeads.add(source);
    return this.each((reader) => reader.refresh());
  }
  refreshHistory(source?: Source) {
    return this.each((reader) => reader.refreshHistory(), source);
  }
  loadNext() {
    return this.each((reader) => reader.loadNext());
  }
  retry(source?: Source) {
    return this.each((reader) => reader.retry(), source);
  }
}
