import type {
  CompletionAdapter,
  DiscoverySnapshot,
} from "./completion-types.ts";
import type { SourceAdapter, SourceScope } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";
import { readCompleteSearch } from "./source-search.ts";

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
    if (scopeKey !== key(scopes)) {
      generation++;
      scopeKey = key(scopes);
      snapshot = { scopes, revision: 0, run: null, authors: [], records: [] };
    }
    return structuredClone(snapshot);
  }
  return {
    read: async (scopes) => read(scopes),
    start: async (scopes, authors) => {
      if (
        scopes.length !== 2 ||
        new Set(scopes.map((s) => s.source)).size !== 2 ||
        authors.length !== 1 ||
        !authors[0].trim()
      )
        throw new SourceError("DISCOVERY_INVALID");
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
                range.state = progress.complete ? "complete" : "checking";
                if (progress.complete) {
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
