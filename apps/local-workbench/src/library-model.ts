import type { LibraryItem, LibraryReference } from "./library-types.ts";
import type { Source } from "./source-types.ts";
import { sortByWorkDate } from "./work-dates.ts";
import {
  libraryReferences,
  libraryFilterMatches,
  compareAdded,
} from "./library-matching.ts";
import type { LibraryFilter, LibrarySort } from "./library-matching.ts";

export const normalizeLibraryText = (value: string) =>
  value.normalize("NFKC").toLocaleLowerCase().trim();
export function parseLibraryReference(
  source: Source,
  input: string,
): LibraryReference | null {
  const value = input.trim();
  const workId =
    source === "JM" ? value.replace(/^JM/i, "") : value.toLowerCase();
  if (
    source === "JM"
      ? !/^[1-9]\d{0,19}$/.test(workId)
      : source !== "Pica" || !/^[a-f0-9]{24}$/.test(workId)
  )
    return null;
  return { source, workId };
}
/** Match once and count without sorting; the view sorts only its chosen filter. */
export function searchLibraryItems(items: LibraryItem[], query: string) {
  const terms = normalizeLibraryText(query).split(/\s+/).filter(Boolean);
  const matches: LibraryItem[] = [];
  const counts: Record<LibraryFilter, number> = { all: 0, owned: 0, review: 0 };
  for (const item of items) {
    if (terms.length) {
      const text = normalizeLibraryText(
        [
          item.title,
          item.fileName,
          ...item.authors,
          ...item.tags,
          item.sourceRef?.source ?? "",
          item.sourceRef?.workId ?? "",
          ...libraryReferences(item).map(
            (reference) => reference.source + " " + reference.workId,
          ),
        ].join(" "),
      );
      if (!terms.every((term) => text.includes(term))) continue;
    }
    matches.push(item);
    counts.all++;
    counts[libraryFilterMatches(item, "owned") ? "owned" : "review"]++;
  }
  return { items: matches, counts };
}

export function sortLibraryItems(
  items: LibraryItem[],
  sort: LibrarySort,
): LibraryItem[] {
  if (sort === "updated-asc" || sort === "updated-desc")
    return sortByWorkDate(items, (item) => item.versionUpdatedAt, sort);
  // Normalize each title once, rather than for every comparison.
  return items
    .map((item) => ({ item, title: normalizeLibraryText(item.title) }))
    .sort(
      (a, b) =>
        compareAdded(a.item, b.item, sort) ||
        (sort === "modified"
          ? (b.item.modifiedAt ?? 0) - (a.item.modifiedAt ?? 0)
          : 0) ||
        a.title.localeCompare(b.title) ||
        a.item.id.localeCompare(b.item.id),
    )
    .map(({ item }) => item);
}

export function filterLibraryItems(
  items: LibraryItem[],
  query: string,
  sort: LibrarySort = "title",
  filter: LibraryFilter = "all",
): LibraryItem[] {
  return sortLibraryItems(
    searchLibraryItems(items, query).items.filter((item) =>
      libraryFilterMatches(item, filter),
    ),
    sort,
  );
}
