import type {
  SourceAdapter,
  SourceScope,
  SourceQueryResult,
  SourceWork,
  SourceItemIssue,
} from "./source-types.ts";
import { mergeSourceWorks } from "./source-types.ts";
import { SourceError } from "./source-runtime.ts";

export const jmSearchScopeNote =
  "JM 搜索范围：不含网页端的 English Manga（英文漫画）分类；“已读完”表示本次返回的全部分页已读取。";

export interface SearchProgress {
  items: SourceWork[];
  page: SourceQueryResult;
  recordsRead: number;
  complete: boolean;
  issues: SourceItemIssue[];
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
    issues?: SourceItemIssue[];
  },
): Promise<void> {
  let items = options.items ?? [],
    issues = options.issues ?? [],
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
    const incomingIssues = result.issues ?? [];
    const rawCount = result.items.length + incomingIssues.length;
    if (recordsRead + rawCount > 20000)
      throw new SourceError("SEARCH_LIMIT_REACHED");
    const merged = mergeSourceWorks(items, result.items);
    const knownIds = new Set([
      ...items.map((work) => work.workId),
      ...issues.flatMap((issue) =>
        issue.workId === null ? [] : [issue.workId],
      ),
    ]);
    const issueCollision =
      incomingIssues.some(
        (issue) => issue.workId !== null && knownIds.has(issue.workId),
      ) || result.items.some((work) => knownIds.has(work.workId));
    const terminal =
      result.hasMore === false ||
      (result.pages !== null && page === Math.max(1, result.pages)) ||
      (result.total !== null &&
        recordsRead + rawCount === result.total &&
        result.hasMore !== true &&
        result.pages === null);
    const contradictory =
      (result.hasMore === true &&
        result.pages !== null &&
        page >= Math.max(1, result.pages)) ||
      (result.hasMore === false &&
        result.pages !== null &&
        page < result.pages) ||
      (result.total !== null && recordsRead + rawCount > result.total);
    recordsRead += rawCount;
    const stalled = rawCount === 0 && !terminal;
    const repeated =
      issueCollision ||
      (page > 1 &&
        rawCount > 0 &&
        incomingIssues.length === 0 &&
        merged.length === items.length);
    const short =
      terminal && result.total !== null && recordsRead < result.total;
    const complete = terminal && !contradictory && !short && !repeated;
    items = merged;
    issues = [...issues, ...incomingIssues];
    options.onPage({
      items,
      page: { ...result, items: [] },
      recordsRead,
      complete,
      issues,
    });
    if (complete) return;
    if (contradictory || stalled || repeated || short)
      throw new SourceError("SEARCH_INCOMPLETE");
    if (page === 1000) throw new SourceError("SEARCH_LIMIT_REACHED");
  }
}
