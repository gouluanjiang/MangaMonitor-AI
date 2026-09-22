import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import {
  validateSourceWork,
  validateSourceIssues,
  validateAuthorQueryPolicy,
  SourceError,
} from "./source-runtime.ts";
import type { AuthorQueryPolicy, Source, SourceScope } from "./source-types.ts";
import type {
  CompletionAdapter,
  DiscoveryBaseline,
  DiscoveryProgress,
  DiscoveryRun,
  DiscoverySnapshot,
} from "./completion-types.ts";
import { discoveryRecordLimit } from "./completion-types.ts";
import { authorQueryMessage } from "./author-query.ts";

const invalid = (): never => {
  throw new SourceError("DISCOVERY_INVALID");
};
const object = (v: unknown): Record<string, unknown> =>
  v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : invalid();
const integer = (v: unknown): number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 0 ? v : invalid();
const str = (v: unknown, max = 1024): string =>
  typeof v === "string" && v.length <= max && !/[\x00-\x1f\x7f-\x9f]/.test(v)
    ? v
    : invalid();
const array = <T>(v: unknown, f: (x: unknown) => T, max = 20000): T[] =>
  Array.isArray(v) && v.length <= max ? v.map((x) => f(x)) : invalid();
const choice = <T extends string>(v: unknown, values: readonly T[]): T =>
  typeof v === "string" && values.includes(v as T) ? (v as T) : invalid();
const nullable = <T>(v: unknown, f: (x: unknown) => T): T | null =>
  v === null ? null : f(v);
const source = (v: unknown): Source => choice(v, ["JM", "Pica"]);
const phase = (v: unknown) =>
  choice(v, ["checking", "complete", "partial", "cancelled", "error"]);
const discoveryMode = (v: unknown) => choice(v, ["incremental", "full"]);
const code = (v: unknown) =>
  nullable(v, (x) =>
    /^[A-Z_0-9]{1,100}$/.test(str(x, 100)) ? (x as string) : invalid(),
  );
function baselineValue(v: unknown): DiscoveryBaseline {
  const b = object(v),
    headIds = array(b.headIds, (id) => str(id, 128), 20),
    queryVersion = integer(b.queryVersion),
    total = integer(b.total);
  if (
    !queryVersion ||
    new Set(headIds).size !== headIds.length ||
    headIds.length !== Math.min(total, 20)
  )
    return invalid();
  return {
    queryVersion,
    headIds,
    total,
    establishedAt: integer(b.establishedAt),
  };
}
function scopesValue(v: unknown): SourceScope[] {
  const scopes = array(
    v,
    (x) => {
      const r = object(x);
      return { source: source(r.source), sessionId: str(r.sessionId, 128) };
    },
    2,
  );
  return scopes.length === 2 &&
    new Set(scopes.map((s) => s.source)).size === 2 &&
    scopes.every((s) => s.sessionId)
    ? scopes
    : invalid();
}
function runValue(v: unknown): DiscoveryRun {
  const r = object(v);
  const queryIndex =
    r.currentQueryIndex == null ? null : integer(r.currentQueryIndex);
  const queryCount =
    r.currentQueryCount == null ? null : integer(r.currentQueryCount);
  if (
    (queryIndex === null) !== (queryCount === null) ||
    (queryIndex !== null &&
      (queryIndex < 1 || queryCount! > 4 || queryIndex > queryCount!))
  )
    return invalid();
  return {
    id: str(r.id, 128),
    phase: phase(r.phase),
    currentAuthor: nullable(r.currentAuthor, str),
    currentSource: nullable(r.currentSource, source),
    currentPage: integer(r.currentPage),
    currentQueryIndex: queryIndex,
    currentQueryCount: queryCount,
    requestsUsed: integer(r.requestsUsed),
    completedScopes: integer(r.completedScopes),
    totalScopes: integer(r.totalScopes),
    errorCode: code(r.errorCode),
    storageWarningCode:
      r.storageWarningCode === undefined ? null : code(r.storageWarningCode),
    mode: r.mode === undefined ? "full" : discoveryMode(r.mode),
    currentStrategy:
      r.currentStrategy === undefined
        ? null
        : nullable(r.currentStrategy, discoveryMode),
  };
}
export function validateDiscoverySnapshot(
  v: unknown,
  expected: SourceScope[],
): DiscoverySnapshot {
  const r = object(v);
  const records = array(
    r.records,
    (x) => {
      const q = object(x);
      if (typeof q.authorVerified !== "boolean") return invalid();
      return {
        work: validateSourceWork(q.work),
        matchedAuthors: array(q.matchedAuthors, str, 1000),
        authorVerified: q.authorVerified,
        observedAt: integer(q.observedAt),
        scanId: str(q.scanId, 128),
      };
    },
    discoveryRecordLimit,
  );
  if (
    new Set(records.map((q) => q.work.source + ":" + q.work.workId)).size !==
    records.length
  )
    return invalid();
  const { recordCount: _count, ...progress } = validateDiscoveryProgress(
    { ...r, recordCount: records.length },
    expected,
  );
  if (r.includesOther !== undefined && typeof r.includesOther !== "boolean")
    return invalid();
  return { ...progress, records, includesOther: r.includesOther ?? true };
}
export function validateDiscoveryProgress(
  v: unknown,
  expected: SourceScope[],
): DiscoveryProgress {
  const r = object(v),
    scopes = scopesValue(r.scopes);
  if (
    !scopes.every((s) =>
      expected.some(
        (e) => e.source === s.source && e.sessionId === s.sessionId,
      ),
    )
  )
    throw new SourceError("STALE_SESSION");
  const authorPolicies = array(
    r.authorPolicies ?? [],
    (value) => validateAuthorQueryPolicy(value),
    4000,
  );
  if (
    new Set(
      authorPolicies.map((policy) =>
        JSON.stringify([policy.source, policy.author]),
      ),
    ).size !== authorPolicies.length
  )
    return invalid();
  return {
    scopes,
    authorPolicies,
    revision: integer(r.revision),
    run: nullable(r.run, runValue),
    recordCount: integer(r.recordCount),
    otherRecordCount:
      r.otherRecordCount === undefined ? 0 : integer(r.otherRecordCount),
    authors: array(
      r.authors,
      (x) => {
        const q = object(x);
        const issueCount =
          q.issueCount === undefined ? 0 : integer(q.issueCount);
        const pagesRead = integer(q.pagesRead);
        if (pagesRead > 4000) return invalid();
        const queryFingerprint =
          q.queryFingerprint == null ? null : str(q.queryFingerprint, 64);
        if (
          queryFingerprint !== null &&
          !/^[a-f0-9]{64}$/.test(queryFingerprint)
        )
          return invalid();
        const queryBaselines = array(
          q.queryBaselines ?? [],
          (value) => {
            const entry = object(value),
              query = str(entry.query, 512);
            if (!query.trim() || query.trim() !== query) return invalid();
            return { query, baseline: baselineValue(entry.baseline) };
          },
          4,
        );
        if (
          new Set(queryBaselines.map((entry) => entry.query)).size !==
            queryBaselines.length ||
          (queryBaselines.length > 0 && queryFingerprint === null)
        )
          return invalid();
        const issueSamples = validateSourceIssues(
          q.issueSamples,
          pagesRead,
          20,
          source(q.source),
        );
        const completedQueries = array(
          q.completedQueries ?? [],
          (query) => str(query, 512),
          4,
        );
        if (
          new Set(completedQueries).size !== completedQueries.length ||
          completedQueries.some(
            (query) => !queryBaselines.some((entry) => entry.query === query),
          )
        )
          return invalid();
        if (
          issueCount > 100000 ||
          issueCount > pagesRead * 1000 ||
          issueSamples.length !== Math.min(issueCount, 20) ||
          (q.pagesComplete !== undefined &&
            typeof q.pagesComplete !== "boolean") ||
          (q.pagesComplete === true &&
            (pagesRead === 0 ||
              !["complete", "partial"].includes(q.state as string))) ||
          (issueCount > 0 &&
            (q.state === "complete" ||
              q.baseline != null ||
              (queryBaselines.length > 0 &&
                issueSamples.some(
                  (issue) =>
                    !issue.query ||
                    queryBaselines.some((entry) => entry.query === issue.query),
                ))))
        )
          return invalid();
        return {
          author: str(q.author),
          source: source(q.source),
          state: choice(q.state, [
            "idle",
            "checking",
            "complete",
            "partial",
            "cancelled",
            "error",
          ]),
          lastAttemptAt: nullable(q.lastAttemptAt, integer),
          lastCompleteAt: nullable(q.lastCompleteAt, integer),
          lastCheckedAt:
            q.lastCheckedAt === undefined
              ? nullable(q.lastCompleteAt, integer)
              : nullable(q.lastCheckedAt, integer),
          lastCheckMode:
            q.lastCheckMode === undefined
              ? null
              : nullable(q.lastCheckMode, discoveryMode),
          baseline:
            q.baseline === undefined
              ? null
              : nullable(q.baseline, baselineValue),
          queryFingerprint,
          queryBaselines,
          completedQueries,
          observedCount: integer(q.observedCount),
          pagesRead,
          errorCode: code(q.errorCode),
          issueCount,
          issueSamples,
          pagesComplete:
            q.pagesComplete === undefined
              ? q.state === "complete" && q.lastCheckMode !== "incremental"
              : q.pagesComplete,
        };
      },
      4000,
    ),
  };
}
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createCompletionAdapter(
  options: { native?: boolean; invoke?: Invoke } = {},
): CompletionAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  const call = async (
    command: string,
    args: Record<string, unknown>,
  ): Promise<unknown> => {
    if (!(options.native ?? isDesktopRuntime()))
      throw new SourceError("DESKTOP_REQUIRED");
    try {
      return await invoke(command, args);
    } catch (cause) {
      const value = (cause as { code?: unknown })?.code;
      throw new SourceError(
        typeof value === "string" && /^[A-Z_0-9]{1,100}$/.test(value)
          ? value
          : "DISCOVERY_UNAVAILABLE",
      );
    }
  };
  return {
    read: async (scopes, includeOther = false) =>
      validateDiscoverySnapshot(
        await call("discovery_read", {
          scopes: scopesValue(scopes),
          includeOther,
        }),
        scopes,
      ),
    progress: async (scopes) =>
      validateDiscoveryProgress(
        await call("discovery_progress", { scopes: scopesValue(scopes) }),
        scopes,
      ),
    start: async (scopes, authors, mode = "incremental") => {
      const result = object(
        await call("discovery_start", {
          scopes: scopesValue(scopes),
          authors: array(authors, str, 1000),
          mode: discoveryMode(mode),
        }),
      );
      const snapshot = validateDiscoverySnapshot(result.snapshot, scopes);
      if (str(result.runId, 128) !== snapshot.run?.id) return invalid();
      return snapshot;
    },
    startUnfinished: async (scopes, authors) => {
      const result = object(
        await call("discovery_start_unfinished", {
          scopes: scopesValue(scopes),
          authors: array(authors, str, 1000),
        }),
      );
      const snapshot = validateDiscoverySnapshot(result.snapshot, scopes);
      if (str(result.runId, 128) !== snapshot.run?.id) return invalid();
      return snapshot;
    },
    cancel: async (runId) => {
      await call("discovery_cancel", { runId: str(runId, 128) });
    },
  };
}
export function completionError(cause: unknown): string {
  const c = (cause as { code?: unknown })?.code;
  const queryMessage = authorQueryMessage(c);
  if (queryMessage) return queryMessage;
  if (
    c === "LOGIN_REQUIRED" ||
    c === "STALE_SESSION" ||
    c === "DISCOVERY_SCOPE_CHANGED"
  )
    return "账号已变化，请连接 JM 和哔咔后重新检查。";
  if (c === "DISCOVERY_NO_AUTHORS") return "请先关注作者，再检查作者更新。";
  if (c === "DISCOVERY_NO_UNFINISHED") return "所选作者没有未完成范围。";
  if (c === "DISCOVERY_LIMIT")
    return "作者目录达到保存上限，本次检查未完成，已读取的结果会保留。";
  return "本次检查未完成，已读取的结果会保留。请查看检查范围后重试。";
}

export interface CompletionReadFailure {
  code: string;
  failures: number;
  retryAfterMs: number | null;
}

/** Historical timestamps alone do not establish coverage of changed queries. */
export function authorCatalogAt(
  range: DiscoverySnapshot["authors"][number],
  policies: AuthorQueryPolicy[] = [],
): number | null {
  if (authorQueryMessage(range.errorCode)) return null;
  const policy = policies.find(
    (item) => item.source === range.source && item.author === range.author,
  );
  if (range.queryFingerprint) {
    if (!policy || policy.queryFingerprint !== range.queryFingerprint)
      return null;
    const baselines = policy.queries.map(
      (query) =>
        range.queryBaselines?.find((entry) => entry.query === query)?.baseline,
    );
    return baselines.every((baseline) => baseline !== undefined)
      ? Math.min(...baselines.map((baseline) => baseline!.establishedAt))
      : null;
  }
  if (
    policy &&
    (policy.queries.length !== 1 || policy.queries[0] !== range.author)
  )
    return null;
  return range.lastCompleteAt;
}

export function unfinishedRangeMessage(
  range: DiscoverySnapshot["authors"][number],
): string {
  const queryMessage = authorQueryMessage(range.errorCode);
  if (queryMessage) return queryMessage;
  switch (range.errorCode) {
    case "SOURCE_ITEMS_PARTIAL":
      return range.pagesComplete
        ? `分页已读完，${range.issueCount ?? 0} 条来源记录待核对`
        : "部分来源记录待核对，已读取结果保留";
    case "SOURCE_CONNECTION_FAILED":
      return "连接失败，已读取结果保留";
    case "SOURCE_TIMEOUT":
      return "来源响应超时，已读取结果保留";
    case "DISCOVERY_PAGINATION_CHANGED":
      return "分页结果发生变化，尚未确认完整范围";
    case "SOURCE_RESPONSE_INVALID":
      return "来源响应格式异常，已读取结果保留";
    case "SOURCE_REQUEST_FAILED":
      return "来源请求失败，已读取结果保留";
    case "DISCOVERY_LIMIT":
      return "达到目录保存上限，已读取结果保留";
    case "DISCOVERY_INTERRUPTED":
      return "上次检查中断，已读取结果保留";
    case "SOURCE_RATE_LIMITED":
      return "来源限制请求，请稍后补查";
    case "LOGIN_REQUIRED":
    case "STALE_SESSION":
      return "账号连接已变化，请连接后补查";
  }
  if (range.state === "idle") return "尚未开始检查";
  if (range.state === "checking") return "正在读取";
  if (range.state === "cancelled") return "已停止检查，已读取结果保留";
  return range.errorCode
    ? `来源读取未完成（${range.errorCode}）`
    : "检查未完成，已读取结果保留";
}

// These retries only read the local progress snapshot. They never restart a
// source check, and session, schema and other terminal errors require a user.
export function completionReadFailure(
  cause: unknown,
  failures: number,
): CompletionReadFailure {
  const rawCode = (cause as { code?: unknown })?.code;
  const code =
    typeof rawCode === "string" && /^[A-Z_0-9]{1,100}$/.test(rawCode)
      ? rawCode
      : "DISCOVERY_READ_FAILED";
  return {
    code,
    failures,
    retryAfterMs:
      code === "BUSY" ? ([1500, 3000, 6000][failures - 1] ?? null) : null,
  };
}
