import type {
  SourceAdapter,
  SourceScope,
  SourceQueryResult,
  SourceWork,
  SourceItemIssue,
  AuthorQueryPolicy,
} from "./source-types.ts";
import { mergeSourceWorks } from "./source-types.ts";
import { SourceError, validateAuthorQueryPolicy } from "./source-runtime.ts";
import { authorQueryError } from "./author-query.ts";

export const jmSearchScopeNote =
  "JM 搜索范围：不含网页端的 English Manga（英文漫画）分类；“已读完”表示本次返回的全部分页已读取。";

export interface SearchProgress {
  items: SourceWork[];
  page: SourceQueryResult;
  recordsRead: number;
  complete: boolean;
  issues: SourceItemIssue[];
  query?: string;
  queryIndex?: number;
  queryCount?: number;
  queryPage?: number;
}

/** Each approved query has an independent traversal; only work IDs are unioned. */
export async function readCompleteAuthorSearch(
  adapter: SourceAdapter,
  scope: SourceScope,
  author: string,
  options: {
    current(): boolean;
    onPolicy?(policy: AuthorQueryPolicy): void;
    onPage(value: SearchProgress): void;
  },
): Promise<void> {
  if (!options.current()) return;
  const policy = await adapter.authorPolicy(scope, author);
  if (!options.current()) return;
  validateAuthorQueryPolicy(policy, scope, author);
  if (
    policy.source !== scope.source ||
    policy.sessionId !== scope.sessionId ||
    policy.author !== author
  )
    throw new SourceError("STALE_SESSION");
  options.onPolicy?.(policy);
  for (const query of policy.queries) {
    const error = authorQueryError(query);
    if (error) throw new SourceError(error);
  }
  let items: SourceWork[] = [],
    issues: SourceItemIssue[] = [],
    recordsRead = 0,
    pagesRead = 0;
  const mergeQueries = (previous: SourceWork[], incoming: SourceWork[]) => {
    const known = new Map(previous.map((work) => [work.workId, work]));
    return mergeSourceWorks(
      previous,
      incoming.map((work) => {
        const old = known.get(work.workId);
        return !work.authors.some((name) => name.trim()) &&
          old?.authors.some((name) => name.trim())
          ? { ...work, authors: old.authors }
          : work;
      }),
    );
  };
  for (
    let index = 0;
    index < policy.queries.length && options.current();
    index++
  ) {
    const query = policy.queries[index];
    let last: SearchProgress | undefined;
    await readCompleteSearch(adapter, scope, query, {
      current: options.current,
      onPage(progress) {
        last = progress;
        const complete =
          progress.complete && index === policy.queries.length - 1;
        options.onPage({
          items: mergeQueries(items, progress.items),
          page:
            policy.queries.length === 1
              ? progress.page
              : {
                  ...progress.page,
                  page: pagesRead + progress.page.page,
                  total: null,
                  pages: null,
                  hasMore: !complete,
                },
          recordsRead: recordsRead + progress.recordsRead,
          complete,
          issues: [
            ...issues,
            ...progress.issues.map((issue) =>
              policy.queries.length === 1 ? issue : { ...issue, query },
            ),
          ],
          query,
          queryIndex: index + 1,
          queryCount: policy.queries.length,
          queryPage: progress.page.page,
        });
      },
    });
    if (!options.current()) return;
    if (!last?.complete) throw new SourceError("SEARCH_INCOMPLETE");
    items = mergeQueries(items, last.items);
    issues = [
      ...issues,
      ...last.issues.map((issue) =>
        policy.queries.length === 1 ? issue : { ...issue, query },
      ),
    ];
    recordsRead += last.recordsRead;
    pagesRead += last.page.page;
  }
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
