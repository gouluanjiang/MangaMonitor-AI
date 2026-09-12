import type { CatalogSnapshot, SourceWork } from "./source-types.ts";
export const SOURCE_MEMORY_BYTES = 32 * 1024 * 1024;
const sizes = new WeakMap<object, number>();
export function jsonBytes(value: object): number {
  const known = sizes.get(value);
  if (known !== undefined) return known;
  const bytes = new TextEncoder().encode(JSON.stringify(value)).byteLength;
  sizes.set(value, bytes);
  return bytes;
}
export function compactWork(work: SourceWork): SourceWork {
  return work.description === null && work.tags.length === 0
    ? work
    : { ...work, description: null, tags: [] };
}
export function catalogBytes(snapshot: CatalogSnapshot): number {
  return (
    jsonBytes({ ...snapshot, items: [] }) +
    snapshot.items.reduce((sum, item) => sum + jsonBytes(item), 0) +
    Math.max(0, snapshot.items.length - 1)
  );
}
export function boundSourceCache<T extends object>(
  entries: Record<string, T>,
): Record<string, T> {
  const kept: [string, T][] = [];
  let bytes = 2;
  for (const entry of Object.entries(entries).reverse()) {
    const size =
      jsonBytes(entry[1]) +
      new TextEncoder().encode(JSON.stringify(entry[0])).byteLength +
      2;
    if (kept.length < 20000 && bytes + size <= SOURCE_MEMORY_BYTES) {
      kept.push(entry);
      bytes += size;
    }
  }
  return Object.fromEntries(kept.reverse());
}

/** Normalized immutable metadata DTOs, including unknown values represented as null. */
export function sameSourceWork(a: SourceWork, b: SourceWork): boolean {
  return (
    a.source === b.source &&
    a.workId === b.workId &&
    a.title === b.title &&
    a.description === b.description &&
    a.favorite === b.favorite &&
    a.chapterCount === b.chapterCount &&
    a.pageCount === b.pageCount &&
    a.coverAvailable === b.coverAvailable &&
    a.authors.length === b.authors.length &&
    a.authors.every((author, i) => author === b.authors[i]) &&
    a.tags.length === b.tags.length &&
    a.tags.every((tag, i) => tag === b.tags[i])
  );
}
