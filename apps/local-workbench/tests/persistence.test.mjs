import test from "node:test";
import assert from "node:assert/strict";
import {
  createWorkbenchPersistence,
  BOOKLISTS_STORAGE_KEY,
  persistenceErrorMessage,
} from "../src/persistence.ts";
import {
  initialPreferences,
  PREFERENCES_STORAGE_KEY,
} from "../src/preferences.ts";
import { initialBooklists, createBooklist } from "../src/booklists.ts";

function memory(entries = []) {
  const values = new Map(entries);
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
  };
}
const changedPreferences = () => ({
  ...initialPreferences(),
  appearance: { ...initialPreferences().appearance, density: 9 },
});
const oneList = () =>
  createBooklist(initialBooklists(), {
    id: "list-a",
    name: "我的书单",
    now: 100,
  });

test("browser preferences remain compatible and do not overwrite booklists", async () => {
  const storage = memory([
    [PREFERENCES_STORAGE_KEY, JSON.stringify(initialPreferences())],
    [BOOKLISTS_STORAGE_KEY, JSON.stringify(oneList())],
  ]);
  const repository = createWorkbenchPersistence({ native: false, storage });
  const beforeLists = storage.getItem(BOOKLISTS_STORAGE_KEY);
  await repository.preferences.write(
    await repository.preferences.read(),
    changedPreferences(),
  );
  assert.equal(
    (
      await createWorkbenchPersistence({
        native: false,
        storage,
      }).preferences.read()
    ).value.appearance.density,
    9,
  );
  assert.equal(storage.getItem(BOOKLISTS_STORAGE_KEY), beforeLists);
});

test("booklists persist across repository instances and fixture storage is isolated", async () => {
  const storage = memory();
  const normal = createWorkbenchPersistence({ native: false, storage });
  const fixture = createWorkbenchPersistence({
    native: false,
    fixture: "ready-100",
    storage,
  });
  await fixture.booklists.write(await fixture.booklists.read(), oneList());
  assert.equal((await normal.booklists.read()).value.lists.length, 0);
  assert.equal(
    (
      await createWorkbenchPersistence({
        native: false,
        fixture: "ready-100",
        storage,
      }).booklists.read()
    ).value.lists[0].name,
    "我的书单",
  );
  await fixture.preferences.write(
    await fixture.preferences.read(),
    changedPreferences(),
  );
  assert.equal((await normal.preferences.read()).value.appearance.density, 7);
});

test("malformed and future documents remain intact instead of becoming empty writable state", async () => {
  for (const raw of [
    "{broken",
    JSON.stringify({ version: 99, lists: [] }),
    JSON.stringify({ ...initialBooklists(), extra: true }),
  ]) {
    const storage = memory([[BOOKLISTS_STORAGE_KEY, raw]]);
    const repo = createWorkbenchPersistence({ native: false, storage });
    await assert.rejects(repo.booklists.read(), { code: "INVALID_DOCUMENT" });
    await assert.rejects(
      repo.booklists.write(
        { revision: null, value: initialBooklists() },
        oneList(),
      ),
      { code: "CONFLICT" },
    );
    assert.equal(storage.getItem(BOOKLISTS_STORAGE_KEY), raw);
  }
});

test("two writers cannot silently replace a newer browser revision", async () => {
  const storage = memory();
  const first = createWorkbenchPersistence({ native: false, storage });
  const second = createWorkbenchPersistence({ native: false, storage });
  const snapshots = await Promise.all([
    first.booklists.read(),
    second.booklists.read(),
  ]);
  const results = await Promise.allSettled([
    first.booklists.write(snapshots[0], oneList()),
    second.booklists.write(snapshots[1], initialBooklists()),
  ]);
  assert.equal(results.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal(
    results.find((r) => r.status === "rejected").reason.code,
    "CONFLICT",
  );
  assert.equal((await first.booklists.read()).value.lists.length, 1);
});

test("a failed write leaves the stored document unchanged and can be retried", async () => {
  const storage = memory([
    [BOOKLISTS_STORAGE_KEY, JSON.stringify(initialBooklists())],
  ]);
  const set = storage.setItem;
  storage.setItem = () => {
    throw new Error("private/path detail");
  };
  const repo = createWorkbenchPersistence({ native: false, storage });
  const before = await repo.booklists.read();
  const error = await repo.booklists.write(before, oneList()).catch((e) => e);
  assert.equal(error.code, "STORAGE_UNAVAILABLE");
  assert.ok(!persistenceErrorMessage(error).includes("private/path"));
  assert.equal((await repo.booklists.read()).value.lists.length, 0);
  storage.setItem = set;
  assert.equal(
    (await repo.booklists.write(before, oneList())).value.lists.length,
    1,
  );
});

test("native failures never read or write the browser preview as a fallback", async () => {
  const storage = {
    getItem() {
      assert.fail("browser read");
    },
    setItem() {
      assert.fail("browser write");
    },
  };
  const repo = createWorkbenchPersistence({
    native: true,
    storage,
    invoke: async () => {
      throw { code: "BUSY" };
    },
  });
  await assert.rejects(repo.preferences.read(), { code: "BUSY" });
  await assert.rejects(
    repo.booklists.write({ revision: 3, value: initialBooklists() }, oneList()),
    { code: "BUSY" },
  );
});

test("native save sends a typed fixed command and waits for the durable response", async () => {
  const calls = [];
  let release;
  const wait = new Promise((resolve) => {
    release = resolve;
  });
  const repo = createWorkbenchPersistence({
    native: true,
    fixture: "ready-100",
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "read_booklists")
        return { revision: 4, value: initialBooklists() };
      await wait;
      return { revision: 5, value: args.value };
    },
  });
  const previous = await repo.booklists.read();
  let complete = false;
  const saving = repo.booklists.write(previous, oneList()).then((result) => {
    complete = true;
    return result;
  });
  await Promise.resolve();
  assert.equal(complete, false);
  assert.deepEqual(calls[1], {
    command: "write_booklists",
    args: { expectedRevision: 4, value: oneList() },
  });
  release();
  assert.equal((await saving).revision, 5);
});

test("invalid native responses cannot replace the current state", async () => {
  for (const response of [
    { revision: -1, value: initialBooklists() },
    { revision: 0, value: { version: 2, lists: [] } },
    { revision: "0", value: initialBooklists() },
    { revision: 0, value: initialBooklists(), file: "arbitrary" },
  ]) {
    const repo = createWorkbenchPersistence({
      native: true,
      invoke: async () => response,
    });
    await assert.rejects(repo.booklists.read(), { code: "INVALID_DOCUMENT" });
  }
});

test("native background selection cancellation preserves the existing draft", async () => {
  const calls = [];
  const repo = createWorkbenchPersistence({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return null;
    },
  });
  assert.equal(await repo.chooseBackground(), null);
  assert.deepEqual(calls, [{ command: "choose_background", args: undefined }]);
});
