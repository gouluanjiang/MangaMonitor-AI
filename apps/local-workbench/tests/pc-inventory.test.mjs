import test from "node:test";
import assert from "node:assert/strict";
import { createInventoryMatcher } from "../src/inventory-model.ts";
import {
  LibraryController,
  createLibraryAdapter,
} from "../src/library-runtime.ts";
import { emptyLibrary } from "../src/library-types.ts";

const rootId = "a".repeat(64),
  entryId = "b".repeat(64);
const work = { source: "JM", workId: "123", title: "Source title" };
const item = {
  id: entryId,
  relativePath: "Renamed.zip",
  fileName: "Renamed.zip",
  format: "zip",
  title: "Renamed",
  authors: [],
  description: null,
  tags: [],
  bytes: 100,
  modifiedAt: 1,
  pageCount: 2,
  coverAvailable: false,
  state: "indexed",
  errorCode: null,
  sourceRef: { source: "JM", workId: "123" },
  identityEvidence: "metadata",
};
const snapshot = (items = [item]) => ({
  ...emptyLibrary(),
  revision: 1,
  rootId,
  rootPath: "C:\\Synthetic",
  generation: 1,
  phase: "complete",
  freshness: "live",
  items,
  visited: items.length,
  updatedAt: 1,
});

test("renamed real PC identities are owned, unavailable files require review, and partial catalogs never prove absence", () => {
  assert.equal(createInventoryMatcher(snapshot())(work).kind, "owned");
  for (const invalid of [
    { state: "unreadable" },
    { errorCode: "LIBRARY_FILE_CHANGED" },
    { pageCount: 0 },
  ])
    assert.equal(
      createInventoryMatcher(snapshot([{ ...item, ...invalid }]))(work).kind,
      "candidate",
    );
  assert.equal(
    createInventoryMatcher(snapshot(), [], true, false)(work).kind,
    "unknown",
  );
  assert.equal(
    createInventoryMatcher(snapshot(), [], false)(work).kind,
    "unknown",
  );
  assert.equal(createInventoryMatcher(snapshot([]))(work).kind, "missing");
  assert.equal(
    createInventoryMatcher({ ...snapshot([]), phase: "paused" })(work).kind,
    "incomplete",
  );
  assert.equal(
    createInventoryMatcher(emptyLibrary())(work).kind,
    "unconfigured",
  );
});

test("explicit mapping picker is scoped; cancellation preserves state and successful import starts one new catalog scan", async () => {
  const calls = [];
  let cancel = true;
  const adapter = createLibraryAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "library_read") return snapshot();
      if (command === "library_import_paths")
        return cancel
          ? null
          : {
              snapshot: { ...snapshot(), revision: 2, phase: "paused" },
              mapped: 2,
              associated: 1,
              unchanged: 0,
            };
      if (command === "library_scan")
        return { ...snapshot(), revision: 3, generation: 2 };
      throw new Error("Unexpected command");
    },
  });
  const controller = new LibraryController(adapter);
  await controller.read();
  await controller.importPaths();
  assert.deepEqual(controller.getState().snapshot, snapshot());
  assert.equal(calls.filter((v) => v.command === "library_scan").length, 0);
  cancel = false;
  await controller.importPaths();
  assert.equal(controller.getState().snapshot.generation, 2);
  assert.deepEqual(
    calls
      .filter((v) => v.command === "library_import_paths")
      .map((v) => v.args),
    [
      { rootId, generation: 1 },
      { rootId, generation: 1 },
    ],
  );
  assert.deepEqual(
    calls.filter((v) => v.command === "library_scan").map((v) => v.args),
    [{ rootId, generation: 1, action: "start" }],
  );
  controller.dispose();
});

test("failed mapping does not start a scan or discard existing entries", async () => {
  const calls = [];
  const adapter = createLibraryAdapter({
    native: true,
    invoke: async (command) => {
      calls.push(command);
      if (command === "library_read") return snapshot();
      throw { code: "LIBRARY_MIGRATION_FILE_CHANGED" };
    },
  });
  const controller = new LibraryController(adapter);
  await controller.read();
  await controller.importPaths();
  assert.deepEqual(controller.getState().snapshot, snapshot());
  assert.match(controller.getState().error, /不一致/);
  assert.deepEqual(calls, ["library_read", "library_import_paths"]);
  controller.dispose();
});
