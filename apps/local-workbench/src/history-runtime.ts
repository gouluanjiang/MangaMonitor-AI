import { invokeDesktop } from "./runtime.ts";
import type { Source } from "./source-types.ts";
export type HistoryIdentity =
  | { kind: "source"; source: Source; workId: string }
  | { kind: "library"; rootId: string; entryId: string };
export type HistoryEntry = {
  identity: HistoryIdentity;
  title: string;
  visitedAt: number;
};
export type ViewingHistory = { enabled: boolean; entries: HistoryEntry[] };
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object";
export function validateHistory(raw: unknown): ViewingHistory {
  if (!object(raw) || !object(raw.value)) throw new Error("HISTORY_INVALID");
  const value = raw.value;
  if (
    value.version !== 1 ||
    typeof value.enabled !== "boolean" ||
    !Array.isArray(value.entries) ||
    value.entries.length > 100
  )
    throw new Error("HISTORY_INVALID");
  const seen = new Set<string>();
  const entries = value.entries.map((row): HistoryEntry => {
    if (
      !object(row) ||
      !object(row.identity) ||
      typeof row.title !== "string" ||
      row.title.length > 4096 ||
      typeof row.visitedAt !== "number" ||
      !Number.isSafeInteger(row.visitedAt) ||
      row.visitedAt < 0
    )
      throw new Error("HISTORY_INVALID");
    const item = row.identity;
    let identity: HistoryIdentity;
    if (
      item.kind === "source" &&
      (item.source === "JM" || item.source === "Pica") &&
      typeof item.workId === "string" &&
      /^[A-Za-z0-9_-]{1,128}$/.test(item.workId)
    )
      identity = { kind: "source", source: item.source, workId: item.workId };
    else if (
      item.kind === "library" &&
      typeof item.rootId === "string" &&
      /^[a-f0-9]{64}$/.test(item.rootId) &&
      typeof item.entryId === "string" &&
      /^[a-f0-9]{64}$/.test(item.entryId)
    )
      identity = {
        kind: "library",
        rootId: item.rootId,
        entryId: item.entryId,
      };
    else throw new Error("HISTORY_INVALID");
    const key = JSON.stringify(identity);
    if (seen.has(key)) throw new Error("HISTORY_INVALID");
    seen.add(key);
    return { identity, title: row.title, visitedAt: row.visitedAt };
  });
  return { enabled: value.enabled, entries };
}
export async function historyCall(
  command: string,
  args?: Record<string, unknown>,
) {
  return validateHistory(await invokeDesktop<unknown>(command, args));
}
