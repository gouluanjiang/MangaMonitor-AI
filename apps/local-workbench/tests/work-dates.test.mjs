import test from "node:test";
import assert from "node:assert/strict";
import {
  formatWorkDate,
  normalizedWorkDate,
  readSortPreference,
  sortByWorkDate,
  sortPreferenceKey,
  updatedSorts,
  writeSortPreference,
} from "../src/work-dates.ts";
import { sameSourceWork } from "../src/source-memory.ts";
import { mergeSourceWorks } from "../src/source-types.ts";
import { validateSourceWork } from "../src/source-runtime.ts";

test("date values retain precision and reject ambiguous, invalid and historical placeholder metadata", () => {
  for (const value of [
    undefined,
    null,
    "",
    "1970-01-01T00:00:00Z",
    "1970-01-01",
    "2026-02-30",
    "2026-09-20 12:00:00",
    1800000000000,
    "1899-12-31",
    "2026-13-01",
    "2026-09-20T25:00:00Z",
    "2026-09-20T24:00:00Z",
    "2026-09-20T23:59:60Z",
  ])
    assert.equal(normalizedWorkDate(value), null);
  assert.equal(formatWorkDate("2026-09-20", true), "2026-09-20");
  assert.equal(normalizedWorkDate("2024-02-29"), "2024-02-29");
  const instant = "2026-09-20T12:34:56.123Z";
  assert.equal(normalizedWorkDate(instant), instant);
  const local = new Date(instant);
  assert.ok(
    formatWorkDate(instant, true).endsWith(
      `${String(local.getHours()).padStart(2, "0")}:34:56`,
    ),
  );
});

test("date sorting puts unknowns last in both directions, keeps ties stable and never changes pagination order", () => {
  const items = [
    { id: "unknown", date: null },
    { id: "old", date: "2026-09-01" },
    { id: "new-a", date: "2026-09-20T00:00:00Z" },
    { id: "new-b", date: "2026-09-20" },
    { id: "invalid", date: "not a date" },
  ];
  const before = structuredClone(items);
  assert.deepEqual(
    sortByWorkDate(items, (x) => x.date, "updated-desc").map((x) => x.id),
    ["new-a", "new-b", "old", "unknown", "invalid"],
  );
  assert.deepEqual(
    sortByWorkDate(items, (x) => x.date, "updated-asc").map((x) => x.id),
    ["old", "new-a", "new-b", "unknown", "invalid"],
  );
  assert.deepEqual(
    sortByWorkDate(items, (x) => x.date, "source"),
    before,
  );
  assert.deepEqual(items, before);
});

test("sort preferences are independent per page, whitelist values and tolerate unavailable browser storage", () => {
  const descriptor = Object.getOwnPropertyDescriptor(
    globalThis,
    "localStorage",
  );
  const values = new Map();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
    },
  });
  try {
    writeSortPreference("author-search", "updated-asc");
    assert.equal(
      readSortPreference("author-search", updatedSorts, "updated-desc"),
      "updated-asc",
    );
    assert.equal(
      readSortPreference("author-updates", updatedSorts, "updated-desc"),
      "updated-desc",
    );
    values.set(sortPreferenceKey("author-search"), "untrusted-value");
    assert.equal(
      readSortPreference("author-search", updatedSorts, "updated-desc"),
      "updated-desc",
    );
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("unavailable");
      },
    });
    assert.equal(
      readSortPreference("author-search", updatedSorts, "updated-desc"),
      "updated-desc",
    );
    assert.doesNotThrow(() => writeSortPreference("library", "added-asc"));
  } finally {
    if (descriptor)
      Object.defineProperty(globalThis, "localStorage", descriptor);
    else delete globalThis.localStorage;
  }
});

test("source metadata keeps a real work date through DTO validation and lightweight refreshes without using observation time", () => {
  const work = {
    source: "JM",
    workId: "123",
    title: "Synthetic",
    authors: ["Synthetic"],
    description: null,
    tags: [],
    favorite: null,
    chapterCount: 1,
    pageCount: 1,
    coverAvailable: false,
  };
  const dated = validateSourceWork({
    ...work,
    sourceUpdatedAt: "2026-09-20",
    observedAt: 1800000000000,
  });
  assert.equal(dated.sourceUpdatedAt, "2026-09-20");
  assert.equal(dated.observedAt, undefined);
  assert.equal(
    validateSourceWork({ ...work, sourceUpdatedAt: "not a date" })
      .sourceUpdatedAt,
    null,
  );
  assert.equal(sameSourceWork(work, dated), false);
  assert.equal(
    mergeSourceWorks([dated], [work])[0].sourceUpdatedAt,
    "2026-09-20",
  );
  assert.equal(
    mergeSourceWorks([dated], [{ ...work, sourceUpdatedAt: "2026-09-21" }])[0]
      .sourceUpdatedAt,
    "2026-09-21",
  );
});
