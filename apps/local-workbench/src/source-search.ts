import type {
  SourceAdapter,
  SourceScope,
  SourceQueryResult,
  SourceWork,
  SourceItemIssue,
  AuthorQueryPolicy,
  JmSearchBoundary,
} from "./source-types.ts";
import { mergeSourceWorks, sourceWorkKey } from "./source-types.ts";
import { partitionAuthorWorks } from "./author-evidence.ts";
import { sameSourceWork } from "./source-memory.ts";
import { SourceError, validateAuthorQueryPolicy } from "./source-runtime.ts";
import { authorQueryError } from "./author-query.ts";

export const jmSearchScopeNote =
  "JM 搜索范围：不含网页端的 English Manga（英文漫画）分类；“已读完”表示本次返回的全部分页已读取。";

export interface SearchPagination {
  total: number | null;
  pages: number | null;
  /** Raw rows still consume the traversal budget when one boundary row overlaps. */
  rawFetched?: number;
  previousPage?: {
    page: number;
    query: string;
    boundary: JmSearchBoundary;
    lastWork: SourceWork | null;
  };
}

export interface SearchProgress {
  items: SourceWork[];
  page: SourceQueryResult;
  /** First accepted page of this query, retained when a failed read resumes. */
  pagination: SearchPagination;
  recordsRead: number;
  complete: boolean;
  issues: SourceItemIssue[];
  query?: string;
  queryIndex?: number;
  queryCount?: number;
  queryPage?: number;
  /** Saved confirmations supplement display only, never raw pagination counts. */
  historicalItems?: SourceWork[];
  historicalReadAt?: number | null;
  historicalReadError?: boolean;
  historicalObservationErrorCode?: string | null;
  metadataOnly?: boolean;
}

/** Each approved query has an independent traversal; only work IDs are unioned. */
export async function readCompleteAuthorSearch(
  adapter: SourceAdapter,
  scope: SourceScope,
  author: string,
  options: {
    current(): boolean;
    onPolicy?(policy: AuthorQueryPolicy): void;
    onHistory?(history: {
      items: SourceWork[];
      checkedAt: number | null;
      complete: boolean;
      observationErrorCode?: string | null;
    }): void;
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
  let historical: SourceWork[] = [],
    historicalReadAt: number | null = null,
    historicalReadError = false,
    historicalObservationErrorCode: string | null = null;
  if (adapter.knownAuthorWorks) {
    try {
      const known = await adapter.knownAuthorWorks(scope, author);
      if (!options.current()) return;
      if (known.source !== scope.source || known.sessionId !== scope.sessionId)
        throw new SourceError("STALE_SESSION");
      historical = partitionAuthorWorks(known.items, author, policy).confirmed;
      historicalReadAt = known.checkedAt;
      historicalObservationErrorCode = known.observationErrorCode ?? null;
      historicalReadError =
        known.historyComplete === false || !!historicalObservationErrorCode;
    } catch {
      if (!options.current()) return;
      historicalReadError = true;
    }
    options.onHistory?.({
      items: historical,
      checkedAt: historicalReadAt,
      complete: !historicalReadError,
      observationErrorCode: historicalObservationErrorCode,
    });
  }
  for (const query of policy.queries) {
    const error = authorQueryError(query);
    if (error) throw new SourceError(error);
  }
  let items: SourceWork[] = [],
    issues: SourceItemIssue[] = [],
    recordsRead = 0,
    pagesRead = 0;
  let lastPublished: SearchProgress | undefined;
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
      requestKind: "author",
      current: options.current,
      onPage(progress) {
        last = progress;
        const complete =
          progress.complete && index === policy.queries.length - 1;
        const currentItems = mergeQueries(items, progress.items);
        const currentKeys = new Set(currentItems.map(sourceWorkKey));
        lastPublished = {
          items: currentItems,
          historicalItems: historical.filter(
            (work) => !currentKeys.has(sourceWorkKey(work)),
          ),
          historicalReadAt,
          historicalReadError,
          historicalObservationErrorCode,
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
          pagination: progress.pagination,
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
        };
        options.onPage(lastPublished);
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
  // Observations have now committed. Read the narrow local projection again so
  // a lightweight list credit cannot replace a stronger saved detail signature.
  if (options.current() && lastPublished && adapter.knownAuthorWorks) {
    try {
      const known = await adapter.knownAuthorWorks(scope, author);
      if (!options.current()) return;
      if (known.source !== scope.source || known.sessionId !== scope.sessionId)
        throw new SourceError("STALE_SESSION");
      const returned = partitionAuthorWorks(
        known.items,
        author,
        policy,
      ).confirmed;
      const incomplete =
        known.historyComplete === false || !!known.observationErrorCode;
      const confirmed = incomplete
        ? mergeSourceWorks(historical, returned)
        : returned;
      const knownById = new Map(
        confirmed.map((work) => [sourceWorkKey(work), work]),
      );
      const liveKeys = new Set(lastPublished.items.map(sourceWorkKey));
      const updated: SearchProgress = {
        ...lastPublished,
        items: lastPublished.items.map(
          (work) => knownById.get(sourceWorkKey(work)) ?? work,
        ),
        historicalItems: confirmed.filter(
          (work) => !liveKeys.has(sourceWorkKey(work)),
        ),
        historicalReadAt: known.checkedAt,
        historicalReadError: incomplete,
        historicalObservationErrorCode: known.observationErrorCode ?? null,
        metadataOnly: true,
      };
      const sameWorks = (a: SourceWork[], b: SourceWork[]) =>
        a.length === b.length &&
        a.every((work, index) => sameSourceWork(work, b[index]));
      if (
        !sameWorks(updated.items, lastPublished.items) ||
        !sameWorks(
          updated.historicalItems ?? [],
          lastPublished.historicalItems ?? [],
        ) ||
        updated.historicalReadAt !== lastPublished.historicalReadAt ||
        updated.historicalReadError !== lastPublished.historicalReadError ||
        updated.historicalObservationErrorCode !==
          lastPublished.historicalObservationErrorCode
      )
        options.onPage(updated);
    } catch {
      if (options.current() && !lastPublished.historicalReadError)
        options.onPage({
          ...lastPublished,
          historicalReadError: true,
          metadataOnly: true,
        });
    }
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
    pagination?: SearchPagination;
    requestKind?: "search" | "author" | "tag" | "category";
    pageLimit?: number;
  },
): Promise<void> {
  let items = options.items ?? [],
    issues = options.issues ?? [],
    recordsRead = options.recordsRead ?? 0,
    pagination = options.pagination;
  let rawFetched = pagination?.rawFetched ?? recordsRead;
  if (
    options.pageLimit !== undefined &&
    (!Number.isSafeInteger(options.pageLimit) || options.pageLimit < 1)
  )
    throw new SourceError("INVALID_INPUT");
  // A resumed page cannot establish a new baseline for already displayed rows.
  if ((options.fromPage ?? 1) > 1 && !pagination)
    throw new SourceError("SEARCH_INCOMPLETE");
  if (
    !Number.isSafeInteger(rawFetched) ||
    rawFetched < recordsRead ||
    rawFetched > 20000
  )
    throw new SourceError("SEARCH_INCOMPLETE");
  for (
    let page = options.fromPage ?? 1;
    page <= 1000 && options.current();
    page++
  ) {
    const result = await adapter.query(scope, {
      kind: options.requestKind ?? "search",
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
    // Match the saved catalog traversal: changed totals/page counts can move a
    // page boundary without repeating IDs. Retain only previously accepted pages.
    if (
      pagination &&
      (result.total !== pagination.total || result.pages !== pagination.pages)
    )
      throw new SourceError("SEARCH_INCOMPLETE");
    pagination ??= { total: result.total, pages: result.pages };
    const incomingIssues = result.issues ?? [];
    const rawCount = result.items.length + incomingIssues.length;
    if (rawFetched + rawCount > 20000)
      throw new SourceError("SEARCH_LIMIT_REACHED");
    const merged = mergeSourceWorks(items, result.items);
    const knownIds = new Set([
      ...items.map((work) => work.workId),
      ...issues.flatMap((issue) =>
        issue.workId === null ? [] : [issue.workId],
      ),
    ]);
    const previousPage = pagination.previousPage;
    const first = result.jmSearchBoundary?.first;
    const previousLast = previousPage?.boundary.last;
    const repeatedItems = result.items.filter((work) =>
      knownIds.has(work.workId),
    );
    const boundaryOverlap =
      scope.source === "JM" &&
      result.total !== null &&
      previousPage?.page === page - 1 &&
      previousPage.query === query &&
      first !== null &&
      first !== undefined &&
      previousLast !== null &&
      previousLast !== undefined &&
      first.workId === previousLast.workId &&
      first.fingerprint === previousLast.fingerprint &&
      result.items[0]?.workId === first.workId &&
      previousPage.lastWork !== null &&
      sameSourceWork(previousPage.lastWork, result.items[0]) &&
      repeatedItems.length === 1 &&
      repeatedItems[0] === result.items[0] &&
      result.items.some((work) => !knownIds.has(work.workId)) &&
      !incomingIssues.some((issue) => issue.index === 1);
    const issueCollision =
      incomingIssues.some(
        (issue) => issue.workId !== null && knownIds.has(issue.workId),
      ) ||
      (repeatedItems.length > 0 && !boundaryOverlap);
    const effectiveCount = rawCount - (boundaryOverlap ? 1 : 0);
    const terminal =
      result.hasMore === false ||
      (result.pages !== null && page === Math.max(1, result.pages)) ||
      (result.total !== null &&
        recordsRead + effectiveCount === result.total &&
        result.hasMore !== true &&
        result.pages === null);
    const contradictory =
      (result.hasMore === true &&
        result.pages !== null &&
        page >= Math.max(1, result.pages)) ||
      (result.hasMore === false &&
        result.pages !== null &&
        page < result.pages) ||
      (result.total !== null && recordsRead + effectiveCount > result.total);
    recordsRead += effectiveCount;
    rawFetched += rawCount;
    const stalled = rawCount === 0 && !terminal;
    const repeated =
      issueCollision ||
      new Set(result.items.map((work) => work.workId)).size !==
        result.items.length ||
      (page > 1 &&
        rawCount > 0 &&
        incomingIssues.length === 0 &&
        merged.length === items.length);
    const short =
      terminal && result.total !== null && recordsRead < result.total;
    const complete = terminal && !contradictory && !short && !repeated;
    items = merged;
    issues = [...issues, ...incomingIssues];
    if (
      result.jmSearchBoundary !== undefined ||
      pagination.rawFetched !== undefined
    ) {
      pagination = {
        total: pagination.total,
        pages: pagination.pages,
        rawFetched,
        ...(result.jmSearchBoundary === undefined
          ? {}
          : {
              previousPage: {
                page,
                query,
                boundary: result.jmSearchBoundary,
                lastWork:
                  result.jmSearchBoundary.last === null
                    ? null
                    : (result.items.at(-1) ?? null),
              },
            }),
      };
    }
    options.onPage({
      items,
      page: { ...result, items: [] },
      pagination: { ...pagination },
      recordsRead,
      complete,
      issues,
    });
    if (complete) return;
    if (contradictory || stalled || repeated || short)
      throw new SourceError("SEARCH_INCOMPLETE");
    if (page === 1000) throw new SourceError("SEARCH_LIMIT_REACHED");
    if (
      options.pageLimit !== undefined &&
      page - (options.fromPage ?? 1) + 1 >= options.pageLimit
    )
      return;
  }
}
