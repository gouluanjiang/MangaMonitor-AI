export type UpdatedSort = "updated-desc" | "updated-asc" | "source";
export const updatedSorts = ["updated-desc", "updated-asc", "source"] as const;

/** Date-only values keep their precision; IPC dates never accept local-time strings. */
export function normalizedWorkDate(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const day = /^\d{4}-\d{2}-\d{2}$/.test(value);
  if (
    !day &&
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(
      value,
    )
  )
    return null;
  if (
    !day &&
    (Number(value.slice(11, 13)) > 23 ||
      Number(value.slice(14, 16)) > 59 ||
      Number(value.slice(17, 19)) > 59)
  )
    return null;
  const time = Date.parse(value);
  if (!Number.isFinite(time)) return null;
  // Old generated packages used the epoch as a compatibility placeholder.
  if (value.startsWith("1970-01-01")) return null;
  const [year, month, date] = value.slice(0, 10).split("-").map(Number);
  if (year < 1900 || year > 9999) return null;
  const check = new Date(Date.UTC(year, month - 1, date));
  if (
    check.getUTCFullYear() !== year ||
    check.getUTCMonth() !== month - 1 ||
    check.getUTCDate() !== date
  )
    return null;
  return value;
}

export function formatWorkDate(value: unknown, full = false): string | null {
  const valid = normalizedWorkDate(value);
  if (!valid) return null;
  if (valid.length === 10) return valid;
  return formatTimestamp(Date.parse(valid), full);
}

export function formatTimestamp(
  value: number | null | undefined,
  full = false,
): string | null {
  if (value == null || !Number.isFinite(value)) return null;
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return null;
  const day = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  return full
    ? `${day} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`
    : day;
}

export function compareNullableTime(
  a: number | null,
  b: number | null,
  ascending: boolean,
): number {
  if (a === null) return b === null ? 0 : 1;
  if (b === null) return -1;
  return (a - b) * (ascending ? 1 : -1);
}

/** Decorate once so large catalogs do not parse dates inside every comparison. */
export function sortByWorkDate<T>(
  items: readonly T[],
  date: (item: T) => unknown,
  sort: UpdatedSort,
): T[] {
  if (sort === "source") return [...items];
  return items
    .map((item, index) => {
      const value = normalizedWorkDate(date(item));
      return { item, index, time: value === null ? null : Date.parse(value) };
    })
    .sort(
      (a, b) =>
        compareNullableTime(a.time, b.time, sort === "updated-asc") ||
        a.index - b.index,
    )
    .map(({ item }) => item);
}

export const sortPreferenceKey = (page: string) =>
  `mangamonitor.workbench.sort.v1.${page}`;
export function readSortPreference<T extends string>(
  page: string,
  allowed: readonly T[],
  fallback: T,
): T {
  try {
    const value = globalThis.localStorage?.getItem(sortPreferenceKey(page));
    return allowed.includes(value as T) ? (value as T) : fallback;
  } catch {
    return fallback;
  }
}
export function writeSortPreference(page: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(sortPreferenceKey(page), value);
  } catch {
    // Sorting remains usable if the webview cannot persist this preference.
  }
}
