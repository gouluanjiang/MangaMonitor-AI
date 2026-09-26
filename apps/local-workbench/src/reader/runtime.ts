import { invokeDesktop } from "../runtime.ts";
import type {
  ReaderAdapter,
  ReaderBook,
  ReaderChapterInfo,
  ReaderImage,
  ReaderPosition,
} from "./types.ts";

export class ReaderError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.name = "ReaderError";
    this.code = code;
  }
}
// Only the host's curated download messages may be shown verbatim in the reader.
export class ReaderDownloadError extends Error {}
export function readerErrorMessage(error: unknown): string {
  const code =
    error instanceof ReaderError
      ? error.code
      : typeof error === "string"
        ? error
        : error &&
            typeof error === "object" &&
            "code" in error &&
            typeof error.code === "string"
          ? error.code
          : error instanceof Error && /^[A-Z][A-Z0-9_]+$/.test(error.message)
            ? error.message
            : "";
  if (/SESSION|AUTH|LOGIN/.test(code))
    return "来源账号需要重新连接。已保存的阅读位置会保留。";
  if (/CHANGED|STALE|MISSING|NOT_FOUND/.test(code))
    return "文件或来源内容已发生变化，请退出后重新打开。";
  if (/LIMIT|TOO_LARGE|MEMORY/.test(code))
    return "这一页超出阅读大小限制，可以跳过此页继续阅读。";
  if (/FORMAT|DECODE|IMAGE/.test(code))
    return "这一页图片无法显示，可以重试或跳过此页。";
  return "暂时无法读取，请重试。已保存的阅读位置会保留。";
}
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new ReaderError("READER_RESPONSE_INVALID");
  return value as Record<string, unknown>;
}
function text(value: unknown): string {
  if (typeof value !== "string" || !value.length || value.length > 16384)
    throw new ReaderError("READER_RESPONSE_INVALID");
  return value;
}
function integer(value: unknown, minimum = 0): number {
  if (!Number.isSafeInteger(value) || (value as number) < minimum)
    throw new ReaderError("READER_RESPONSE_INVALID");
  return value as number;
}
function position(value: unknown): ReaderPosition {
  const raw = record(value);
  if (
    typeof raw.offset !== "number" ||
    !Number.isFinite(raw.offset) ||
    raw.offset < 0 ||
    raw.offset > 1
  )
    throw new ReaderError("READER_RESPONSE_INVALID");
  return {
    chapterId: text(raw.chapterId),
    pageIndex: integer(raw.pageIndex),
    offset: raw.offset,
  };
}
export function parseReaderBook(value: unknown): ReaderBook {
  const raw = record(value);
  if (
    !["library", "JM", "Pica"].includes(String(raw.origin)) ||
    !Array.isArray(raw.chapters) ||
    raw.chapters.length < 1 ||
    raw.chapters.length > 20000
  )
    throw new ReaderError("READER_RESPONSE_INVALID");
  const chapters = raw.chapters.map((v) => {
    const chapter = record(v);
    return {
      id: text(chapter.id),
      title: text(chapter.title),
      pageCount:
        chapter.pageCount === null ? null : integer(chapter.pageCount, 1),
    };
  });
  if (new Set(chapters.map((v) => v.id)).size !== chapters.length)
    throw new ReaderError("READER_RESPONSE_INVALID");
  const source = raw.sourceRef === null ? null : record(raw.sourceRef);
  if (source && source.source !== "JM" && source.source !== "Pica")
    throw new ReaderError("READER_RESPONSE_INVALID");
  return {
    readerId: text(raw.readerId),
    title: text(raw.title),
    origin: raw.origin as ReaderBook["origin"],
    sourceRef: source
      ? { source: source.source as "JM" | "Pica", workId: text(source.workId) }
      : null,
    chapters,
    position: raw.position === null ? null : position(raw.position),
  };
}
export function parseReaderChapter(
  value: unknown,
  readerId: string,
  chapterId: string,
): ReaderChapterInfo {
  const raw = record(value);
  if (raw.readerId !== readerId || raw.chapterId !== chapterId)
    throw new ReaderError("READER_STALE_RESPONSE");
  const pageCount = integer(raw.pageCount, 1);
  if (pageCount > 50000) throw new ReaderError("READER_LIMIT");
  return { readerId, chapterId, pageCount };
}
export function parseReaderImage(
  value: unknown,
  readerId: string,
  chapterId: string,
  pageIndex: number,
): ReaderImage {
  const raw = record(value);
  if (
    raw.readerId !== readerId ||
    raw.chapterId !== chapterId ||
    raw.pageIndex !== pageIndex
  )
    throw new ReaderError("READER_STALE_RESPONSE");
  if (
    typeof raw.dataUrl !== "string" ||
    !/^data:image\/(?:jpeg|png|webp|gif);base64,[A-Za-z0-9+/=]+$/.test(
      raw.dataUrl,
    )
  )
    throw new ReaderError("READER_IMAGE_INVALID");
  const width = integer(raw.width, 1),
    height = integer(raw.height, 1);
  if (
    raw.dataUrl.length > 64 * 1024 * 1024 ||
    width > 20000 ||
    height > 20000 ||
    width * height > 32_000_000
  )
    throw new ReaderError("READER_IMAGE_LIMIT");
  return {
    readerId,
    chapterId,
    pageIndex,
    dataUrl: raw.dataUrl,
    width,
    height,
  };
}
export const nativeReaderAdapter: ReaderAdapter = {
  async open(request, requestId) {
    return parseReaderBook(
      await invokeDesktop("reader_open", { request, requestId }),
    );
  },
  async cancelOpen(requestId) {
    await invokeDesktop("reader_cancel_open", { requestId });
  },
  async chapter(readerId, chapterId) {
    return parseReaderChapter(
      await invokeDesktop("reader_chapter", { readerId, chapterId }),
      readerId,
      chapterId,
    );
  },
  async page(readerId, chapterId, pageIndex) {
    return parseReaderImage(
      await invokeDesktop("reader_page", { readerId, chapterId, pageIndex }),
      readerId,
      chapterId,
      pageIndex,
    );
  },
  async savePosition(readerId, value) {
    await invokeDesktop("reader_save_position", {
      readerId,
      position: position(value),
    });
  },
  async close(readerId) {
    await invokeDesktop("reader_close", { readerId });
  },
  async fullscreen(fullscreen) {
    await invokeDesktop("reader_fullscreen", { fullscreen });
  },
};
