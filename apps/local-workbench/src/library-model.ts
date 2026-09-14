import type { LibraryItem, LibraryReference } from "./library-types.ts";
import type { Source } from "./source-types.ts";
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
export function filterLibraryItems(
  items: LibraryItem[],
  query: string,
  sort: LibrarySort = "title",
  filter: LibraryFilter = "all",
): LibraryItem[] {
  const terms = normalizeLibraryText(query).split(/\s+/).filter(Boolean);
  return items
    .filter((item) => {
      if (!libraryFilterMatches(item, filter)) return false;
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
      return terms.every((term) => text.includes(term));
    })
    .sort(
      (a, b) =>
        compareAdded(a, b, sort) ||
        (sort === "modified" ? (b.modifiedAt ?? 0) - (a.modifiedAt ?? 0) : 0) ||
        normalizeLibraryText(a.title).localeCompare(
          normalizeLibraryText(b.title),
        ) ||
        a.id.localeCompare(b.id),
    );
}
