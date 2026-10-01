import type { SourceScope, SourceWork } from "./source-types.ts";
import { validateSourceWork } from "./source-runtime.ts";

export interface SpecialRun {
  id: number;
  phase: string;
  startedAt: number | null;
  finishedAt: number | null;
  newCount: number;
  errorCode: string | null;
}
export interface SpecialSnapshot {
  scopes: SourceScope[];
  authors: {
    author: string;
    enabled: boolean;
    baselinesComplete: number;
    errorCodes: string[];
  }[];
  updates: {
    work: SourceWork;
    authors: string[];
    discoveredAt: number;
    readAt: number | null;
  }[];
  run: SpecialRun;
}
const record = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("SPECIAL_RESPONSE_INVALID");
  return value as Record<string, unknown>;
};
function integer(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}
function nullableTime(value: unknown) {
  return value === null || integer(value);
}
export function validateSpecialRun(value: unknown): SpecialRun {
  const run = record(value);
  if (
    !integer(run.id) ||
    !integer(run.newCount) ||
    ![
      "",
      "idle",
      "waiting",
      "checking",
      "complete",
      "partial",
      "error",
      "unavailable",
      "cancelled",
    ].includes(String(run.phase)) ||
    !nullableTime(run.startedAt) ||
    !nullableTime(run.finishedAt) ||
    !(run.errorCode === null || typeof run.errorCode === "string")
  )
    throw new Error("SPECIAL_RESPONSE_INVALID");
  return { ...run, phase: run.phase || "idle" } as unknown as SpecialRun;
}
export function validateSpecialSnapshot(
  value: unknown,
  scopes: SourceScope[],
): SpecialSnapshot {
  const result = record(value);
  if (
    JSON.stringify(result.scopes) !== JSON.stringify(scopes) ||
    !Array.isArray(result.authors) ||
    result.authors.length > 2000 ||
    !Array.isArray(result.updates) ||
    result.updates.length > 100000
  )
    throw new Error("SPECIAL_RESPONSE_INVALID");
  const authors = result.authors.map((value) => {
    const row = record(value);
    if (
      typeof row.author !== "string" ||
      typeof row.enabled !== "boolean" ||
      !integer(row.baselinesComplete) ||
      row.baselinesComplete > 2 ||
      !Array.isArray(row.errorCodes) ||
      !row.errorCodes.every((code) => typeof code === "string")
    )
      throw new Error("SPECIAL_RESPONSE_INVALID");
    return row as unknown as SpecialSnapshot["authors"][number];
  });
  const keys = new Set<string>();
  const updates = result.updates.map((value) => {
    const row = record(value),
      rawWork = record(row.work);
    if (rawWork.source !== "JM" && rawWork.source !== "Pica")
      throw new Error("SPECIAL_RESPONSE_INVALID");
    const work = validateSourceWork(rawWork, rawWork.source);
    const key = work.source + ":" + work.workId;
    if (
      keys.has(key) ||
      !Array.isArray(row.authors) ||
      !row.authors.every((name) => typeof name === "string") ||
      !integer(row.discoveredAt) ||
      !nullableTime(row.readAt)
    )
      throw new Error("SPECIAL_RESPONSE_INVALID");
    keys.add(key);
    return {
      work,
      authors: row.authors as string[],
      discoveredAt: row.discoveredAt,
      readAt: row.readAt as number | null,
    };
  });
  return { scopes, authors, updates, run: validateSpecialRun(result.run) };
}
export function specialRunMessage(run: SpecialRun | undefined): string {
  if (!run) return "尚未读取特别关注状态。";
  switch (run.phase) {
    case "waiting":
      return "正在等待现有作者检查；相同范围将复用，不重复扫描。";
    case "checking":
      return "正在检查特别关注作者，可继续浏览。";
    case "complete":
      return run.newCount
        ? `特别关注检查完成，新收录 ${run.newCount} 部作品。`
        : "特别关注检查完成，没有新作品。";
    case "partial":
      return `特别关注检查未完整完成；已发现 ${run.newCount} 部，既有未读保留。`;
    case "cancelled":
      return "已停止本次检查，既有未读保留。";
    case "unavailable":
      return "账号尚未连接或会话已变化，请连接后重试。";
    case "error":
      return "特别关注检查失败，请重试；既有未读保留。";
    default:
      return "在关注作者名单中设为特别关注，首次建立基线后开始记录新作品。";
  }
}
