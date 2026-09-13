import type { LibraryItem, LibraryReference } from "./library-types.ts";

export const libraryReferences = (item: LibraryItem): LibraryReference[] => [
  ...(item.sourceRef ? [item.sourceRef] : []),
  ...(item.links ?? []).map((link) => link.reference),
];
export const matchingText = (text: string) =>
  text.normalize("NFKC").toLowerCase().replace(/\s/g, "");
/** Broad normalization only proposes candidates. It never grants ownership. */
export function candidateTitle(title: string): string {
  let value = matchingText(title).replace(/\.(zip|cbz)$/, "");
  for (;;) {
    const next = value.replace(/^\(c\d{2,3}\)/, "").replace(/^\[[^\]]*\]/, "");
    if (next === value) return value;
    value = next;
  }
}
export const fileNeedsReview = (item: LibraryItem) =>
  item.state !== "indexed" ||
  item.errorCode !== null ||
  !(item.pageCount && item.pageCount > 0);
export type LibraryFilter = "all" | "owned" | "review" | "unlinked";
export type LibrarySort = "title" | "modified" | "added-desc" | "added-asc";
export function compareAdded(
  a: LibraryItem,
  b: LibraryItem,
  sort: LibrarySort,
) {
  if (sort !== "added-asc" && sort !== "added-desc") return 0;
  if (a.addedAt == null && b.addedAt == null) return 0;
  if (a.addedAt == null) return 1;
  if (b.addedAt == null) return -1;
  return (a.addedAt - b.addedAt) * (sort === "added-asc" ? 1 : -1);
}
export function libraryFilterMatches(item: LibraryItem, filter: LibraryFilter) {
  return (
    filter === "all" ||
    (filter === "review"
      ? fileNeedsReview(item)
      : filter === "unlinked"
        ? !fileNeedsReview(item) && libraryReferences(item).length === 0
        : !fileNeedsReview(item))
  );
}
export const libraryFilterLabels: Record<LibraryFilter, string> = {
  all: "全部",
  owned: "已入库",
  review: "文件待核对",
  unlinked: "来源待关联",
};
