import test from "node:test";
import assert from "node:assert/strict";
import {
  createLibraryAdapter,
  LibraryController,
} from "../src/library-runtime.ts";
import { createInventoryMatcher } from "../src/inventory-model.ts";

// Synthetic responses only. No filesystem, shell, source or media operation.
const rootId = "a".repeat(64),
  entryId = "b".repeat(64);
const item = {
  id: entryId,
  relativePath: "Synthetic.zip",
  fileName: "Synthetic.zip",
  format: "zip",
  title: "Synthetic",
  authors: [],
  description: null,
  tags: [],
  bytes: 100,
  modifiedAt: null,
  pageCount: 2,
  coverAvailable: true,
  state: "indexed",
  errorCode: null,
  sourceRef: { source: "JM", workId: "123" },
  identityEvidence: "metadata",
};
const snapshot = (overrides = {}) => ({
  revision: 2,
  rootId,
  rootPath: "C:\\Synthetic",
  generation: 3,
  phase: "complete",
  freshness: "live",
  items: [item],
  visited: 1,
  skipped: 0,
  updatedAt: null,
  errorCode: null,
  ...overrides,
});
const outcome = (errorCode = null) => ({
  snapshot: snapshot({
    revision: 3,
    items: [
      {
        ...item,
        state: "unreadable",
        errorCode: "LIBRARY_RECYCLED",
        coverAvailable: false,
      },
    ],
  }),
  recycled: true,
  errorCode,
});

test("recycle IPC accepts IDs and revision only, cancellation is distinct, bad scope is rejected", async () => {
  const calls = [];
  let value = null;
  const adapter = createLibraryAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return value;
    },
  });
  assert.equal(await adapter.recycle(rootId, 3, entryId, 2), null);
  assert.deepEqual(calls[0], {
    command: "library_recycle",
    args: { rootId, generation: 3, entryId, expectedRevision: 2 },
  });
  await assert.rejects(adapter.recycle(rootId, 3, "../other", 2));
  assert.equal(calls.length, 1);
  value = outcome();
  assert.equal((await adapter.recycle(rootId, 3, entryId, 2)).recycled, true);
  value = { ...value, snapshot: snapshot({ rootId: "c".repeat(64) }) };
  await assert.rejects(adapter.recycle(rootId, 3, entryId, 2));
  value = { ...outcome(), snapshot: snapshot({ revision: 1 }) };
  await assert.rejects(adapter.recycle(rootId, 3, entryId, 2));
});

test("cancel and failure preserve the original library, repeated clicks cannot start a second recycle", async () => {
  let resolve;
  let calls = 0;
  const controller = new LibraryController({
    read: async () => snapshot(),
    recycle: async () => {
      calls++;
      return new Promise((r) => (resolve = r));
    },
  });
  await controller.read();
  const expected = controller.getState().snapshot;
  const pending = controller.recycle(entryId, expected);
  await assert.rejects(
    controller.recycle(entryId, expected),
    (e) => e.code === "LIBRARY_BUSY",
  );
  resolve(null);
  assert.equal(await pending, null);
  assert.equal(calls, 1);
  assert.equal(controller.getState().snapshot, expected);
  controller.adapter.recycle = async () => {
    throw { code: "LIBRARY_ITEM_BUSY" };
  };
  await assert.rejects(
    controller.recycle(entryId, expected),
    (e) => e.code === "LIBRARY_ITEM_BUSY",
  );
  assert.equal(controller.getState().snapshot, expected);
  assert.match(controller.getState().error, /正在被读取/);
  await assert.rejects(
    controller.recycle(entryId, { ...expected, revision: 1 }),
    (e) => e.code === "LIBRARY_STALE_SNAPSHOT",
  );
  controller.dispose();
});

test("success and partial registration results publish missing file state without erasing identity", async () => {
  const controller = new LibraryController({
    read: async () => snapshot(),
    recycle: async () => outcome("LIBRARY_RECYCLE_RESULT_UNCERTAIN"),
  });
  await controller.read();
  const result = await controller.recycle(
    entryId,
    controller.getState().snapshot,
  );
  assert.equal(result.recycled, true);
  assert.equal(controller.getState().snapshot.items[0].sourceRef.workId, "123");
  assert.equal(
    controller.getState().snapshot.items[0].errorCode,
    "LIBRARY_RECYCLED",
  );
  assert.match(controller.getState().error, /尚未确认/);
  const match = createInventoryMatcher(controller.getState().snapshot, {
    rootId,
    items: [
      {
        source: "JM",
        workId: "123",
        libraryEntryId: entryId,
        localFiles: "present",
      },
    ],
  });
  assert.equal(
    match({ source: "JM", workId: "123", title: "renamed" }).kind,
    "missing",
  );
  controller.dispose();
});

test("disposed library ignores a late recycle response", async () => {
  let resolve;
  const controller = new LibraryController({
    read: async () => snapshot(),
    recycle: async () => new Promise((r) => (resolve = r)),
  });
  await controller.read();
  const before = controller.getState().snapshot;
  const pending = controller.recycle(entryId, before);
  controller.dispose();
  resolve(outcome());
  assert.equal(await pending, null);
  assert.equal(controller.getState().snapshot, before);
});

test("uncertain OS result never inherits a stale present receipt", () => {
  const current = snapshot();
  current.items[0].state = "unreadable";
  current.items[0].errorCode = "LIBRARY_RECYCLE_RESULT_UNCERTAIN";
  const match = createInventoryMatcher(current, {
    rootId,
    items: [
      {
        source: "JM",
        workId: "123",
        libraryEntryId: entryId,
        localFiles: "present",
      },
    ],
  });
  assert.equal(
    match({ source: "JM", workId: "123", title: "x" }).kind,
    "unknown",
  );
});
