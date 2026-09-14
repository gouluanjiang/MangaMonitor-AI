import type {
  SourceAdapter,
  SourceScope,
  SourceQueryResult,
  SourceWork,
} from "./source-types.ts";
import { mergeSourceWorks } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";

export interface SearchProgress {
  items: SourceWork[];
  page: SourceQueryResult;
  recordsRead: number;
  complete: boolean;
}

/** A user-initiated catalog read. The caller owns cancellation and displayed scope. */
export async function readCompleteSearch(
  adapter: SourceAdapter,
  scope: SourceScope,
  query: string,
  options: {
    current(): boolean;
    onPage(value: SearchProgress): void;
    fromPage?: number;
    items?: SourceWork[];
    recordsRead?: number;
  },
): Promise<void> {
  let items = options.items ?? [],
    recordsRead = options.recordsRead ?? 0;
  for (
    let page = options.fromPage ?? 1;
    page <= 1000 && options.current();
    page++
  ) {
    const result = await adapter.query(scope, {
      kind: "search",
      query,
      folderId: null,
      page,
    });
    if (!options.current()) return;
    if (
      result.page !== page ||
      result.source !== scope.source ||
      result.sessionId !== scope.sessionId
    )
      throw new SourceError("STALE_SESSION");
    const merged = mergeSourceWorks(items, result.items);
    const terminal =
      result.hasMore === false ||
      (result.pages !== null && page === Math.max(1, result.pages)) ||
      (result.total !== null &&
        recordsRead + result.items.length === result.total &&
        result.hasMore !== true &&
        result.pages === null);
    const contradictory =
      (result.hasMore === true &&
        result.pages !== null &&
        page >= Math.max(1, result.pages)) ||
      (result.hasMore === false &&
        result.pages !== null &&
        page < result.pages) ||
      (result.total !== null &&
        recordsRead + result.items.length > result.total);
    recordsRead += result.items.length;
    const stalled = result.items.length === 0 && !terminal;
    const repeated =
      page > 1 && result.items.length > 0 && merged.length === items.length;
    const overLimit = recordsRead > 20000;
    const short =
      terminal && result.total !== null && recordsRead < result.total;
    const complete =
      terminal && !contradictory && !short && !overLimit && !repeated;
    items = merged.slice(0, 20000);
    options.onPage({
      items,
      page: { ...result, items: [] },
      recordsRead,
      complete,
    });
    if (complete) return;
    if (contradictory || stalled || repeated || short)
      throw new SourceError("SEARCH_INCOMPLETE");
    if (overLimit || page === 1000)
      throw new SourceError("SEARCH_LIMIT_REACHED");
  }
}
