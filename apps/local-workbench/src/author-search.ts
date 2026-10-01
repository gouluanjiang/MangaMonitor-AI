import type {
  CompletionAdapter,
  DiscoverySnapshot,
} from "./completion-types.ts";
import type {
  AuthorQueryPolicy,
  SourceAdapter,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import { mergeSourceWorks, sourceWorkKey } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";
import { readCompleteAuthorSearch } from "./source-search.ts";
import { workHasAuthor } from "./author-evidence.ts";
import { authorQueryError } from "./author-query.ts";
import {
  authorSearchScheduler,
  AuthorSearchScheduler,
} from "./author-search-scheduler.ts";
import {
  newAuthorSearchMetrics,
  type AuthorSearchMetrics,
} from "./author-search-metrics.ts";

export interface AuthorSearchAdapter extends CompletionAdapter {
  setActive(active: boolean): void;
  dispose(): void;
  rendered(
    runId: string,
    revision: number,
    elapsedMs: number,
    hasVisibleWorks: boolean,
  ): AuthorSearchMetrics | null;
}

/** Each tab owns its generation and retained results. Closing cannot resurrect it.
 * Stored metadata supplements display, never proof that current paging finished. */
export function createAuthorSearchAdapter(
  adapter: SourceAdapter,
  scheduler: AuthorSearchScheduler = authorSearchScheduler,
): AuthorSearchAdapter {
  let generation = 0,
    scopeKey = "",
    authorKey = "",
    active = true,
    disposed = false;
  let snapshot: DiscoverySnapshot;
  let started = 0;
  const listeners = new Set<() => void>();
  let scheduled = false;
  function publish() {
    snapshot.revision++;
    if (
      snapshot.records.length &&
      snapshot.searchMetrics?.firstRecordsMs === null
    )
      snapshot.searchMetrics.firstRecordsMs = performance.now() - started;
    if (scheduled || disposed) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      if (!disposed) listeners.forEach((listener) => listener());
    });
  }
  function read(scopes: SourceScope[]) {
    const key = JSON.stringify(
      [...scopes].sort((a, b) => a.source.localeCompare(b.source)),
    );
    if (key !== scopeKey) {
      generation++;
      scopeKey = key;
      authorKey = "";
      snapshot = { scopes, revision: 0, run: null, authors: [], records: [] };
      scheduler.wake();
    }
    return structuredClone(snapshot);
  }
  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    setActive(value) {
      active = value;
      scheduler.wake();
    },
    dispose() {
      disposed = true;
      generation++;
      listeners.clear();
      scheduler.wake();
    },
    rendered(runId, revision, elapsedMs, hasVisibleWorks) {
      if (
        disposed ||
        !snapshot?.searchMetrics ||
        snapshot.run?.id !== runId ||
        revision > snapshot.revision
      )
        return null;
      const metrics = snapshot.searchMetrics;
      metrics.renderMs += elapsedMs;
      metrics.renderedBatches++;
      if (hasVisibleWorks && metrics.firstVisibleMs === null)
        metrics.firstVisibleMs = performance.now() - started;
      return { ...metrics };
    },
    read: async (scopes) => read(scopes),
    progress: async (scopes) => {
      read(scopes);
      const { records, ...progress } = snapshot;
      return { ...structuredClone(progress), recordCount: records.length };
    },
    startUnfinished: async () => {
      throw new SourceError("DISCOVERY_NO_UNFINISHED");
    },
    start: async (scopes, authors) => {
      if (
        disposed ||
        scopes.length !== 2 ||
        new Set(scopes.map((s) => s.source)).size !== 2 ||
        authors.length !== 1 ||
        !authors[0].trim()
      )
        throw new SourceError("DISCOVERY_INVALID");
      const author = authors[0].trim();
      const queryError = authorQueryError(author);
      if (queryError) throw new SourceError(queryError);
      read(scopes);
      if (authorKey === author && snapshot.run?.phase === "checking")
        return read(scopes);
      const retained = authorKey === author ? snapshot.records : [];
      authorKey = author;
      const request = ++generation,
        now = Date.now();
      started = performance.now();
      const runId = "author-search-" + request;
      const current = () => !disposed && generation === request;
      const supplements = new Map<string, number>();
      const historyErrors = new Map<string, boolean>();
      const metrics = newAuthorSearchMetrics();
      snapshot = {
        scopes,
        revision: snapshot.revision + 1,
        records: retained,
        authorPolicies: [],
        searchMetrics: metrics,
        authors: scopes.map((s) => ({
          source: s.source,
          author,
          state: "checking",
          pagesRead: 0,
          observedCount: 0,
          lastAttemptAt: now,
          lastCompleteAt: null,
          lastCheckedAt: null,
          lastCheckMode: "full",
          errorCode: null,
          issueCount: 0,
          issueSamples: [],
          pagesComplete: false,
        })),
        run: {
          id: runId,
          phase: "checking",
          currentAuthor: author,
          currentSource: null,
          currentPage: 0,
          requestsUsed: 0,
          completedScopes: 0,
          totalScopes: 2,
          errorCode: null,
          mode: "full",
          currentStrategy: "full",
        },
      };
      publish();
      function merge(works: SourceWork[], policy?: AuthorQueryPolicy) {
        const before = new Map(
          snapshot.records.map((r) => [sourceWorkKey(r.work), r]),
        );
        snapshot.records = mergeSourceWorks(
          snapshot.records.map((r) => r.work),
          works,
        ).map((work) => ({
          ...(before.get(sourceWorkKey(work)) ?? {
            matchedAuthors: [author],
            observedAt: now,
            scanId: runId,
          }),
          work,
          authorVerified: workHasAuthor(
            work,
            author,
            policy?.source === work.source
              ? policy
              : snapshot.authorPolicies?.find((p) => p.source === work.source),
          ),
        }));
      }
      const timed: SourceAdapter = {
        ...adapter,
        authorPolicy: async (...args) => {
          const time = performance.now();
          try {
            return await adapter.authorPolicy(...args);
          } finally {
            metrics.policyMs += performance.now() - time;
          }
        },
        ...(adapter.knownAuthorWorks
          ? {
              knownAuthorWorks: async (
                ...args: Parameters<
                  NonNullable<SourceAdapter["knownAuthorWorks"]>
                >
              ) => {
                const time = performance.now();
                try {
                  return await adapter.knownAuthorWorks!(...args);
                } finally {
                  metrics.localCatalogMs += performance.now() - time;
                }
              },
            }
          : {}),
        query: (scope, query) =>
          scheduler.run(
            scope.source,
            () => active,
            current,
            async (queuedMs) => {
              metrics.queueMs += queuedMs;
              const time = performance.now();
              try {
                const value = await adapter.query(scope, query);
                const total = performance.now() - time;
                if (value.timing) {
                  metrics.nativeQueueMs += value.timing.queueMs;
                  metrics.sourceOperationMs += value.timing.sourceOperationMs;
                  metrics.localCommitMs += value.timing.localCommitMs;
                  metrics.transportOtherMs += Math.max(
                    0,
                    total -
                      value.timing.queueMs -
                      value.timing.sourceOperationMs -
                      value.timing.localCommitMs,
                  );
                } else {
                  metrics.requestsWithoutNativeTiming++;
                  metrics.transportOtherMs += total;
                }
                return value;
              } catch (cause) {
                metrics.failedRequests++;
                metrics.transportOtherMs += performance.now() - time;
                throw cause;
              }
            },
          ),
      };
      void Promise.all(
        scopes.map(async (scope) => {
          const range = snapshot.authors.find(
            (row) => row.source === scope.source,
          )!;
          let policy: AuthorQueryPolicy | undefined;
          try {
            await readCompleteAuthorSearch(timed, scope, author, {
              current,
              onPolicy(value) {
                policy = value;
                snapshot.authorPolicies!.push(value);
                range.queryFingerprint = value.queryFingerprint;
              },
              onHistory(history) {
                merge(history.items, policy);
                supplements.set(scope.source, history.items.length);
                snapshot.historicalSupplementCount = [
                  ...supplements.values(),
                ].reduce((a, b) => a + b, 0);
                snapshot.historicalSupplementAt =
                  Math.max(
                    snapshot.historicalSupplementAt ?? 0,
                    history.checkedAt ?? 0,
                  ) || null;
                historyErrors.set(scope.source, !history.complete);
                snapshot.historicalReadError = [...historyErrors.values()].some(
                  Boolean,
                );
                snapshot.observationErrorCode ||= history.observationErrorCode;
                publish();
              },
              onPage(progress) {
                merge(
                  [...progress.items, ...(progress.historicalItems ?? [])],
                  policy,
                );
                supplements.set(
                  scope.source,
                  progress.historicalItems?.length ?? 0,
                );
                snapshot.historicalSupplementCount = [
                  ...supplements.values(),
                ].reduce((a, b) => a + b, 0);
                snapshot.historicalSupplementAt =
                  Math.max(
                    snapshot.historicalSupplementAt ?? 0,
                    progress.historicalReadAt ?? 0,
                  ) || null;
                historyErrors.set(scope.source, !!progress.historicalReadError);
                snapshot.historicalReadError = [...historyErrors.values()].some(
                  Boolean,
                );
                snapshot.observationErrorCode ||=
                  progress.page.observationErrorCode ??
                  progress.historicalObservationErrorCode;
                range.pagesRead = progress.page.page;
                range.observedCount = progress.items.length;
                range.issueCount = progress.issues.length;
                range.issueSamples = progress.issues.slice(0, 20);
                range.pagesComplete = progress.complete;
                range.state = progress.complete
                  ? range.issueCount
                    ? "partial"
                    : "complete"
                  : "checking";
                range.errorCode =
                  progress.complete && range.issueCount
                    ? "SOURCE_ITEMS_PARTIAL"
                    : null;
                if (progress.complete && !range.issueCount) {
                  range.lastCompleteAt = Date.now();
                  range.lastCheckedAt = range.lastCompleteAt;
                }
                snapshot.run!.currentSource = scope.source;
                snapshot.run!.currentPage =
                  progress.queryPage ?? progress.page.page;
                snapshot.run!.currentQueryIndex = progress.queryIndex ?? null;
                snapshot.run!.currentQueryCount = progress.queryCount ?? null;
                if (!progress.metadataOnly) snapshot.run!.requestsUsed++;
                publish();
              },
            });
          } catch (cause) {
            if (!current()) return;
            range.state = "error";
            range.errorCode =
              cause instanceof SourceError ? cause.code : "SEARCH_INCOMPLETE";
          }
          if (!current()) return;
          if (range.state === "complete") snapshot.run!.completedScopes++;
          publish();
        }),
      ).then(() => {
        if (!current()) return;
        snapshot.run!.phase = snapshot.authors.every(
          (row) => row.state === "complete",
        )
          ? "complete"
          : "partial";
        metrics.completedMs = performance.now() - started;
        publish();
      });
      return read(scopes);
    },
    cancel: async (runId) => {
      if (snapshot?.run?.id !== runId) return;
      generation++;
      snapshot.run.phase = "cancelled";
      for (const range of snapshot.authors)
        if (range.state === "checking") range.state = "cancelled";
      if (snapshot.searchMetrics)
        snapshot.searchMetrics.completedMs = performance.now() - started;
      scheduler.wake();
      publish();
    },
  };
}
