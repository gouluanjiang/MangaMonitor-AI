import type { LibraryItem, LibraryReference } from "./library-types.ts";

export const libraryReferences = (item: LibraryItem): LibraryReference[] => [
  ...(item.sourceRef ? [item.sourceRef] : []),
  ...(item.links ?? []).map((link) => link.reference),
];
export const fileNeedsReview = (item: LibraryItem) =>
  item.state !== "indexed" ||
  item.errorCode !== null ||
  !(item.pageCount && item.pageCount > 0);
export type LibraryFilter = "all" | "owned" | "review";
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
    (filter === "review" ? fileNeedsReview(item) : !fileNeedsReview(item))
  );
}
export const libraryFilterLabels: Record<LibraryFilter, string> = {
  all: "全部",
  owned: "文件可用",
  review: "文件待核对",
};
