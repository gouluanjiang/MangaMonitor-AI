import type {
  LibraryItem,
  LibraryReference,
  LibrarySnapshot,
} from "./library-types.ts";
import type { Source } from "./source-types.ts";

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
  sort: "title" | "modified" = "title",
): LibraryItem[] {
  const terms = normalizeLibraryText(query).split(/\s+/).filter(Boolean);
  return items
    .filter((item) => {
      const text = normalizeLibraryText(
        [
          item.title,
          item.fileName,
          ...item.authors,
          ...item.tags,
          item.sourceRef?.source ?? "",
          item.sourceRef?.workId ?? "",
        ].join(" "),
      );
      return terms.every((term) => text.includes(term));
    })
    .sort(
      (a, b) =>
        (sort === "modified" ? (b.modifiedAt ?? 0) - (a.modifiedAt ?? 0) : 0) ||
        normalizeLibraryText(a.title).localeCompare(
          normalizeLibraryText(b.title),
        ) ||
        a.id.localeCompare(b.id),
    );
}
export interface LibraryMatch {
  kind: "unconfigured" | "exact" | "candidate" | "missing" | "incomplete";
  items: LibraryItem[];
}
type SourceIdentity = { source: Source; workId: string; title: string };
const key = (ref: LibraryReference) => ref.source + ":" + ref.workId;
export function createLibraryMatcher(snapshot: LibrarySnapshot | undefined) {
  const refs = new Map<string, LibraryItem[]>();
  const titles = new Map<string, LibraryItem[]>();
  for (const item of snapshot?.items ?? []) {
    if (item.sourceRef)
      refs.set(key(item.sourceRef), [
        ...(refs.get(key(item.sourceRef)) ?? []),
        item,
      ]);
    const title = normalizeLibraryText(item.title);
    if (title) titles.set(title, [...(titles.get(title) ?? []), item]);
  }
  return (work: SourceIdentity): LibraryMatch => {
    if (!snapshot?.rootId) return { kind: "unconfigured", items: [] };
    const exact = refs.get(key(work));
    if (exact?.length) return { kind: "exact", items: exact };
    const candidates = titles.get(normalizeLibraryText(work.title));
    if (candidates?.length) return { kind: "candidate", items: candidates };
    return {
      kind: snapshot.phase === "complete" ? "missing" : "incomplete",
      items: [],
    };
  };
}
export const matchLibraryWork = (
  snapshot: LibrarySnapshot | undefined,
  work: SourceIdentity,
) => createLibraryMatcher(snapshot)(work);
export const libraryMatchLabel = (match: LibraryMatch) =>
  ({
    unconfigured: "未选择漫画库",
    exact: "已关联本地文件",
    candidate: "同标题待确认",
    missing: "目录内未匹配",
    incomplete: "目录未读完，待核对",
  })[match.kind];
