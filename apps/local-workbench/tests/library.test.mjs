import test from "node:test";
import assert from "node:assert/strict";
import {
  filterLibraryItems,
  matchLibraryWork,
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
import {
  phoneNameKey,
  phoneLibraryRows,
  phoneStatusForItem,
  inventoryForWork,
} from "../src/phone-library-model.ts";
import {
  createPhoneLibraryAdapter,
  validatePhoneLibrarySnapshot,
} from "../src/phone-library-runtime.ts";

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
  pageCount: null,
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

test("matching distinguishes an explicit source identity from an ambiguous title candidate", () => {
  const work = { source: "JM", workId: "123", title: "合成作品" };
  const candidate = item(1, { title: "合成作品" });
  const sameTitle = item(2, { title: "合成作品" });
  assert.equal(matchLibraryWork(empty(), work).kind, "unconfigured");
  const found = matchLibraryWork(snapshot([candidate, sameTitle]), work);
  assert.equal(found.kind, "candidate");
  assert.equal(found.items.length, 2);
  assert.equal(
    found.items.every((entry) => entry.sourceRef === null),
    true,
  );
  const mapped = item(3, {
    title: "标题可以不同",
    sourceRef: { source: "JM", workId: "123" },
    identityEvidence: "manual",
  });
  assert.deepEqual(
    matchLibraryWork(snapshot([candidate, mapped]), work).items.map(
      (entry) => entry.id,
    ),
    [entryId(3)],
  );
  assert.equal(matchLibraryWork(snapshot([mapped]), work).kind, "exact");
  assert.equal(
    matchLibraryWork(snapshot([mapped]), {
      ...work,
      source: "Pica",
      workId: picaId,
    }).kind,
    "missing",
  );
  assert.equal(
    matchLibraryWork(
      snapshot([], { phase: "paused", freshness: "cached" }),
      work,
    ).kind,
    "incomplete",
  );
  assert.equal(matchLibraryWork(snapshot([]), work).kind, "missing");
  assert.equal(
    matchLibraryWork(
      snapshot([item(4, { title: "合成作品 [另一翻译]" })]),
      work,
    ).kind,
    "missing",
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
      item(1),
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
  await adapter.link(rootId, 1, entryId(1), {
    source: "JM",
    workId: "123",
    path: "C:\\Not sent.zip",
  });
  await assert.rejects(adapter.cover(rootId, 1, entryId(1)), LibraryError);
  assert.deepEqual(
    calls.map((call) => call.command),
    [
      "library_read",
      "library_choose",
      "library_scan",
      "library_link",
      "library_cover",
    ],
  );
  assert.deepEqual(calls[2].args, { rootId, generation: 1, action: "pause" });
  assert.deepEqual(calls[3].args, {
    rootId,
    generation: 1,
    entryId: entryId(1),
    reference: { source: "JM", workId: "123" },
  });
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

test("manual mappings persist across a fresh controller without authorizing source or file writes", async () => {
  let saved = snapshot([item(1)]);
  const calls = [];
  const adapter = {
    read: async () => clone({ ...saved, freshness: "cached" }),
    link: async (scope, generation, id, reference) => {
      calls.push({ scope, generation, id, reference });
      saved = snapshot(
        [
          item(1, {
            sourceRef: reference,
            identityEvidence: reference ? "manual" : null,
          }),
        ],
        { revision: saved.revision + 1 },
      );
      return clone(saved);
    },
  };
  const controller = new LibraryController(adapter);
  await controller.read();
  await controller.link(entryId(1), { source: "JM", workId: "123" });
  controller.dispose();
  const reopened = new LibraryController(adapter);
  await reopened.read();
  assert.equal(
    matchLibraryWork(reopened.getState().snapshot, {
      source: "JM",
      workId: "123",
      title: "标题无须相同",
    }).kind,
    "exact",
  );
  await reopened.link(entryId(1), null);
  assert.equal(reopened.getState().snapshot.items[0].sourceRef, null);
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0], {
    scope: rootId,
    generation: 1,
    id: entryId(1),
    reference: { source: "JM", workId: "123" },
  });
  reopened.dispose();
});

const phone = (overrides = {}) => ({
  revision: 1,
  importedNames: [],
  importedAt: null,
  importFileName: null,
  manualEntries: [],
  ...overrides,
});
const manual = (number, overrides = {}) => ({
  id: entryId(number),
  name: "合成手机作品 " + number,
  reference: null,
  markedAt: 1800000000000,
  ...overrides,
});

test("phone evidence removes one archive suffix and normalizes NFC without erasing case or version", () => {
  assert.equal(
    phoneNameKey("  [合成作者] Cafe\u0301 A [翻译甲].ZIP  "),
    "[合成作者] Café A [翻译甲]",
  );
  assert.equal(phoneNameKey("合成.zip.rar"), "合成.zip");
  assert.notEqual(phoneNameKey("合成 Ａ.zip"), phoneNameKey("合成 A.zip"));
  assert.notEqual(phoneNameKey("合成 A.zip"), phoneNameKey("合成 a.zip"));
  assert.notEqual(
    phoneNameKey("合成 [翻译甲].zip"),
    phoneNameKey("合成 [翻译乙].zip"),
  );
  const directory = item(1, {
    format: "directory",
    fileName: "[合成作者] Cafe\u0301 A [翻译甲]",
    relativePath: "[合成作者] Cafe\u0301 A [翻译甲]",
  });
  assert.equal(
    validateLibrarySnapshot(snapshot([directory])).items[0].format,
    "directory",
  );
  assert.equal(phoneStatusForItem(phone(), directory), "downloaded");
  assert.equal(
    phoneStatusForItem(
      phone({
        importedNames: ["[合成作者] Café A [翻译甲].zip"],
        importedAt: 1800000000000,
        importFileName: "合成手机名单.txt",
      }),
      directory,
    ),
    "owned",
  );
  assert.equal(
    phoneStatusForItem(
      phone({
        importedNames: ["[合成作者] Café A [翻译乙].zip"],
        importedAt: 1800000000000,
        importFileName: "合成手机名单.txt",
      }),
      directory,
    ),
    "downloaded",
  );
});

test("2833 phone names are browsable without a PC root and manual/imported overlap remains reversible", () => {
  const names = Array.from(
    { length: 2833 },
    (_, i) => `合成手机作品 ${i + 1}.zip`,
  );
  const value = phone({
    importedNames: names,
    importedAt: 1800000000000,
    importFileName: "合成手机名单.txt",
    manualEntries: [manual(1)],
  });
  const rows = phoneLibraryRows(validatePhoneLibrarySnapshot(value));
  assert.equal(rows.length, 2833);
  assert.equal(new Set(rows.map((row) => row.id)).size, 2833);
  const overlap = rows.find(
    (row) => phoneNameKey(row.name) === "合成手机作品 1",
  );
  assert.equal(overlap.imported, true);
  assert.equal(overlap.manualEntries.length, 1);
  assert.equal(phoneLibraryRows({ ...value, manualEntries: [] }).length, 2833);
  assert.equal(
    inventoryForWork(empty(), value, {
      source: "JM",
      workId: "123",
      title: "合成手机作品 1",
    }).kind,
    "candidate",
    "unlinked phone title alone is not an exact platform identity",
  );
});

test("phone presence wins over PC presence only for exact name or explicit platform evidence", () => {
  const work = { source: "JM", workId: "123", title: "合成来源标题" };
  const local = item(1, {
    format: "directory",
    fileName: "合成完整文件名 [翻译甲]",
    relativePath: "合成完整文件名 [翻译甲]",
    title: work.title,
    sourceRef: { source: "JM", workId: "123" },
    identityEvidence: "manual",
  });
  const pc = snapshot([local]);
  const onPhone = phone({
    importedNames: ["合成完整文件名 [翻译甲].zip"],
    importedAt: 1800000000000,
    importFileName: "合成手机名单.txt",
  });
  assert.equal(inventoryForWork(pc, phone(), work).kind, "downloaded");
  assert.equal(inventoryForWork(pc, onPhone, work).kind, "owned");
  const onlyCandidate = { ...local, sourceRef: null, identityEvidence: null };
  assert.equal(
    inventoryForWork(snapshot([onlyCandidate]), onPhone, work).kind,
    "candidate",
  );
  const explicitPhone = phone({
    manualEntries: [
      manual(2, {
        name: "不同手机标题",
        reference: { source: "JM", workId: "123" },
      }),
    ],
  });
  assert.equal(inventoryForWork(empty(), explicitPhone, work).kind, "owned");
  assert.notEqual(
    inventoryForWork(empty(), explicitPhone, {
      ...work,
      source: "Pica",
      workId: picaId,
    }).kind,
    "owned",
  );
  assert.equal(
    inventoryForWork(
      snapshot([], { phase: "error", errorCode: "LIBRARY_UNAVAILABLE" }),
      phone(),
      work,
    ).kind,
    "incomplete",
  );
  assert.equal(pc.items.length, 1);
  assert.equal(
    pc.items[0].format,
    "directory",
    "phone ownership never removes or rewrites PC entries",
  );
});

test("an explicit phone mark never lends its title to a different source or work ID", () => {
  const sameName = "合成同名作品 [翻译甲]";
  const marked = phone({
    manualEntries: [
      manual(1, {
        name: sameName + ".zip",
        reference: { source: "JM", workId: "123" },
      }),
    ],
  });
  const jm = item(1, {
    fileName: sameName + ".zip",
    title: sameName,
    sourceRef: { source: "JM", workId: "123" },
    identityEvidence: "manual",
  });
  const anotherJM = item(2, {
    fileName: sameName + ".cbz",
    title: sameName,
    sourceRef: { source: "JM", workId: "456" },
    identityEvidence: "manual",
  });
  const pica = item(3, {
    fileName: sameName + ".rar",
    title: sameName,
    sourceRef: { source: "Pica", workId: picaId },
    identityEvidence: "manual",
  });
  const unlinked = item(4, {
    fileName: sameName,
    title: sameName,
    format: "directory",
  });
  assert.equal(phoneStatusForItem(marked, jm), "owned");
  for (const entry of [anotherJM, pica, unlinked])
    assert.equal(phoneStatusForItem(marked, entry), "downloaded");
  const pc = snapshot([jm, anotherJM, pica, unlinked]);
  assert.equal(
    inventoryForWork(pc, marked, {
      source: "Pica",
      workId: picaId,
      title: sameName,
    }).kind,
    "downloaded",
  );
  assert.equal(
    inventoryForWork(pc, marked, {
      source: "JM",
      workId: "456",
      title: sameName,
    }).kind,
    "downloaded",
  );
  const independentNameEvidence = {
    ...marked,
    importedNames: [sameName + ".zip"],
    importedAt: 1800000000000,
    importFileName: "合成手机名单.txt",
  };
  assert.equal(
    phoneStatusForItem(independentNameEvidence, pica),
    "owned",
    "a separate exact imported filename remains legitimate name evidence",
  );
  const nameOnly = phone({
    manualEntries: [manual(2, { name: sameName + ".zip" })],
  });
  assert.equal(phoneStatusForItem(nameOnly, unlinked), "owned");
});

test("manual phone records retain the raw full name and strip only one suffix for comparison", () => {
  const marked = phone({
    manualEntries: [manual(1, { name: "合成作品.zip.rar" })],
  });
  const rows = phoneLibraryRows(validatePhoneLibrarySnapshot(marked));
  assert.equal(rows[0].name, "合成作品.zip.rar");
  assert.equal(
    phoneStatusForItem(marked, item(1, { fileName: "合成作品.zip.cbz" })),
    "owned",
  );
  assert.equal(
    phoneStatusForItem(marked, item(2, { fileName: "合成作品.zip" })),
    "downloaded",
  );
  assert.equal(
    phoneStatusForItem(marked, item(3, { fileName: "合成作品" })),
    "downloaded",
  );
  assert.equal(
    phoneLibraryRows({
      ...marked,
      importedNames: ["合成作品.zip.cbz"],
      importedAt: 1800000000000,
      importFileName: "合成手机名单.txt",
    }).length,
    1,
  );
});

test("phone import uses a native picker and replaces imported names while preserving manual marks", async () => {
  let saved = phone({
    importedNames: ["合成旧名单.zip"],
    importedAt: 1800000000000,
    importFileName: "旧合成名单.txt",
    manualEntries: [manual(1)],
  });
  const calls = [];
  let canceled = true;
  const adapter = createPhoneLibraryAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "phone_library_read") return clone(saved);
      if (command === "phone_library_import") {
        if (canceled) return null;
        saved = {
          ...saved,
          revision: saved.revision + 1,
          importedNames: ["合成新名单.rar"],
          importFileName: "新合成名单.txt",
        };
        return clone(saved);
      }
      if (command === "phone_library_unmark") {
        saved = { ...saved, revision: saved.revision + 1, manualEntries: [] };
        return clone(saved);
      }
      throw Error("unexpected command");
    },
  });
  await adapter.read();
  assert.equal(await adapter.import(1), null);
  canceled = false;
  const imported = await adapter.import(1);
  assert.deepEqual(imported.importedNames, ["合成新名单.rar"]);
  assert.deepEqual(imported.manualEntries, [manual(1)]);
  const unmarked = await adapter.unmark(imported.revision, entryId(1));
  assert.deepEqual(unmarked.importedNames, ["合成新名单.rar"]);
  assert.equal(unmarked.manualEntries.length, 0);
  assert.deepEqual(calls[2], {
    command: "phone_library_import",
    args: { revision: 1 },
  });
  assert.equal(
    calls.every((call) => call.command.startsWith("phone_library_")),
    true,
  );
  assert.equal(
    calls.some((call) => JSON.stringify(call.args ?? {}).includes("path")),
    false,
  );
});

test("phone snapshots reject paths and duplicate manual IDs without weakening previous inventory", () => {
  const valid = phone({ manualEntries: [manual(1)] });
  for (const invalid of [
    { ...valid, revision: -1 },
    {
      ...valid,
      importedNames: ["C:\\private\\archive.zip"],
      importedAt: 1800000000000,
      importFileName: "synthetic.txt",
    },
    {
      ...valid,
      importedNames: ["C:archive.zip"],
      importedAt: 1800000000000,
      importFileName: "synthetic.txt",
    },
    { ...valid, manualEntries: [manual(1), manual(1)] },
    {
      ...valid,
      manualEntries: [
        manual(1, { reference: { source: "Pica", workId: "123" } }),
      ],
    },
  ])
    assert.throws(() => validatePhoneLibrarySnapshot(invalid));
  assert.deepEqual(valid.manualEntries, [manual(1)]);
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
