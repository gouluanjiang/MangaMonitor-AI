import test from "node:test";
import assert from "node:assert/strict";
import {
  filterLibraryItems,
  normalizeLibraryText,
  parseLibraryReference,
} from "../src/library-model.ts";
import {
  createLibraryAdapter,
  LibraryController,
  LibraryError,
  validateLibrarySnapshot,
} from "../src/library-runtime.ts";
import { LibraryCoverCache } from "../src/library-cover-cache.ts";

// All names, IDs, paths and IPC records in this file are synthetic.
// No archive, user directory, website, credential or production inventory is used.
const rootId = "a".repeat(64);
const otherRoot = "b".repeat(64);
const picaId = "0123456789abcdef01234567";
const entryId = (number) => number.toString(16).padStart(64, "0");
const item = (number, overrides = {}) => ({
  id: entryId(number),
  relativePath: `合成目录/合成作品 ${String(number).padStart(4, "0")}.zip`,
  fileName: `合成作品 ${String(number).padStart(4, "0")}.zip`,
  format: "zip",
  title: `合成作品 ${String(number).padStart(4, "0")}`,
  authors: ["合成作者"],
  description: null,
  tags: [],
  bytes: 1024,
  modifiedAt: 1800000000000,
  pageCount: 20,
  coverAvailable: false,
  state: "indexed",
  errorCode: null,
  sourceRef: null,
  identityEvidence: null,
  ...overrides,
});
const snapshot = (items = [], overrides = {}) => ({
  revision: 1,
  rootId,
  rootPath: "C:\\Synthetic library",
  generation: 1,
  phase: "complete",
  freshness: "live",
  items,
  visited: items.length,
  skipped: 0,
  updatedAt: 1800000000000,
  errorCode: null,
  ...overrides,
});
const empty = () => ({
  revision: 0,
  rootId: null,
  rootPath: null,
  generation: 0,
  phase: "idle",
  freshness: "none",
  items: [],
  visited: 0,
  skipped: 0,
  updatedAt: null,
  errorCode: null,
});
const clone = (value) => structuredClone(value);

test("admission sorting keeps unknown history last and keeps only actual file filters", () => {
  const entries = [
    item(1, { addedAt: null }),
    item(2, {
      addedAt: 200,
      sourceRef: { source: "JM", workId: "2" },
      identityEvidence: "metadata",
    }),
    item(3, { addedAt: 100 }),
    item(4, {
      addedAt: 300,
      state: "unreadable",
      errorCode: "LIBRARY_FILE_CHANGED",
    }),
  ];
  assert.deepEqual(
    filterLibraryItems(entries, "", "added-desc").map((v) => v.id),
    [entryId(4), entryId(2), entryId(3), entryId(1)],
  );
  assert.deepEqual(
    filterLibraryItems(entries, "", "added-asc").map((v) => v.id),
    [entryId(3), entryId(2), entryId(4), entryId(1)],
  );
  assert.deepEqual(
    filterLibraryItems(entries, "", "title", "review").map((v) => v.id),
    [entryId(4)],
  );
  assert.deepEqual(
    filterLibraryItems(entries, "", "title", "owned").map((v) => v.id),
    [entryId(1), entryId(2), entryId(3)],
  );
});

test("2833 synthetic names remain individually addressable after Unicode search and sorting", () => {
  const items = Array.from({ length: 2833 }, (_, i) => item(i + 1));
  items[0] = item(1, {
    title: "[合成作者] Cafe\u0301 Ａ巻 [翻译甲]",
    fileName: "[合成作者] Cafe\u0301 Ａ巻 [翻译甲].zip",
  });
  items[1] = item(2, {
    title: "[合成作者] Café A巻 [翻译甲]",
    fileName: "[合成作者] Café A巻 [翻译甲].zip",
  });
  items[2] = item(3, {
    title: "[合成作者] Café A巻 [翻译乙]",
    fileName: "[合成作者] Café A巻 [翻译乙].zip",
  });
  assert.equal(normalizeLibraryText("  Cafe\u0301 Ａ  "), "café a");
  const all = filterLibraryItems(items, "", "title");
  assert.equal(all.length, 2833);
  assert.equal(new Set(all.map((entry) => entry.id)).size, 2833);
  assert.deepEqual(
    new Set(
      filterLibraryItems(items, "Café A巻 [翻译甲]").map((entry) => entry.id),
    ),
    new Set([entryId(1), entryId(2)]),
  );
  assert.equal(filterLibraryItems(items, "翻译乙").length, 1);
  assert.equal(filterLibraryItems(items, "合成作者").length, 2833);
  assert.equal(
    items[0].title.includes("\u0301"),
    true,
    "search must not rewrite stored archive names",
  );
});

test("manual references accept platform IDs and reject paths, URLs and cross-platform formats", () => {
  assert.deepEqual(parseLibraryReference("JM", "JM413751"), {
    source: "JM",
    workId: "413751",
  });
  assert.deepEqual(parseLibraryReference("JM", "413751"), {
    source: "JM",
    workId: "413751",
  });
  assert.deepEqual(parseLibraryReference("JM", "12345678901234567890"), {
    source: "JM",
    workId: "12345678901234567890",
  });
  assert.deepEqual(parseLibraryReference("Pica", picaId), {
    source: "Pica",
    workId: picaId,
  });
  for (const value of [
    "",
    "0",
    "000123",
    "123456789012345678901",
    "../123",
    "C:\\123.zip",
    "https://example.invalid/123",
    "123 456",
    picaId,
  ])
    assert.equal(parseLibraryReference("JM", value), null);
  for (const value of ["123", "JM413751", "../" + picaId, picaId + "g"])
    assert.equal(parseLibraryReference("Pica", value), null);
});

test("cached unsupported and corrupt records remain visible while malformed IPC snapshots reject", () => {
  const valid = snapshot(
    [
      item(1, { pageCount: null }),
      item(2, {
        format: "rar",
        fileName: "合成不支持.rar",
        relativePath: "合成不支持.rar",
        state: "unsupported",
        errorCode: "LIBRARY_FORMAT_UNSUPPORTED",
      }),
      item(3, { state: "unreadable", errorCode: "LIBRARY_ARCHIVE_INVALID" }),
    ],
    { freshness: "cached" },
  );
  const restored = validateLibrarySnapshot(valid);
  assert.equal(restored.items.length, 3);
  assert.equal(restored.items[0].pageCount, null);
  assert.equal(restored.items[1].state, "unsupported");
  assert.equal(restored.items[2].state, "unreadable");
  for (const invalid of [
    { ...valid, generation: -1 },
    { ...valid, rootId: "../arbitrary" },
    { ...valid, items: [item(1), item(1)] },
    { ...valid, items: [item(1, { relativePath: "../escape.zip" })] },
    { ...valid, items: [item(1, { bytes: -1 })] },
    {
      ...valid,
      items: [
        item(1, {
          sourceRef: { source: "Pica", workId: "123" },
          identityEvidence: "manual",
        }),
      ],
    },
  ])
    assert.throws(() => validateLibrarySnapshot(invalid), LibraryError);
});

test("library adapter sends opaque scope and IDs only, and rejects stale cover responses", async () => {
  const calls = [];
  const value = snapshot([item(1)]);
  const adapter = createLibraryAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "library_choose") return null;
      if (command === "library_cover")
        return {
          rootId: otherRoot,
          generation: 1,
          entryId: entryId(1),
          dataUrl: null,
        };
      return clone(value);
    },
  });
  await adapter.read();
  assert.equal(await adapter.choose(), null);
  await adapter.scan(rootId, 1, "pause");
  await assert.rejects(adapter.cover(rootId, 1, entryId(1)), LibraryError);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["library_read", "library_choose", "library_scan", "library_cover"],
  );
  assert.deepEqual(calls[2].args, { rootId, generation: 1, action: "pause" });
  const before = calls.length;
  await assert.rejects(
    adapter.cover("../arbitrary", 1, entryId(1)),
    LibraryError,
  );
  await assert.rejects(adapter.scan(rootId, -1, "next"), LibraryError);
  assert.equal(
    calls.length,
    before,
    "invalid renderer input must not be forwarded",
  );
});

test("browser library mode never invokes native folder selection or file access", async () => {
  let calls = 0;
  const adapter = createLibraryAdapter({
    native: false,
    invoke: async () => {
      calls++;
    },
  });
  await assert.rejects(adapter.choose(), LibraryError);
  await assert.rejects(adapter.cover(rootId, 1, entryId(1)), LibraryError);
  assert.equal(calls, 0);
});

test("restoring a saved catalog never starts a filesystem scan and canceled choice preserves it", async () => {
  const calls = [];
  const saved = snapshot([item(1)], { freshness: "cached", phase: "paused" });
  const controller = new LibraryController({
    read: async () => {
      calls.push("read");
      return clone(saved);
    },
    choose: async () => {
      calls.push("choose");
      return null;
    },
    scan: async () => {
      calls.push("scan");
      throw Error("unexpected scan");
    },
  });
  await controller.read();
  assert.equal(controller.getState().snapshot.items.length, 1);
  await controller.choose();
  assert.equal(controller.getState().snapshot.rootId, rootId);
  assert.deepEqual(calls, ["read", "choose"]);
  controller.dispose();
});

test("a scan response from another root or generation cannot replace the current catalog", async () => {
  let response = snapshot([item(2)], { rootId: otherRoot });
  const controller = new LibraryController({
    read: async () => snapshot([item(1)]),
    scan: async () => response,
  });
  await controller.read();
  await controller.scan("start");
  assert.equal(controller.getState().snapshot.rootId, rootId);
  assert.ok(controller.getState().error);
  response = snapshot([item(2)], { generation: 1 });
  await controller.scan("start");
  assert.equal(controller.getState().snapshot.generation, 1);
  assert.ok(controller.getState().error);
  assert.deepEqual(
    controller.getState().snapshot.items.map((entry) => entry.id),
    [entryId(1)],
  );
  controller.dispose();
});

test("successful compressed covers are reused in the same run and cannot leak into a new root generation", async () => {
  const cache = new LibraryCoverCache(1024);
  cache.setScope(rootId, 1);
  let loads = 0;
  const load = async () => {
    loads++;
    return {
      rootId,
      generation: 1,
      entryId: entryId(1),
      dataUrl: "data:image/jpeg;base64,/9j/2Q==",
    };
  };
  const first = cache.acquire(rootId, 1, entryId(1), load);
  const result = await first.promise;
  assert.equal(result.status, "ready");
  first.release();
  const second = cache.acquire(rootId, 1, entryId(1), load);
  assert.equal((await second.promise).url, result.url);
  assert.equal(loads, 1);
  second.release();
  cache.setScope(rootId, 2);
  assert.equal(cache.peek(rootId, 1, entryId(1)), undefined);
  let release;
  const late = cache.acquire(
    rootId,
    2,
    entryId(1),
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  cache.setScope(otherRoot, 3);
  release({
    rootId,
    generation: 2,
    entryId: entryId(1),
    dataUrl: "data:image/jpeg;base64,/9j/2Q==",
  });
  assert.equal((await late.promise).status, "cancelled");
  assert.equal(cache.peek(otherRoot, 3, entryId(1)), undefined);
  late.release();
  cache.clear();
});
