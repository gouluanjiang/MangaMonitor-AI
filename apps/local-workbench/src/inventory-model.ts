import type { LibraryItem, LibrarySnapshot } from "./library-types.ts";
import type { SourceWork } from "./source-types.ts";
import type { SourceMatchPair } from "./source-matches-types.ts";
import { createLibraryMatcher } from "./library-model.ts";

/** Ownership comes from the current computer library, never a historical list. */
export interface InventoryMatch {
  kind:
    | "owned"
    | "candidate"
    | "missing"
    | "incomplete"
    | "unconfigured"
    | "unknown";
  items: LibraryItem[];
}
export const readableLibraryItem = (item: LibraryItem) =>
  item.state === "indexed" &&
  item.errorCode === null &&
  (item.pageCount ?? 0) > 0;
export const libraryItemStatus = (item: LibraryItem) =>
  readableLibraryItem(item) ? "已入库 · 电脑漫画库" : "文件待核对";
export function createInventoryMatcher(
  library: LibrarySnapshot | undefined,
  pairs: SourceMatchPair[] = [],
  matchesReady = true,
  libraryReady = true,
) {
  const match = createLibraryMatcher(library, matchesReady ? pairs : []);
  return (
    work: Pick<SourceWork, "source" | "workId" | "title">,
  ): InventoryMatch => {
    const result = match(work);
    if (!libraryReady || !matchesReady || library?.phase === "error")
      return { kind: "unknown", items: result.items };
    return { ...result, kind: result.kind === "exact" ? "owned" : result.kind };
  };
}
export const inventoryLabel = (match: InventoryMatch) =>
  ({
    owned: "已入库 · 电脑漫画库",
    candidate: "文件或同标题待确认",
    missing: "漫画库内未匹配",
    incomplete: "漫画库目录未读完",
    unconfigured: "尚未设置漫画库",
    unknown: "文件或关联未核对",
  })[match.kind];
