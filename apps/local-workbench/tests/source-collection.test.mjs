import test from "node:test";
import assert from "node:assert/strict";
import { CollectionReader, appendCatalog } from "../src/source-collection.ts";
import { gridWindow } from "../src/source-grid-layout.ts";
import { queueCover } from "../src/source-cover-queue.ts";
const scope = { source: "JM", sessionId: "synthetic-collection" };

test("refresh clears a failed next-page budget and compact indexes enforce the memory budget", async () => {
  const f = fixture(60),
    r = reader(f);
  await r.resume();
  f.failPage = 2;
  await r.loadNext();
  await r.refresh();
  assert.deepEqual(f.calls, [1, 2, 1]);
  assert.equal(r.state.phase, "ready");
  const rich = {
    ...work(1),
    description: "long".repeat(10000),
    tags: ["large".repeat(1000)],
  };
  const snapshot = appendCatalog(
    null,
    { items: [rich], page: 1, total: 1, pages: 1, hasMore: false, folders: [] },
    1,
    2048,
  );
  assert.equal(snapshot.items[0].description, null);
  assert.deepEqual(snapshot.items[0].tags, []);
  assert.throws(
    () =>
      appendCatalog(
        null,
        {
          items: [{ ...rich, title: "字".repeat(1000) }],
          page: 1,
          total: 1,
          pages: 1,
          hasMore: false,
          folders: [],
        },
        1,
        2048,
      ),
    { code: "CATALOG_LIMIT" },
  );
});
const work = (id) => ({
  source: "JM",
  workId: String(id),
  title: "合成作品 " + String(id).padStart(4, "0"),
  authors: [],
  description: null,
  tags: [],
  favorite: true,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});
function fixture(total = 2000) {
  const calls = [],
    writes = [];
  let saved = null,
    complete = null,
    failPage = 0,
    cacheFailure = false,
    override = null,
    held = null;
  const makePage = (page) => ({
    ...scope,
    items: Array.from(
      { length: Math.min(20, total - (page - 1) * 20) },
      (_, i) => work((page - 1) * 20 + i + 1),
    ),
    page,
    total,
    pages: Math.ceil(total / 20),
    hasMore: page < Math.ceil(total / 20),
    folders: [],
  });
  const adapter = {
    query: async (_scope, query) => {
      calls.push(query.page);
      if (held) await held;
      if (query.page === failPage) {
        failPage = 0;
        throw Error("synthetic timeout");
      }
      return (
        override?.(query.page, makePage(query.page)) ?? makePage(query.page)
      );
    },
    catalog: async (_scope, request) => {
      if (cacheFailure) throw Error("synthetic cache unavailable");
      if (request.action === "write") {
        saved = structuredClone(request.snapshot);
        writes.push(saved);
        if (saved.complete) complete = saved;
      }
      return {
        ...scope,
        snapshot: structuredClone(saved),
        completeSnapshot: structuredClone(complete),
      };
    },
  };
  return {
    adapter,
    calls,
    writes,
    makePage,
    get saved() {
      return saved;
    },
    get complete() {
      return complete;
    },
    set saved(value) {
      saved = value;
    },
    set complete(value) {
      complete = value;
    },
    set failPage(value) {
      failPage = value;
    },
    set cacheFailure(value) {
      cacheFailure = value;
    },
    set override(value) {
      override = value;
    },
    set held(value) {
      held = value;
    },
  };
}
const reader = (data) =>
  new CollectionReader(data.adapter, scope, null, false, 0);

test("first page is persisted without automatically reading page 2; 2000 works advance through 100 explicit viewport batches", async () => {
  const f = fixture(),
    r = reader(f);
  await r.resume();
  assert.deepEqual(f.calls, [1]);
  assert.equal(r.state.phase, "ready");
  assert.equal(f.saved.items.length, 20);
  for (let page = 2; page <= 100; page++) await r.loadNext();
  assert.equal(r.state.snapshot.items.length, 2000);
  assert.equal(r.state.phase, "complete");
  assert.equal(f.calls.length, 100);
  assert.equal(f.complete.items.at(-1).workId, "2000");
  await r.loadNext();
  assert.equal(f.calls.length, 100);
  assert.deepEqual(
    [...r.state.snapshot.items]
      .reverse()
      .slice(0, 2)
      .map((w) => w.workId),
    ["2000", "1999"],
  );
  r.dispose();
});

test("a failed cached-head validation is retried and never republishes old complete as newly verified", async () => {
  const f = fixture(40);
  f.complete = f.saved = appendCatalog(
    appendCatalog(null, f.makePage(1), 10),
    f.makePage(2),
    11,
  );
  f.override = (page, value) =>
    page === 1 ? { ...value, hasMore: false } : value;
  const r = reader(f);
  await r.resume();
  assert.equal(r.state.phase, "error");
  await r.resume();
  assert.equal(r.state.phase, "error");
  assert.deepEqual(f.calls, [1, 1]);
  assert.equal(r.state.freshness, "cached");
});
test("pause stops new work after the outstanding page; retry retains successful pages", async () => {
  const f = fixture(60),
    r = reader(f);
  await r.resume();
  let release;
  f.held = new Promise((resolve) => {
    release = resolve;
  });
  const pending = r.loadNext();
  await new Promise((resolve) => setTimeout(resolve, 5));
  r.pause();
  release();
  await pending;
  f.held = null;
  assert.equal(r.state.phase, "paused");
  assert.equal(f.saved.items.length, 40);
  await r.loadNext();
  assert.equal(f.calls.length, 2);
  await r.resume();
  f.failPage = 3;
  await r.loadNext();
  assert.equal(r.state.phase, "error");
  assert.equal(r.state.snapshot.items.length, 40);
  await r.resume();
  assert.equal(r.state.phase, "complete");
  assert.deepEqual(f.calls, [1, 2, 3, 3]);
});
test("cached restart verifies first page then resumes at next missing page", async () => {
  const f = fixture(60);
  f.saved = appendCatalog(
    appendCatalog(null, f.makePage(1), 10),
    f.makePage(2),
    11,
  );
  const r = reader(f);
  await r.resume();
  assert.deepEqual(f.calls, [1]);
  assert.equal(r.state.displaySnapshot.items.length, 40);
  assert.equal(r.state.freshness, "verified-cache");
  await r.loadNext();
  assert.deepEqual(f.calls, [1, 3]);
  assert.equal(r.state.snapshot.complete, true);
});
test("changed cached head rebuilds page 1 while keeping the previous complete snapshot", async () => {
  const f = fixture(40);
  f.complete = f.saved = appendCatalog(
    appendCatalog(null, f.makePage(1), 10),
    f.makePage(2),
    11,
  );
  f.override = (page, value) =>
    page === 1
      ? { ...value, items: [work("new"), ...value.items.slice(1)] }
      : value;
  const r = reader(f);
  await r.resume();
  assert.equal(r.state.phase, "ready");
  assert.equal(r.state.displaySnapshot.items.length, 20);
  assert.equal(r.state.displaySnapshot.items[0].workId, "new");
  assert.equal(r.state.completeSnapshot.items.length, 40);
  assert.deepEqual(f.calls, [1]);
});
test("refresh shows the first new page immediately and does not fetch all pages", async () => {
  const f = fixture(60),
    r = reader(f);
  await r.readAll();
  assert.equal(r.state.snapshot.complete, true);
  r.stopReadAll();
  await r.refresh();
  assert.equal(r.state.displaySnapshot.items.length, 20);
  assert.equal(r.state.completeSnapshot.items.length, 60);
  assert.equal(r.state.phase, "ready");
  assert.deepEqual(f.calls, [1, 2, 3, 1]);
});
test("duplicates, overlaps, non-progress, empty continuation and changing totals never become a complete catalog", async () => {
  for (const alter of [
    (value) => ({ ...value, items: [work(1), ...value.items.slice(1)] }),
    (value) => ({ ...value, page: 1 }),
    (value) => ({ ...value, items: [] }),
    (value) => ({ ...value, total: 61 }),
    (value) => ({ ...value, items: [value.items[0], value.items[0]] }),
  ]) {
    const f = fixture(60),
      r = reader(f);
    f.override = (page, value) => (page === 2 ? alter(value) : value);
    await r.resume();
    await r.loadNext();
    assert.equal(r.state.phase, "error");
    assert.equal(r.state.snapshot.complete, false);
    assert.equal(r.state.snapshot.items.length, 20);
  }
});
test("unavailable or unwritable local cache degrades to live memory browsing", async () => {
  const f = fixture(40);
  f.cacheFailure = true;
  const r = reader(f);
  await r.resume();
  assert.equal(r.state.phase, "ready");
  assert.equal(r.state.snapshot.items.length, 20);
  assert.ok(r.state.cacheWarning);
  await r.loadNext();
  assert.equal(r.state.phase, "complete");
  const g = fixture(40),
    s = reader(g);
  await s.resume();
  g.cacheFailure = true;
  await s.loadNext();
  assert.equal(s.state.phase, "complete");
  assert.equal(s.state.snapshot.items.length, 40);
  assert.ok(s.state.cacheWarning);
});
test("unknown JM pagination terminates at its verified total; incomplete limits do not masquerade as complete", () => {
  const terminal = appendCatalog(null, {
    items: [work(1)],
    page: 1,
    total: null,
    pages: 1,
    hasMore: null,
    folders: [],
  });
  assert.equal(terminal.complete, true);
  assert.throws(
    () =>
      appendCatalog(null, {
        items: [work(1)],
        page: 1,
        total: 2,
        pages: 1,
        hasMore: null,
        folders: [],
      }),
    { code: "CATALOG_CHANGED" },
  );
  let snapshot = null;
  for (let page = 1; page <= 2; page++)
    snapshot = appendCatalog(
      snapshot,
      {
        items: [work(page)],
        page,
        total: 2,
        pages: null,
        hasMore: null,
        folders: [],
      },
      page,
    );
  assert.equal(snapshot.complete, true);
  assert.throws(
    () =>
      appendCatalog(
        { ...snapshot, complete: false, page: 999, total: null, hasMore: null },
        {
          items: [work(3)],
          page: 1000,
          total: null,
          pages: null,
          hasMore: null,
          folders: [],
        },
      ),
    { code: "CATALOG_LIMIT" },
  );
});
test("virtual windows stay bounded at beginning middle and end without hiding full data semantics", () => {
  for (const columns of [2, 5, 7, 9]) {
    for (const scroll of [0, 30000, 1000000]) {
      const range = gridWindow(2000, columns, 300, scroll, 900);
      assert.ok((range.last - range.first) * columns <= columns * 8);
      assert.equal(range.height, Math.ceil(2000 / columns) * 300);
    }
  }
});
test("cover queue cancels waiting work and recovers from synchronous failures", async () => {
  let a, b;
  const started = [];
  const first = queueCover(
    () =>
      new Promise((resolve) => {
        started.push(1);
        a = resolve;
      }),
  );
  const second = queueCover(
    () =>
      new Promise((resolve) => {
        started.push(2);
        b = resolve;
      }),
  );
  const cancelled = queueCover(async () => {
    started.push(3);
    return null;
  });
  assert.equal(cancelled.cancel(), true);
  await Promise.resolve();
  a(null);
  b(null);
  await Promise.all([first.promise, second.promise, cancelled.promise]);
  const failure = queueCover(() => {
    throw Error("synthetic sync failure");
  });
  await assert.rejects(failure.promise, /synthetic sync failure/);
  await queueCover(async () => {
    started.push(4);
    return null;
  }).promise;
  assert.deepEqual(started, [1, 2, 4]);
});
