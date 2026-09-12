import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { emptyLibrary } from "./library-types.ts";
import type {
  LibraryAdapter,
  LibraryCover,
  LibraryItem,
  LibraryReference,
  LibraryScanAction,
  LibrarySnapshot,
} from "./library-types.ts";
import { parseLibraryReference } from "./library-model.ts";

export class LibraryError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.name = "LibraryError";
    this.code = code;
  }
}
const bad = (): never => {
  throw new LibraryError("LIBRARY_RESPONSE_INVALID");
};
const record = (value: unknown): Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : bad();
const text = (value: unknown, max = 16384): string =>
  typeof value === "string" && value.length <= max ? value : bad();
const integer = (value: unknown): number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : bad();
const nullableNumber = (value: unknown) =>
  value === null ? null : integer(value);
const nullableText = (value: unknown) => (value === null ? null : text(value));
const id = (value: unknown): string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value) ? value : bad();
function strings(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > 1000) return bad();
  return value.map((v) => text(v));
}
function reference(value: unknown): LibraryReference | null {
  if (value === null) return null;
  const raw = record(value);
  if (raw.source !== "JM" && raw.source !== "Pica") return bad();
  const result = parseLibraryReference(raw.source, text(raw.workId, 64));
  if (!result || result.workId !== raw.workId) return bad();
  return result;
}
function item(value: unknown): LibraryItem {
  const raw = record(value);
  if (
    !["zip", "cbz", "rar", "directory"].includes(String(raw.format)) ||
    !["indexed", "unreadable", "unsupported"].includes(String(raw.state)) ||
    ![null, "metadata", "filename", "manual"].includes(
      raw.identityEvidence as string | null,
    ) ||
    typeof raw.coverAvailable !== "boolean"
  )
    return bad();
  const relativePath = text(raw.relativePath);
  if (
    !relativePath ||
    /^[\\/]|^[a-z]:/i.test(relativePath) ||
    relativePath
      .split(/[\\/]/)
      .some((part) => part === ".." || part === "." || !part)
  )
    return bad();
  const fileName = text(raw.fileName);
  if (!fileName || /[\\/]/.test(fileName)) return bad();
  const sourceRef = reference(raw.sourceRef);
  if ((sourceRef === null) !== (raw.identityEvidence === null)) return bad();
  return {
    id: id(raw.id),
    relativePath,
    fileName,
    format: raw.format as LibraryItem["format"],
    title: text(raw.title),
    authors: strings(raw.authors),
    description: nullableText(raw.description),
    tags: strings(raw.tags),
    bytes: integer(raw.bytes),
    modifiedAt: nullableNumber(raw.modifiedAt),
    pageCount: nullableNumber(raw.pageCount),
    coverAvailable: raw.coverAvailable,
    state: raw.state as LibraryItem["state"],
    errorCode: nullableText(raw.errorCode),
    sourceRef,
    identityEvidence: raw.identityEvidence as LibraryItem["identityEvidence"],
  };
}
export function validateLibrarySnapshot(value: unknown): LibrarySnapshot {
  const raw = record(value);
  if (
    !["idle", "reading", "paused", "complete", "error"].includes(
      String(raw.phase),
    ) ||
    !["none", "cached", "live"].includes(String(raw.freshness)) ||
    !Array.isArray(raw.items) ||
    raw.items.length > 20000
  )
    return bad();
  const items = raw.items.map(item);
  if (new Set(items.map((value) => value.id)).size !== items.length)
    return bad();
  const rootId = raw.rootId === null ? null : id(raw.rootId);
  const rootPath = nullableText(raw.rootPath);
  if (
    (rootId === null) !== (rootPath === null) ||
    (rootId === null && (items.length > 0 || raw.phase !== "idle"))
  )
    return bad();
  return {
    revision: integer(raw.revision),
    rootId,
    rootPath,
    generation: integer(raw.generation),
    phase: raw.phase as LibrarySnapshot["phase"],
    freshness: raw.freshness as LibrarySnapshot["freshness"],
    items,
    visited: integer(raw.visited),
    skipped: integer(raw.skipped),
    updatedAt: nullableNumber(raw.updatedAt),
    errorCode: nullableText(raw.errorCode),
  };
}
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createLibraryAdapter(
  options: { invoke?: Invoke; native?: boolean } = {},
): LibraryAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  async function call(
    command: string,
    args: Record<string, unknown> = {},
  ): Promise<unknown> {
    if (!(options.native ?? isDesktopRuntime()))
      throw new LibraryError("DESKTOP_REQUIRED");
    try {
      return await invoke(command, args);
    } catch (cause) {
      const code = (cause as { code?: unknown })?.code;
      throw new LibraryError(
        typeof code === "string" && /^[A-Z_]{1,80}$/.test(code)
          ? code
          : "LIBRARY_UNAVAILABLE",
      );
    }
  }
  return {
    read: async () => validateLibrarySnapshot(await call("library_read")),
    choose: async () => {
      const result = await call("library_choose");
      return result === null ? null : validateLibrarySnapshot(result);
    },
    scan: async (rootId, generation, action) => {
      if (!["start", "next", "pause", "resume"].includes(action)) return bad();
      return validateLibrarySnapshot(
        await call("library_scan", {
          rootId: id(rootId),
          generation: integer(generation),
          action,
        }),
      );
    },
    link: async (rootId, generation, entryId, ref) =>
      validateLibrarySnapshot(
        await call("library_link", {
          rootId: id(rootId),
          generation: integer(generation),
          entryId: id(entryId),
          reference: reference(ref),
        }),
      ),
    cover: async (rootId, generation, entryId) => {
      id(rootId);
      integer(generation);
      id(entryId);
      const result = record(
        await call("library_cover", { rootId, generation, entryId }),
      );
      if (
        result.rootId !== rootId ||
        result.generation !== generation ||
        result.entryId !== entryId ||
        (result.dataUrl !== null &&
          (typeof result.dataUrl !== "string" ||
            result.dataUrl.length > 349560 ||
            !/^data:image\/jpeg;base64,[A-Za-z0-9+/]+={0,2}$/.test(
              result.dataUrl,
            )))
      )
        return bad();
      return result as unknown as LibraryCover;
    },
  };
}
export function libraryErrorMessage(cause: unknown): string {
  const code =
    typeof cause === "string" ? cause : (cause as { code?: string })?.code;
  switch (code) {
    case "LIBRARY_FILE_CHANGED":
    case "LIBRARY_STALE":
    case "LIBRARY_STALE_GENERATION":
    case "LIBRARY_STALE_SNAPSHOT":
    case "LIBRARY_ROOT_CHANGED":
      return "文件或目录已改变，请重新读取漫画库。";
    case "LIBRARY_UNSUPPORTED":
    case "LIBRARY_RAR_UNSUPPORTED":
      return "RAR 暂不支持读取封面和内容，原文件保留。";
    case "LIBRARY_ARCHIVE_UNSUPPORTED":
      return "此压缩包的编码或分卷格式暂不支持，原文件保留。";
    case "LIBRARY_SCAN_RESTART_REQUIRED":
    case "LIBRARY_RESTART_REQUIRED":
      return "上次读取已结束，请重新读取目录。";
    case "LIBRARY_LIMIT_EXCEEDED":
    case "LIBRARY_LIMIT_REACHED":
    case "LIBRARY_ARCHIVE_LIMIT":
    case "LIBRARY_ENTRY_LIMIT":
      return "目录或压缩包超过本批读取范围，已读取内容保留。";
    case "LIBRARY_DOWNLOAD_INCOMPLETE":
      return "发现下载中的章节，当前页数仅包含已保存的正文图片。";
    case "LIBRARY_COVER_ONLY":
      return "目前只有封面，未读到正文图片。";
    case "LIBRARY_IDENTITY_CONFLICT":
      return "文件名与元数据中的来源编号不一致，请手动确认关联。";
    case "LIBRARY_METADATA_INVALID":
    case "LIBRARY_METADATA_LIMIT":
      return "作品元数据未能读取，已保留文件名和可读取的图片信息。";
    case "LIBRARY_SCAN_INCOMPLETE":
      return "部分目录未能读完，已读取的作品保留。请检查提示后重新读取。";
    case "LIBRARY_RESPONSE_INVALID":
      return "漫画库数据无法确认，请重新读取。";
    case "DESKTOP_REQUIRED":
      return "请在桌面应用中选择漫画库。";
    default:
      return "漫画库暂时无法读取，已读取内容保留。请检查目录后重试。";
  }
}
export interface LibraryControllerState {
  snapshot: LibrarySnapshot;
  error: string;
  busy: boolean;
}
/** One bounded native batch at a time. Only explicit actions start scanning. */
export class LibraryController {
  readonly adapter: LibraryAdapter;
  private state: LibraryControllerState = {
    snapshot: emptyLibrary(),
    error: "",
    busy: false,
  };
  private listeners = new Set<(state: LibraryControllerState) => void>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private epoch = 0;
  private paused = false;
  constructor(adapter: LibraryAdapter) {
    this.adapter = adapter;
  }
  getState() {
    return this.state;
  }
  subscribe(listener: (state: LibraryControllerState) => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private publish(value: Partial<LibraryControllerState>) {
    this.state = { ...this.state, ...value };
    for (const listener of this.listeners) listener(this.state);
  }
  private async run(
    operation: () => Promise<LibrarySnapshot | null>,
    drive = false,
  ) {
    if (this.state.busy) return;
    clearTimeout(this.timer);
    const token = this.epoch;
    const previousError = this.state.error;
    let accepted = false;
    this.publish({ busy: true, error: "" });
    try {
      const snapshot = await operation();
      if (token !== this.epoch) return;
      if (snapshot) {
        accepted = true;
        this.publish({
          snapshot,
          error: snapshot.errorCode
            ? libraryErrorMessage(snapshot.errorCode)
            : "",
        });
      } else this.publish({ error: previousError });
    } catch (cause) {
      if (token === this.epoch)
        this.publish({ error: libraryErrorMessage(cause) });
    } finally {
      if (token === this.epoch) {
        this.publish({ busy: false });
        if (
          drive &&
          accepted &&
          this.paused &&
          !this.state.error &&
          this.state.snapshot.phase === "reading"
        )
          void this.scan("pause");
        else if (
          drive &&
          accepted &&
          !this.paused &&
          !this.state.error &&
          this.state.snapshot.phase === "reading"
        )
          this.timer = setTimeout(() => void this.scan("next"), 80);
      }
    }
  }
  read() {
    this.paused = false;
    return this.run(() => this.adapter.read());
  }
  choose() {
    this.paused = false;
    return this.run(() => this.adapter.choose(), true);
  }
  scan(action: LibraryScanAction) {
    if (action === "pause") {
      this.paused = true;
      clearTimeout(this.timer);
      if (this.state.busy) return Promise.resolve();
    } else if (action !== "next") this.paused = false;
    if (action === "next" && (this.paused || this.state.error))
      return Promise.resolve();
    const { rootId, generation } = this.state.snapshot;
    if (!rootId) return Promise.resolve();
    return this.run(async () => {
      const snapshot = await this.adapter.scan(rootId, generation, action);
      if (
        snapshot.rootId !== rootId ||
        (action === "start"
          ? snapshot.generation <= generation
          : snapshot.generation !== generation)
      )
        return bad();
      return snapshot;
    }, action !== "pause");
  }
  link(entryId: string, reference: LibraryReference | null) {
    const { rootId, generation } = this.state.snapshot;
    if (!rootId) return Promise.resolve();
    return this.run(async () => {
      const snapshot = await this.adapter.link(
        rootId,
        generation,
        entryId,
        reference,
      );
      if (snapshot.rootId !== rootId || snapshot.generation !== generation)
        return bad();
      return snapshot;
    }, true);
  }
  dispose() {
    this.epoch++;
    clearTimeout(this.timer);
    this.listeners.clear();
    this.state = { ...this.state, busy: false };
  }
}
