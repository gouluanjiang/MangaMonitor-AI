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
    onQuery = null,
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
      onQuery?.(query.page);
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
    set onQuery(value) {
      onQuery = value;
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
test("revalidation waits for an outstanding page and checks the head before resuming full indexing", async () => {
  for (const pauseAgain of [false, true]) {
    const f = fixture(60),
      r = reader(f);
    await r.resume();
    let release, pageStarted;
    f.held = new Promise((resolve) => {
      release = resolve;
    });
    const started = new Promise((resolve) => {
      pageStarted = resolve;
    });
    f.onQuery = (page) => {
      if (page === 2) pageStarted();
    };
    const inFlight = r.readAll();
    await started;
    r.pause();
    const verifying = r.revalidate();
    if (pauseAgain) r.pause();
    await Promise.resolve();
    assert.deepEqual(f.calls, [1, 2]);
    release();
    await Promise.all([inFlight, verifying]);
    assert.deepEqual(f.calls, pauseAgain ? [1, 2] : [1, 2, 1, 3]);
    assert.equal(r.state.phase, pauseAgain ? "paused" : "complete");
  }
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
      assert.ok(
        range.last > range.first,
        "the tail keeps a measurable row even below the grid",
      );
    }
  }
  assert.deepEqual(gridWindow(0, 7, 300, 1000000, 900), {
    first: 0,
    last: 0,
    height: 0,
  });
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

test("1877 Pica favorite entries read all 94 pages, retain same-page duplicates and cache raw counts", async () => {
  const data = fixture(1877);
  const picaScope = { source: "Pica", sessionId: "synthetic-pica" };
  data.override = (page, result) => {
    const items = result.items.map((item) => ({ ...item, source: "Pica" }));
    if (page === 33) items[11] = { ...items[10] };
    return { ...result, ...picaScope, items };
  };
  const collector = new CollectionReader(
    data.adapter,
    picaScope,
    null,
    false,
    0,
  );
  await collector.readAll();
  assert.equal(collector.state.phase, "complete");
  assert.equal(data.calls.length, 94);
  assert.equal(data.saved.items.length, 1877);
  assert.equal(new Set(data.saved.items.map((item) => item.workId)).size, 1876);
  assert.equal(data.saved.items.at(-1).workId, "1877");
  assert.deepEqual(
    data.saved.pageEnds,
    Array.from({ length: 94 }, (_, index) => Math.min((index + 1) * 20, 1877)),
  );
  collector.dispose();
  const restored = new CollectionReader(
    data.adapter,
    picaScope,
    null,
    false,
    0,
  );
  await restored.readAll();
  assert.equal(restored.state.phase, "complete");
  assert.deepEqual(data.calls.slice(94), [1]);
  assert.equal(restored.state.snapshot.items.length, 1877);
  assert.deepEqual(restored.state.snapshot.pageEnds, data.saved.pageEnds);
  restored.dispose();
});

test("legacy Pica partial caches restart from the verified head and build page boundaries", async () => {
  const data = fixture(60);
  const picaScope = { source: "Pica", sessionId: "synthetic-pica-legacy" };
  data.override = (_page, result) => ({
    ...result,
    ...picaScope,
    items: result.items.map((item) => ({ ...item, source: "Pica" })),
  });
  const picaPage = (page) => ({
    ...data.makePage(page),
    items: data
      .makePage(page)
      .items.map((item) => ({ ...item, source: "Pica" })),
  });
  data.saved = appendCatalog(
    appendCatalog(null, picaPage(1), 1),
    picaPage(2),
    2,
  );
  delete data.saved.pageEnds;
  const restored = new CollectionReader(
    data.adapter,
    picaScope,
    null,
    false,
    0,
  );
  await restored.resume();
  assert.deepEqual(data.calls, [1]);
  assert.equal(restored.state.snapshot.items.length, 20);
  assert.deepEqual(restored.state.snapshot.pageEnds, [20]);
  await restored.readAll();
  assert.deepEqual(data.calls, [1, 2, 3]);
  assert.equal(restored.state.phase, "complete");
  assert.deepEqual(data.saved.pageEnds, [20, 40, 60]);
  restored.dispose();
});

test("legacy duplicate-free complete Pica caches still reuse their complete contents", async () => {
  const data = fixture(40);
  const picaScope = { source: "Pica", sessionId: "synthetic-pica-legacy" };
  data.override = (_page, result) => ({
    ...result,
    ...picaScope,
    items: result.items.map((item) => ({ ...item, source: "Pica" })),
  });
  let snapshot = null;
  for (let page = 1; page <= 2; page++) {
    const result = data.makePage(page);
    snapshot = appendCatalog(snapshot, {
      ...result,
      items: result.items.map((item) => ({ ...item, source: "Pica" })),
    });
  }
  delete snapshot.pageEnds;
  data.saved = data.complete = snapshot;
  const restored = new CollectionReader(
    data.adapter,
    picaScope,
    null,
    false,
    0,
  );
  await restored.readAll();
  assert.deepEqual(data.calls, [1]);
  assert.equal(restored.state.phase, "complete");
  assert.equal(restored.state.snapshot.items.length, 40);
  assert.equal(restored.state.snapshot.pageEnds, undefined);
  restored.dispose();
});

test("legacy JM partial progress stays reusable and cannot grow unproven duplicate entries", () => {
  const data = fixture(40);
  const old = appendCatalog(null, data.makePage(1), 1);
  delete old.pageEnds;
  const next = appendCatalog(old, data.makePage(2), 2);
  assert.equal(next.complete, true);
  assert.equal(next.pageEnds, undefined);
  const picaOld = {
    ...old,
    items: old.items.map((item) => ({ ...item, source: "Pica" })),
  };
  const duplicate = { ...work(21), source: "Pica" };
  const items = data
    .makePage(2)
    .items.map((item) => ({ ...item, source: "Pica" }));
  items[0] = duplicate;
  items[1] = { ...duplicate };
  assert.throws(() => appendCatalog(picaOld, { ...data.makePage(2), items }), {
    code: "CATALOG_CHANGED",
  });
  assert.deepEqual(
    appendCatalog(null, {
      items: [],
      page: 1,
      total: 0,
      pages: 0,
      hasMore: false,
      folders: [],
    }).pageEnds,
    [0],
  );
});

test("Pica conflicting same-page records and cross-page overlap cannot claim complete", () => {
  const pica = (id) => ({ ...work(id), source: "Pica" });
  const firstPage = {
    items: [pica(1), pica(2)],
    page: 1,
    total: 4,
    pages: 2,
    hasMore: true,
    folders: [],
  };
  const previous = appendCatalog(null, firstPage, 1);
  for (const items of [
    [pica(1), pica(3)],
    [pica(3), { ...pica(3), title: "Conflict" }],
  ]) {
    assert.throws(
      () =>
        appendCatalog(
          previous,
          { ...firstPage, page: 2, hasMore: false, items },
          2,
        ),
      { code: "CATALOG_CHANGED" },
    );
  }
});

test("explicit retry after full-read cancellation retries one failed page, while a failed head stays first-page only", async () => {
  const data = fixture(80);
  data.failPage = 3;
  const collector = reader(data);
  await collector.readAll();
  assert.equal(collector.state.phase, "error");
  collector.stopReadAll();
  await collector.retry();
  assert.deepEqual(data.calls, [1, 2, 3, 3]);
  assert.equal(collector.state.snapshot.items.length, 60);
  assert.equal(collector.state.phase, "ready");
  collector.dispose();

  const head = fixture(80);
  head.failPage = 1;
  const first = reader(head);
  await first.resume();
  assert.equal(first.state.phase, "error");
  await first.retry();
  assert.deepEqual(head.calls, [1, 1]);
  assert.equal(first.state.snapshot.items.length, 20);
  assert.equal(first.state.phase, "ready");
  first.dispose();
});
