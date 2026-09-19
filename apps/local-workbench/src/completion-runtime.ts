import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { validateSourceWork, SourceError } from "./source-runtime.ts";
import type { Source, SourceScope } from "./source-types.ts";
import type {
  CompletionAdapter,
  DiscoveryBaseline,
  DiscoveryRun,
  DiscoverySnapshot,
} from "./completion-types.ts";
import { discoveryRecordLimit } from "./completion-types.ts";

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
  return {
    id: str(r.id, 128),
    phase: phase(r.phase),
    currentAuthor: nullable(r.currentAuthor, str),
    currentSource: nullable(r.currentSource, source),
    currentPage: integer(r.currentPage),
    requestsUsed: integer(r.requestsUsed),
    completedScopes: integer(r.completedScopes),
    totalScopes: integer(r.totalScopes),
    errorCode: code(r.errorCode),
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
  return {
    scopes,
    revision: integer(r.revision),
    run: nullable(r.run, runValue),
    records,
    authors: array(
      r.authors,
      (x) => {
        const q = object(x);
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
          observedCount: integer(q.observedCount),
          pagesRead: integer(q.pagesRead),
          errorCode: code(q.errorCode),
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
    read: async (scopes) =>
      validateDiscoverySnapshot(
        await call("discovery_read", { scopes: scopesValue(scopes) }),
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
    cancel: async (runId) => {
      await call("discovery_cancel", { runId: str(runId, 128) });
    },
  };
}
export function completionError(cause: unknown): string {
  const c = (cause as { code?: unknown })?.code;
  if (
    c === "LOGIN_REQUIRED" ||
    c === "STALE_SESSION" ||
    c === "DISCOVERY_SCOPE_CHANGED"
  )
    return "账号已变化，请连接 JM 和哔咔后重新检查。";
  if (c === "DISCOVERY_NO_AUTHORS") return "请先关注作者，再检查作者更新。";
  if (c === "DISCOVERY_LIMIT")
    return "作者目录达到保存上限，本次检查未完成，已读取的结果会保留。";
  return "本次检查未完成，已读取的结果会保留。请查看检查范围后重试。";
}
