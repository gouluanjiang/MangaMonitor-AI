import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import type {
  AccountSummary,
  CatalogSnapshot,
  FavoriteResult,
  FollowingSnapshot,
  Source,
  SourceAdapter,
  SourcePage,
  SourceQueryResult,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import { sources } from "./source-types.ts";

export class SourceError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.name = "SourceError";
    this.code = code;
  }
}
export function sourceErrorMessage(error: unknown): string {
  const code = error instanceof SourceError ? error.code : "SOURCE_UNAVAILABLE";
  if (code === "CATALOG_CHANGED")
    return "收藏范围在读取期间发生变化，已读内容保留。请重新读取完整收藏。";
  if (code === "CATALOG_LIMIT")
    return "当前收藏索引达到 20000 部、1000 页或 32 MiB 内存上限，已读内容保留。";
  if (code === "DESKTOP_REQUIRED")
    return "请在桌面应用中连接来源账号。浏览器预览不会连接真实账号。";
  if (
    /SESSION|LOGIN_REQUIRED|AUTH_REQUIRED|AUTH_EXPIRED|UNAUTHORIZED/.test(code)
  )
    return "账号会话已失效，请重新登录后重试。";
  if (/LOGIN_FAILED|LOGIN_REJECTED|INVALID_CREDENTIAL|AUTH_FAILED/.test(code))
    return "登录未成功，请检查账号信息后重试。";
  if (code === "CREDENTIAL_CHANGED")
    return "登录状态已在另一应用实例改变，请重新读取账号状态后再操作。";
  if (code === "VAULT_BUSY") return "系统会话存储正在使用中，请稍后重试。";
  if (/CREDENTIAL|KEYRING|VAULT|SECURE_STORE/.test(code))
    return "系统安全存储暂不可用，未改用明文保存会话。";
  if (/CONFLICT/.test(code))
    return "本机关注已在另一处改变。请重新读取后，再次确认这次操作。";
  if (
    /UNVERIFIED|UNCONFIRMED|READBACK|FAVORITE_OUTCOME_UNKNOWN|FAVORITE_RECONCILIATION_REQUIRED/.test(
      code,
    )
  )
    return "收藏操作结果未确认，请先重新读取核对。确认之前不会再次提交收藏操作。";
  if (code === "SOURCE_ACCESS_DENIED")
    return "来源拒绝访问或需要额外验证。账号会话未被清除，请稍后重试。";
  if (/RATE|BUDGET|BUSY/.test(code))
    return "来源请求暂时繁忙或达到限制，请稍后重试。";
  if (/INVALID|MALFORMED|VALIDATION/.test(code))
    return "来源数据或输入无法确认，请检查输入或重新读取。";
  return "来源暂时无法读取，请稍后重试。已读取内容会保留。";
}
const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, maximum = 10000): value is string =>
  typeof value === "string" && value.length <= maximum;
const nullableText = (value: unknown) => value === null || text(value);
const integer = (value: unknown) =>
  Number.isSafeInteger(value) && (value as number) >= 0;
const nullableInteger = (value: unknown) => value === null || integer(value);
const identity = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value);
function invalid(): never {
  throw new SourceError("INVALID_RESPONSE");
}
function validSource(value: unknown): value is Source {
  return value === "JM" || value === "Pica";
}
function scoped(
  value: unknown,
  scope: SourceScope,
): asserts value is Record<string, unknown> {
  if (
    !object(value) ||
    value.source !== scope.source ||
    value.sessionId !== scope.sessionId
  ) {
    throw new SourceError("STALE_SESSION");
  }
}
export function validateSourceWork(
  value: unknown,
  source?: Source,
): SourceWork {
  if (
    !object(value) ||
    !validSource(value.source) ||
    (source && value.source !== source) ||
    !identity(value.workId) ||
    !text(value.title) ||
    !value.title.trim() ||
    !Array.isArray(value.authors) ||
    !value.authors.every((item) => text(item, 2000)) ||
    !nullableText(value.description) ||
    !Array.isArray(value.tags) ||
    !value.tags.every((item) => text(item, 2000)) ||
    !(value.favorite === null || typeof value.favorite === "boolean") ||
    !nullableInteger(value.chapterCount) ||
    !nullableInteger(value.pageCount) ||
    typeof value.coverAvailable !== "boolean"
  )
    invalid();
  // Construct a new DTO. Never forward raw native fields such as URLs or credentials.
  return {
    source: value.source,
    workId: value.workId,
    title: value.title,
    authors: [...value.authors] as string[],
    description: value.description as string | null,
    tags: [...value.tags] as string[],
    favorite: value.favorite as boolean | null,
    chapterCount: value.chapterCount as number | null,
    pageCount: value.pageCount as number | null,
    coverAvailable: value.coverAvailable,
  };
}
export function validateAccount(value: unknown): AccountSummary {
  if (
    !object(value) ||
    !validSource(value.source) ||
    !["disconnected", "connected", "expired", "unavailable"].includes(
      value.state as string,
    ) ||
    !nullableText(value.sessionId) ||
    !nullableText(value.accountId) ||
    !nullableText(value.displayName) ||
    typeof value.remembered !== "boolean" ||
    !nullableText(value.errorCode) ||
    (value.state === "connected" && (!value.sessionId || !value.accountId))
  )
    invalid();
  return {
    source: value.source,
    sessionId: value.sessionId as string | null,
    accountId: value.accountId as string | null,
    displayName: value.displayName as string | null,
    state: value.state as AccountSummary["state"],
    remembered: value.remembered,
    errorCode: value.errorCode as string | null,
  };
}
export function validateSourcePage(
  value: unknown,
  scope: SourceScope,
): SourceQueryResult {
  scoped(value, scope);
  if (
    !Array.isArray(value.items) ||
    !integer(value.page) ||
    (value.page as number) < 1 ||
    !nullableInteger(value.total) ||
    !nullableInteger(value.pages) ||
    !(value.hasMore === null || typeof value.hasMore === "boolean") ||
    !Array.isArray(value.folders)
  )
    invalid();
  const folders: SourcePage["folders"] = value.folders.map(
    (folder: unknown) => {
      if (
        !object(folder) ||
        !identity(folder.id) ||
        !text(folder.name, 2000) ||
        !nullableInteger(folder.count)
      )
        invalid();
      return {
        id: folder.id,
        name: folder.name,
        count: folder.count as number | null,
      };
    },
  );
  const items = value.items.map((item: unknown) =>
    validateSourceWork(item, scope.source),
  );
  if (new Set(items.map((item) => item.workId)).size !== items.length)
    throw new SourceError("CATALOG_CHANGED");
  return {
    ...scope,
    items,
    page: value.page as number,
    total: value.total as number | null,
    pages: value.pages as number | null,
    hasMore: value.hasMore as boolean | null,
    folders,
  };
}
export function validateCatalogSnapshot(
  value: unknown,
  scope: SourceScope,
): CatalogSnapshot {
  if (
    !object(value) ||
    !Array.isArray(value.items) ||
    value.items.length > 20000 ||
    typeof value.complete !== "boolean" ||
    !integer(value.updatedAt) ||
    !Array.isArray(value.firstPageIds) ||
    value.firstPageIds.length > 1000 ||
    !value.firstPageIds.every(identity)
  )
    invalid();
  const page = validateSourcePage({ ...value, ...scope }, scope);
  if (
    page.page > 1000 ||
    value.firstPageIds.length > page.items.length ||
    value.firstPageIds.some((id, index) => page.items[index]?.workId !== id) ||
    (page.items.length > 0 && !value.firstPageIds.length) ||
    (page.page === 1 &&
      (value.firstPageIds.length !== page.items.length ||
        page.items.length > 1000)) ||
    (page.page > 1 &&
      (page.items.length <= value.firstPageIds.length ||
        page.items.length > page.page * 1000)) ||
    (!page.items.length &&
      !(page.page === 1 && page.total === 0 && value.complete)) ||
    (!value.complete &&
      (page.hasMore === false ||
        (page.pages !== null && page.page === Math.max(1, page.pages)) ||
        (page.total !== null && page.total === page.items.length))) ||
    (value.complete &&
      ((page.hasMore !== false &&
        !(page.pages !== null && page.page === Math.max(1, page.pages))) ||
        page.hasMore === true ||
        (page.total !== null && page.total !== page.items.length))) ||
    (page.pages !== null &&
      (page.page > Math.max(1, page.pages) ||
        (value.complete && page.pages > page.page) ||
        (page.page === page.pages && page.hasMore === true) ||
        (page.pages === 0 && page.items.length > 0))) ||
    new TextEncoder().encode(JSON.stringify(value)).byteLength >
      32 * 1024 * 1024
  )
    invalid();
  return {
    items: page.items,
    page: page.page,
    total: page.total,
    pages: page.pages,
    hasMore: page.hasMore,
    folders: page.folders,
    complete: value.complete,
    updatedAt: value.updatedAt as number,
    firstPageIds: [...value.firstPageIds] as string[],
  };
}
function validateFollowing(
  value: unknown,
  scope: SourceScope,
): FollowingSnapshot {
  scoped(value, scope);
  if (
    !integer(value.revision) ||
    !Array.isArray(value.works) ||
    !Array.isArray(value.authors) ||
    !value.authors.every((name) => text(name, 2000) && name.trim())
  )
    invalid();
  const works = value.works.map((work: unknown) => {
    if (
      !object(work) ||
      !identity(work.workId) ||
      !text(work.title) ||
      !work.title.trim()
    )
      invalid();
    return { workId: work.workId, title: work.title };
  });
  if (
    new Set(works.map((work) => work.workId)).size !== works.length ||
    new Set(value.authors).size !== value.authors.length
  )
    invalid();
  return {
    ...scope,
    revision: value.revision as number,
    works,
    authors: [...value.authors] as string[],
  };
}
type Invoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;
export function createSourceAdapter(
  options: { native?: boolean; invoke?: Invoke } = {},
): SourceAdapter {
  const available = options.native ?? isDesktopRuntime();
  const invoke = options.invoke ?? invokeDesktop;
  async function call(command: string, args?: Record<string, unknown>) {
    if (!available) throw new SourceError("DESKTOP_REQUIRED");
    try {
      return await invoke(command, args);
    } catch (error) {
      if (error instanceof SourceError) throw error;
      const code =
        object(error) &&
        typeof error.code === "string" &&
        /^[A-Z0-9_]{1,80}$/.test(error.code)
          ? error.code
          : "SOURCE_UNAVAILABLE";
      throw new SourceError(code); // Never retain or display raw native error text.
    }
  }
  function checkScope(scope: SourceScope) {
    if (
      !validSource(scope.source) ||
      !text(scope.sessionId, 2048) ||
      !scope.sessionId
    ) {
      throw new SourceError("LOGIN_REQUIRED");
    }
  }
  return {
    available,
    mode: available ? "native" : "unavailable",
    async accounts(refresh = false) {
      if (!available)
        return sources.map((source) => ({
          source,
          sessionId: null,
          accountId: null,
          displayName: null,
          state: "unavailable",
          remembered: false,
          errorCode: "DESKTOP_REQUIRED",
        }));
      const result = await call("source_accounts", { refresh });
      if (!Array.isArray(result)) invalid();
      const accounts = result.map(validateAccount);
      if (
        accounts.length !== 2 ||
        new Set(accounts.map((account) => account.source)).size !== 2
      )
        invalid();
      return accounts;
    },
    async login(input) {
      if (
        !validSource(input.source) ||
        !input.username.trim() ||
        !input.password ||
        typeof input.remember !== "boolean"
      ) {
        throw new SourceError("INVALID_INPUT");
      }
      const result = validateAccount(await call("source_login", { ...input }));
      if (result.source !== input.source) invalid();
      if (result.state !== "connected")
        throw new SourceError(result.errorCode ?? "LOGIN_FAILED");
      return result;
    },
    async logout(scope) {
      if (
        !validSource(scope.source) ||
        !(
          scope.sessionId === null ||
          (text(scope.sessionId, 2048) && scope.sessionId)
        )
      )
        throw new SourceError("INVALID_INPUT");
      const result = validateAccount(await call("source_logout", { ...scope }));
      if (result.source !== scope.source || result.state === "connected")
        invalid();
      return result;
    },
    async query(scope, query) {
      checkScope(scope);
      if (
        !["favorites", "search", "detail"].includes(query.kind) ||
        !text(query.query, 4096) ||
        !integer(query.page) ||
        query.page < 1 ||
        !(query.folderId === null || identity(query.folderId)) ||
        (scope.source === "Pica" && query.folderId !== null) ||
        (query.reverse !== undefined && typeof query.reverse !== "boolean")
      )
        throw new SourceError("INVALID_INPUT");
      const result = validateSourcePage(
        await call("source_query", {
          ...scope,
          ...query,
          reverse: query.reverse ?? false,
        }),
        scope,
      );
      if (result.page !== query.page || result.items.length > 1000) invalid();
      return result;
    },
    async catalog(scope, request) {
      checkScope(scope);
      if (
        !["read", "write"].includes(request.action) ||
        typeof request.reverse !== "boolean" ||
        !(request.folderId === null || identity(request.folderId)) ||
        (scope.source === "Pica" && request.folderId !== null)
      )
        throw new SourceError("INVALID_INPUT");
      const snapshot =
        request.action === "write"
          ? validateCatalogSnapshot(request.snapshot, scope)
          : undefined;
      const result = await call("source_catalog", {
        ...scope,
        folderId: request.folderId,
        reverse: request.reverse,
        action: request.action,
        ...(snapshot ? { snapshot } : {}),
      });
      scoped(result, scope);
      const current =
        result.snapshot === null
          ? null
          : validateCatalogSnapshot(result.snapshot, scope);
      const complete =
        result.completeSnapshot === null
          ? null
          : validateCatalogSnapshot(result.completeSnapshot, scope);
      if (complete && !complete.complete) invalid();
      return { ...scope, snapshot: current, completeSnapshot: complete };
    },
    async favorite(scope, workId, desired) {
      checkScope(scope);
      if (!identity(workId) || typeof desired !== "boolean")
        throw new SourceError("INVALID_INPUT");
      const result = await call("source_favorite", {
        ...scope,
        workId,
        desired,
      });
      scoped(result, scope);
      if (
        result.workId !== workId ||
        result.verified !== true ||
        result.favorite !== desired ||
        typeof result.changed !== "boolean"
      )
        throw new SourceError("UNCONFIRMED_WRITE");
      return {
        ...scope,
        workId,
        favorite: desired,
        changed: result.changed,
        verified: true,
      } as FavoriteResult;
    },
    async cover(scope, workId) {
      checkScope(scope);
      if (!identity(workId)) throw new SourceError("INVALID_INPUT");
      const result = await call("source_cover", { ...scope, workId });
      scoped(result, scope);
      if (
        result.workId !== workId ||
        !(
          result.dataUrl === null ||
          (text(result.dataUrl, 12 * 1024 * 1024) &&
            /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/]+=*$/.test(
              result.dataUrl,
            ))
        )
      )
        invalid();
      return result.dataUrl as string | null;
    },
    async following(scope) {
      checkScope(scope);
      return validateFollowing(
        await call("source_following", { ...scope }),
        scope,
      );
    },
    async follow(scope, mutation) {
      checkScope(scope);
      if (
        !["work", "author"].includes(mutation.kind) ||
        !text(mutation.value, 2000) ||
        !mutation.value.trim() ||
        (mutation.kind === "work" && !identity(mutation.value)) ||
        typeof mutation.desired !== "boolean" ||
        !integer(mutation.expectedRevision)
      )
        throw new SourceError("INVALID_INPUT");
      return validateFollowing(
        await call("source_follow", { ...scope, ...mutation }),
        scope,
      );
    },
  };
}
