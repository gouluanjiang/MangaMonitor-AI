import type { LibraryItem, LibrarySnapshot } from "./library-types.ts";
import type { SourceWork } from "./source-types.ts";
import type { DownloadInventorySnapshot } from "./download-types.ts";

/** Explicit same-source download/review records plus current file presence. */
export interface InventoryMatch {
  kind: "owned" | "missing" | "unconfigured" | "unknown";
  items: LibraryItem[];
}
export const readableLibraryItem = (item: LibraryItem) =>
  item.state === "indexed" &&
  item.errorCode === null &&
  (item.pageCount ?? 0) > 0;
export const libraryItemStatus = (item: LibraryItem) =>
  readableLibraryItem(item) ? "已入库 · 电脑漫画库" : "文件待核对";
export const inventoryScopeNote =
  "按对应来源的下载记录、已核对旧库登记和实际文件统计；未登记的旧漫画可能显示“未入库”。";
export function createInventoryMatcher(
  library: LibrarySnapshot | undefined,
  downloads: DownloadInventorySnapshot | undefined,
  downloadsReady = true,
  libraryReady = true,
) {
  const entries = new Map(
    (library?.items ?? []).map((item) => [item.id, item]),
  );
  const registered = new Map(
    (downloads?.items ?? []).map((item) => [
      item.source + ":" + item.workId,
      item,
    ]),
  );
  return (
    work: Pick<SourceWork, "source" | "workId" | "title">,
  ): InventoryMatch => {
    if (!library?.rootId) return { kind: "unconfigured", items: [] };
    if (
      !libraryReady ||
      !downloadsReady ||
      !downloads ||
      downloads.rootId !== library.rootId ||
      library.phase === "error"
    )
      return { kind: "unknown", items: [] };
    const download = registered.get(work.source + ":" + work.workId);
    const item = download && entries.get(download.libraryEntryId);
    const items = item ? [item] : [];
    if (!download || download.localFiles === "missing")
      return { kind: "missing", items: [] };
    return {
      kind: download.localFiles === "present" ? "owned" : "unknown",
      items,
    };
  };
}
export const inventoryLabel = (match: InventoryMatch) =>
  ({
    owned: "已入库 · 电脑漫画库",
    missing: "未入库",
    unconfigured: "尚未设置漫画库",
    unknown: "入库状态待核实",
  })[match.kind];

export type InventoryFilter = "all" | "owned" | "missing" | "unknown";
export const inventoryFilterLabels: Record<InventoryFilter, string> = {
  all: "全部",
  owned: "已入库",
  missing: "未入库",
  unknown: "状态待核对",
};
export function inventoryFilterMatches(
  match: InventoryMatch,
  filter: InventoryFilter,
) {
  return (
    filter === "all" ||
    (filter === "unknown"
      ? ["unknown", "unconfigured"].includes(match.kind)
      : match.kind === filter)
  );
}
