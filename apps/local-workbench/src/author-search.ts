import type {
  CompletionAdapter,
  DiscoverySnapshot,
} from "./completion-types.ts";
import type { SourceAdapter, SourceScope } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";
import { readCompleteSearch } from "./source-search.ts";
import { authorQueryError } from "./author-query.ts";

/** Ad-hoc author searches never change following, stored update results, or the queue. */
export function createAuthorSearchAdapter(
  adapter: SourceAdapter,
): CompletionAdapter {
  let generation = 0,
    scopeKey = "";
  let snapshot: DiscoverySnapshot;
  const key = (scopes: SourceScope[]) =>
    JSON.stringify(
      [...scopes].sort((a, b) => a.source.localeCompare(b.source)),
    );
  function read(scopes: SourceScope[]) {
    ensureScope(scopes);
    return structuredClone(snapshot);
  }
  function ensureScope(scopes: SourceScope[]) {
    if (scopeKey !== key(scopes)) {
      generation++;
      scopeKey = key(scopes);
      snapshot = { scopes, revision: 0, run: null, authors: [], records: [] };
    }
  }
  return {
    read: async (scopes) => read(scopes),
    progress: async (scopes) => {
      ensureScope(scopes);
      const { records, ...progress } = snapshot;
      return { ...structuredClone(progress), recordCount: records.length };
    },
    startUnfinished: async () => {
      throw new SourceError("DISCOVERY_NO_UNFINISHED");
    },
    start: async (scopes, authors) => {
      if (
        scopes.length !== 2 ||
        new Set(scopes.map((s) => s.source)).size !== 2 ||
        authors.length !== 1 ||
        !authors[0].trim()
      )
        throw new SourceError("DISCOVERY_INVALID");
      const queryError = authorQueryError(authors[0]);
      if (queryError) throw new SourceError(queryError);
      read(scopes);
      const request = ++generation,
        author = authors[0].trim(),
        now = Date.now();
      const runId = "author-search-" + request;
      snapshot = {
        scopes,
        revision: snapshot.revision + 1,
        records: [],
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
      const started = structuredClone(snapshot);
      void (async () => {
        for (const scope of scopes) {
          if (generation !== request) return;
          const range = snapshot.authors.find(
            (row) => row.source === scope.source,
          )!;
          snapshot.run!.currentSource = scope.source;
          try {
            await readCompleteSearch(adapter, scope, author, {
              current: () => generation === request,
              onPage: (progress) => {
                const other = snapshot.records.filter(
                  (record) => record.work.source !== scope.source,
                );
                snapshot.records = [
                  ...other,
                  ...progress.items.map((work) => ({
                    work,
                    matchedAuthors: [author],
                    authorVerified: work.authors.some(
                      (name) =>
                        name.normalize("NFKC").toLocaleLowerCase() ===
                        author.normalize("NFKC").toLocaleLowerCase(),
                    ),
                    observedAt: now,
                    scanId: runId,
                  })),
                ];
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
                snapshot.run!.currentPage = progress.page.page;
                snapshot.run!.requestsUsed++;
                snapshot.revision++;
              },
            });
          } catch {
            if (generation !== request) return;
            range.state = "error";
            range.errorCode = "SEARCH_INCOMPLETE";
          }
          if (generation !== request) return;
          if (range.state === "complete") snapshot.run!.completedScopes++;
        }
        if (generation !== request) return;
        snapshot.run!.phase = snapshot.authors.every(
          (row) => row.state === "complete",
        )
          ? "complete"
          : "partial";
        snapshot.revision++;
      })();
      return started;
    },
    cancel: async (runId) => {
      if (snapshot?.run?.id !== runId) return;
      generation++;
      snapshot.run.phase = "cancelled";
      for (const range of snapshot.authors)
        if (range.state === "checking") range.state = "cancelled";
      snapshot.revision++;
    },
  };
}
