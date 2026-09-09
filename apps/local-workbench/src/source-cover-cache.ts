import type { SourceAdapter, SourceScope } from "./source-types.ts";
import { queueCover } from "./source-cover-queue.ts";

const messages = {
  SOURCE_TIMEOUT: "封面请求超时，可重试。",
  SOURCE_CONNECTION_FAILED: "未能连接封面服务，可重试。",
  SOURCE_REQUEST_FAILED: "封面服务请求失败，可重试。",
  SOURCE_ACCESS_DENIED: "封面服务拒绝访问或要求额外验证。",
  SOURCE_RATE_LIMITED: "封面服务限制了请求，请稍后重试。",
  SOURCE_COVER_ACCESS_DENIED:
    "封面服务器拒绝访问（401/403），账号会话未因此失效。",
  SOURCE_COVER_RATE_LIMITED: "封面服务器限制了请求（429），请稍后重试。",
  SOURCE_COVER_SERVER_ERROR: "封面服务器暂时出错（5xx），可稍后重试。",
  SOURCE_COVER_NOT_FOUND: "可用封面地址均返回404；作品状态未因此改变。",
  SOURCE_COVER_INVALID: "封面内容无法识别或解码。",
  SOURCE_RESPONSE_TOO_LARGE: "封面响应超过允许大小。",
  SOURCE_REDIRECT_REFUSED: "封面跳转地址未通过来源校验。",
  SOURCE_COVER_UNAVAILABLE: "来源暂未提供可读取的封面。",
  SESSION_EXPIRED: "账号会话已失效，请重新登录。",
  AUTH_REQUIRED: "请连接账号后读取封面。",
  CREDENTIAL_CHANGED: "账号已在另一应用实例改变，请重新读取账号。",
  STALE_SESSION: "这次封面响应属于已切换的账号。",
  INVALID_RESPONSE: "封面响应格式无法确认。",
  DESKTOP_REQUIRED: "请在桌面应用中读取封面。",
  COVER_NOT_AVAILABLE: "来源未返回可用封面。",
  COVER_DECODE_FAILED: "封面数据无法解码，可重试。",
  COVER_CACHE_LIMIT: "封面大小超过本次会话缓存限额。",
  SOURCE_UNAVAILABLE: "封面暂时无法读取，可重试。",
} as const;
export type CoverErrorCode = keyof typeof messages;
export type CoverResult =
  | { status: "ready"; url: string }
  | { status: "error"; code: CoverErrorCode }
  | { status: "deferred"; reason: "cancelled" | "busy" };
export const coverErrorMessage = (code: CoverErrorCode) => messages[code];
const keyOf = (scope: SourceScope, workId: string) =>
  JSON.stringify([scope.source, scope.sessionId, workId]);
const scopeOf = (scope: SourceScope) =>
  JSON.stringify([scope.source, scope.sessionId]);
type ReadyEntry = {
  scope: string;
  result: Extract<CoverResult, { status: "ready" }>;
  bytes: number;
  asset: CoverAsset;
  token: symbol;
  users: number;
};
type ErrorEntry = {
  scope: string;
  result: Extract<CoverResult, { status: "error" }>;
  expiresAt: number;
};
type Pending = {
  scope: string;
  job: ReturnType<typeof queueCover>;
  promise: Promise<CoverResult>;
  users: number;
  discarded: boolean;
  token: symbol;
};
export interface CoverLease {
  promise: Promise<CoverResult>;
  release(): void;
}
export interface CoverAsset {
  url: string;
  size: number;
  revoke(): void;
}
function encodeCover(dataUrl: string): CoverAsset {
  const match =
    /^data:(image\/(?:png|jpeg|webp));base64,([A-Za-z0-9+/]+=*)$/.exec(dataUrl);
  if (!match) throw { code: "SOURCE_COVER_INVALID" };
  let decoded: string;
  try {
    decoded = atob(match[2]);
  } catch {
    throw { code: "SOURCE_COVER_INVALID" };
  }
  const bytes = new Uint8Array(decoded.length);
  for (let index = 0; index < decoded.length; index++)
    bytes[index] = decoded.charCodeAt(index);
  const blob = new Blob([bytes], { type: match[1] });
  const url = URL.createObjectURL(blob);
  return { url, size: blob.size, revoke: () => URL.revokeObjectURL(url) };
}

/** Compressed thumbnail Blobs survive virtual-card unmounts, without retaining base64 strings. */
export class CoverSessionCache {
  private ready = new Map<string, ReadyEntry>();
  private errors = new Map<string, ErrorEntry>();
  private pending = new Map<string, Pending>();
  private bytes = 0;
  private maximumBytes: number;
  private maximumEntries: number;
  private encode: (dataUrl: string) => CoverAsset;
  constructor(
    options: {
      maximumBytes?: number;
      maximumEntries?: number;
      encode?: (dataUrl: string) => CoverAsset;
    } = {},
  ) {
    this.maximumBytes = options.maximumBytes ?? 256 * 1024 * 1024;
    this.maximumEntries = options.maximumEntries ?? 4096;
    this.encode = options.encode ?? encodeCover;
  }
  peek(scope: SourceScope, workId: string): CoverResult | undefined {
    const key = keyOf(scope, workId);
    const success = this.ready.get(key);
    if (success) return success.result;
    const failure = this.errors.get(key);
    return failure && failure.expiresAt > Date.now()
      ? failure.result
      : undefined;
  }
  private forgetReady(key: string) {
    const entry = this.ready.get(key);
    if (entry) {
      this.bytes -= entry.bytes;
      this.ready.delete(key);
      entry.asset.revoke();
    }
  }
  private storeReady(
    key: string,
    scope: string,
    dataUrl: string,
    token: symbol,
    users: number,
  ): CoverResult {
    this.forgetReady(key);
    this.errors.delete(key);
    const asset = this.encode(dataUrl);
    // This budgets compressed Blob bytes plus metadata, not the renderer process RAM.
    const bytes = asset.size + (key.length + asset.url.length) * 2 + 192;
    if (bytes > this.maximumBytes || this.maximumEntries < 1) {
      asset.revoke();
      this.storeError(key, scope, "COVER_CACHE_LIMIT");
      return { status: "error", code: "COVER_CACHE_LIMIT" };
    }
    while (
      this.ready.size >= this.maximumEntries ||
      this.bytes + bytes > this.maximumBytes
    ) {
      const oldest = [...this.ready].find(
        ([, entry]) => entry.users === 0,
      )?.[0];
      if (oldest === undefined) {
        asset.revoke();
        return { status: "deferred", reason: "busy" };
      }
      this.forgetReady(oldest);
    }
    const result = { status: "ready", url: asset.url } as const;
    this.ready.set(key, { scope, result, bytes, asset, token, users });
    this.bytes += bytes;
    return result;
  }
  private storeError(key: string, scope: string, code: CoverErrorCode) {
    this.errors.delete(key);
    while (this.errors.size >= 256) {
      const oldest = this.errors.keys().next().value;
      if (oldest === undefined) break;
      this.errors.delete(oldest);
    }
    this.errors.set(key, {
      scope,
      result: { status: "error", code },
      expiresAt: Date.now() + 30000,
    });
  }
  acquire(
    scope: SourceScope,
    workId: string,
    load: () => Promise<string | null>,
  ): CoverLease {
    const key = keyOf(scope, workId);
    const cached = this.peek(scope, workId);
    if (cached) {
      const entry = this.ready.get(key);
      if (entry) {
        this.ready.delete(key);
        this.ready.set(key, entry);
        entry.users++;
      }
      let released = false;
      return {
        promise: Promise.resolve(cached),
        release() {
          if (!released && entry) entry.users--;
          released = true;
        },
      };
    }
    let request = this.pending.get(key);
    if (!request) {
      if (this.pending.size >= 128)
        return {
          promise: Promise.resolve({ status: "deferred", reason: "busy" }),
          release() {},
        };
      const job = queueCover(load);
      const created: Pending = {
        scope: scopeOf(scope),
        job,
        promise: Promise.resolve({ status: "deferred", reason: "cancelled" }),
        users: 0,
        discarded: false,
        token: Symbol(),
      };
      created.promise = job.promise
        .then((dataUrl): CoverResult => {
          if (created.discarded)
            return { status: "deferred", reason: "cancelled" };
          if (dataUrl !== null) {
            return this.storeReady(
              key,
              created.scope,
              dataUrl,
              created.token,
              created.users,
            );
          }
          this.storeError(key, created.scope, "COVER_NOT_AVAILABLE");
          return { status: "error", code: "COVER_NOT_AVAILABLE" };
        })
        .catch((cause: unknown): CoverResult => {
          if (created.discarded)
            return { status: "deferred", reason: "cancelled" };
          if (cause instanceof Error && cause.message === "COVER_QUEUE_FULL")
            return { status: "deferred", reason: "busy" };
          const candidate =
            typeof cause === "object" && cause !== null && "code" in cause
              ? cause.code
              : undefined;
          const code: CoverErrorCode =
            typeof candidate === "string" && Object.hasOwn(messages, candidate)
              ? (candidate as CoverErrorCode)
              : "SOURCE_UNAVAILABLE";
          this.storeError(key, created.scope, code);
          return { status: "error", code };
        })
        .finally(() => {
          if (this.pending.get(key) === created) this.pending.delete(key);
        });
      this.pending.set(key, created);
      request = created;
    }
    const shared = request;
    shared.users++;
    let released = false;
    return {
      promise: shared.promise,
      release: () => {
        if (released) return;
        released = true;
        shared.users--;
        const ready = this.ready.get(key);
        if (ready?.token === shared.token) ready.users--;
        if (shared.users === 0 && shared.job.cancel()) {
          shared.discarded = true;
          if (this.pending.get(key) === shared) this.pending.delete(key);
        }
      },
    };
  }
  retryFailures(scope: SourceScope) {
    const wanted = scopeOf(scope);
    for (const [key, entry] of this.errors)
      if (entry.scope === wanted) this.errors.delete(key);
  }
  decodeFailed(scope: SourceScope, workId: string, url: string) {
    const key = keyOf(scope, workId);
    const latest = this.ready.get(key);
    // Revoked, evicted or replaced URLs are stale UI events, not negative evidence.
    if (!latest || latest.result.url !== url) return;
    this.forgetReady(key);
    this.storeError(key, scopeOf(scope), "COVER_DECODE_FAILED");
  }
  retainScopes(scopes: SourceScope[]) {
    const keep = new Set(scopes.map(scopeOf));
    for (const [key, entry] of this.ready)
      if (!keep.has(entry.scope)) this.forgetReady(key);
    for (const [key, entry] of this.errors)
      if (!keep.has(entry.scope)) this.errors.delete(key);
    for (const [key, request] of this.pending)
      if (!keep.has(request.scope)) {
        request.discarded = true;
        request.job.cancel();
        this.pending.delete(key);
      }
  }
}
const caches = new WeakMap<SourceAdapter, CoverSessionCache>();
export function getCoverCache(adapter: SourceAdapter) {
  let cache = caches.get(adapter);
  if (!cache) {
    cache = new CoverSessionCache();
    caches.set(adapter, cache);
  }
  return cache;
}
