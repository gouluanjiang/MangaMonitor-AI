import test from "node:test";
import assert from "node:assert/strict";
import {
  appendRecentUpdates,
  RecentUpdatesReader,
} from "../src/recent-updates.ts";
import { SourceError } from "../src/source-runtime.ts";

const scope = { source: "JM", sessionId: "synthetic-recent" };
const work = (id, overrides = {}) => ({
  source: "JM",
  workId: String(id),
  title: `Synthetic work ${id}`,
  authors: ["Writer"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
  ...overrides,
});
const page = (number, ids, overrides = {}) => ({
  ...scope,
  page: number,
  items: ids.map((id) => work(id)),
  total: 40,
  pages: 3,
  hasMore: true,
  folders: [],
  ...overrides,
});

test("moving recent pages merge duplicate identities without reordering earlier works or requiring an unchanged site total", () => {
  const first = appendRecentUpdates(null, page(1, [3, 2, 1]), 1);
  const unchanged = structuredClone(first);
  const second = appendRecentUpdates(
    first,
    page(2, [], {
      total: 39,
      items: [
        work(1, { tags: ["中文"], sourceUpdatedAt: "2026-09-26" }),
        work(4),
      ],
    }),
    2,
  );
  assert.deepEqual(
    second.items.map((w) => w.workId),
    ["3", "2", "1", "4"],
  );
  assert.deepEqual(second.items[2].tags, ["中文"]);
  assert.equal(second.items[2].sourceUpdatedAt, "2026-09-26");
  assert.equal(second.duplicates, 1);
  assert.equal(second.hasMore, true);
  assert.equal(second.total, 39);
  assert.deepEqual(first, unchanged);
  const refreshed = appendRecentUpdates(null, page(1, [9, 8]), 3);
  assert.deepEqual(
    refreshed.items.map((w) => w.workId),
    ["9", "8"],
  );
  const unknownFirst = appendRecentUpdates(
    null,
    page(1, [1, 2], {
      total: 4,
      pages: null,
      hasMore: null,
    }),
  );
  const unknownNext = appendRecentUpdates(
    unknownFirst,
    page(2, [2, 3], {
      total: 4,
      pages: null,
      hasMore: null,
    }),
  );
  // Two moving pages contain four records but only three unique works. The
  // mutable total is not evidence that the remaining source page was read.
  assert.notEqual(unknownNext.hasMore, false);
  const unknownThird = appendRecentUpdates(
    unknownNext,
    page(3, [3, 4], {
      total: 3,
      pages: null,
      hasMore: null,
    }),
  );
  assert.deepEqual(
    unknownThird.items.map((w) => w.workId),
    ["1", "2", "3", "4"],
  );
});

test("stalled pages and browser budgets stop instead of silently skipping or fetching an entire site", () => {
  const first = appendRecentUpdates(null, page(1, [1, 2]), 1);
  const original = structuredClone(first);
  for (const broken of [page(2, []), page(2, [1, 2])]) {
    assert.throws(() => appendRecentUpdates(first, broken), {
      code: "RECENT_PAGE_STALLED",
    });
  }
  assert.throws(() => appendRecentUpdates(first, page(3, [3])), {
    code: "INVALID_RESPONSE",
  });
  assert.throws(() => appendRecentUpdates(first, page(2, [3]), 2, 1), {
    code: "RECENT_LIMIT",
  });
  assert.deepEqual(first, original);
  const empty = appendRecentUpdates(
    null,
    page(1, [], { total: 0, pages: 1, hasMore: false }),
  );
  assert.equal(empty.items.length, 0);
  assert.equal(empty.hasMore, false);
  const issues = appendRecentUpdates(
    first,
    page(2, [], {
      issues: [
        { page: 2, index: 1, workId: null, code: "SOURCE_ITEM_INVALID" },
      ],
      hasMore: false,
    }),
  );
  assert.equal(issues.items.length, 2);
  assert.equal(issues.issues.length, 1);
});

test("recent reader starts one page, coalesces simultaneous next requests and retries the failed page while retaining its catalog", async () => {
  const calls = [];
  let release;
  let failure = true;
  const reader = new RecentUpdatesReader(
    {
      query: async (requested, query) => {
        calls.push({ requested, query });
        if (query.page === 2 && failure) {
          await new Promise((resolve) => {
            release = resolve;
          });
          throw new SourceError("SOURCE_TIMEOUT");
        }
        return page(query.page, query.page === 1 ? [1, 2] : [2, 3]);
      },
    },
    scope,
  );
  await reader.start();
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0], {
    requested: scope,
    query: { kind: "recent", query: "", folderId: null, page: 1 },
  });
  const retained = reader.state.snapshot;
  const next = reader.loadNext();
  const repeated = reader.loadNext();
  assert.equal(calls.length, 2);
  release();
  await Promise.all([next, repeated]);
  assert.equal(reader.state.phase, "error");
  assert.equal(reader.state.snapshot, retained);
  failure = false;
  await reader.retry();
  assert.deepEqual(
    calls.map((c) => c.query.page),
    [1, 2, 2],
  );
  assert.deepEqual(
    reader.state.snapshot.items.map((w) => w.workId),
    ["1", "2", "3"],
  );
  assert.equal(reader.state.phase, "ready");
  await Promise.resolve();
  assert.equal(calls.length, 3);
});

test("refresh replaces only on success and a disposed or different-session reader cannot publish a stale result", async () => {
  let revision = 0;
  let release;
  let fail = false;
  let hold = false;
  let stale = false;
  const reader = new RecentUpdatesReader(
    {
      query: async () => {
        const response = page(1, revision ? [9, 8] : [2, 1]);
        if (stale) response.sessionId = "another-account";
        if (hold)
          await new Promise((resolve) => {
            release = resolve;
          });
        if (fail) throw new SourceError("SOURCE_TIMEOUT");
        return response;
      },
    },
    scope,
  );
  await reader.start();
  const retained = reader.state.snapshot;
  fail = true;
  await reader.refresh();
  assert.equal(reader.state.snapshot, retained);
  fail = false;
  revision = 1;
  await reader.retry();
  assert.deepEqual(
    reader.state.snapshot.items.map((w) => w.workId),
    ["9", "8"],
  );
  stale = true;
  await reader.refresh();
  assert.equal(reader.state.error.code, "STALE_SESSION");
  assert.deepEqual(
    reader.state.snapshot.items.map((w) => w.workId),
    ["9", "8"],
  );
  stale = false;
  hold = true;
  const pending = reader.refresh();
  const beforeDispose = reader.state;
  reader.dispose();
  release();
  await pending;
  assert.equal(reader.state, beforeDispose);
});

test("a slow saved history cannot delay live results or cause extra detail requests", async () => {
  let releaseHistory;
  const calls = [];
  const reader = new RecentUpdatesReader(
    {
      query: async (_scope, request) => {
        calls.push(request.kind);
        return page(1, [1, 2]);
      },
      recentHistory: () =>
        new Promise((resolve) => {
          releaseHistory = resolve;
        }),
    },
    scope,
  );
  const started = reader.start();
  await Promise.resolve();
  assert.equal(reader.state.phase, "ready");
  assert.deepEqual(calls, ["recent"]);
  const live = reader.state.snapshot;
  releaseHistory({ ...scope, items: [work(3)], coverage: {}, revision: 1 });
  await started;
  assert.equal(reader.state.snapshot, live);
  assert.equal(reader.state.retainedItems[0].workId, "3");
  assert.deepEqual(calls, ["recent"]);
});

test("failed supplementary history preserves a successful live page", async () => {
  const reader = new RecentUpdatesReader(
    {
      query: async () => page(1, [1, 2]),
      recentHistory: async () => {
        throw new SourceError("STORE_BUSY");
      },
    },
    scope,
  );
  await reader.start();
  assert.equal(reader.state.historyError, true);
  assert.equal(reader.state.historyErrorCode, "STORE_BUSY");
  assert.equal(reader.state.phase, "ready");
  assert.equal(reader.state.snapshot.items.length, 2);
});

test("history diagnostics discard raw errors and clear after recovery", async () => {
  let failed = true;
  const reader = new RecentUpdatesReader(
    {
      query: async () => page(1, [1, 2]),
      recentHistory: async () => {
        if (failed) throw new Error("private path and account data");
        return { ...scope, items: [work(3)], coverage: {}, revision: 2 };
      },
    },
    scope,
  );
  await reader.start();
  assert.equal(reader.state.historyErrorCode, "SOURCE_UNAVAILABLE");
  assert.equal(JSON.stringify(reader.state).includes("private path"), false);
  failed = false;
  await reader.refreshHistory();
  assert.equal(reader.state.historyError, false);
  assert.equal(reader.state.historyErrorCode, null);
  assert.equal(reader.state.retainedItems[0].workId, "3");
});

const idsOf = (items) => items.map((item) => item.workId);
const range = (first, last) =>
  Array.from({ length: last - first + 1 }, (_, index) => first + index);
const history = (items) => ({ ...scope, items, coverage: {}, revision: 1 });

test("live pages enrich saved works in place; only explicit refresh adopts their known source order", async () => {
  const saved = range(1, 400).map((id) =>
    work(id, id === 332 ? { tags: ["中文"] } : {}),
  );
  const pages = [
    range(1, 80),
    range(81, 160),
    [...range(161, 178), 332, ...range(179, 239)],
  ];
  const calls = [];
  const reader = new RecentUpdatesReader(
    {
      recentHistory: async () => history(saved),
      query: async (_scope, request) => {
        calls.push(request.page);
        return page(request.page, pages[request.page - 1], {
          total: 500,
          pages: 7,
          items: pages[request.page - 1].map((id) =>
            work(id, id === 332 ? { title: "Updated synthetic title" } : {}),
          ),
        });
      },
    },
    scope,
  );
  await reader.start();
  const original = idsOf(reader.state.displayItems);
  await reader.loadNext();
  await reader.loadNext();
  assert.equal(idsOf(reader.state.snapshot.items).indexOf("332"), 178);
  assert.equal(idsOf(reader.state.displayItems).indexOf("332"), 331);
  assert.deepEqual(idsOf(reader.state.displayItems), original);
  assert.equal(reader.state.displayItems[331].title, "Updated synthetic title");
  assert.deepEqual(reader.state.displayItems[331].tags, ["中文"]);
  assert.equal(reader.state.snapshot.items.length, 240);
  assert.equal(new Set(idsOf(reader.state.displayItems)).size, 400);

  await reader.refresh();
  assert.equal(idsOf(reader.state.displayItems).indexOf("332"), 178);
  assert.equal(reader.state.snapshot.items.length, 80);
  const refreshed = idsOf(reader.state.displayItems);
  saved.reverse();
  saved.unshift(work(501));
  await reader.refreshHistory();
  assert.deepEqual(idsOf(reader.state.displayItems), [...refreshed, "501"]);
  assert.deepEqual(calls, [1, 2, 3, 1]);
});

for (const historyFirst of [false, true]) {
  test(`initial ${historyFirst ? "history" : "live page"} displays without waiting and keeps its order when the other arrives`, async () => {
    let releaseLive, releaseHistory;
    const reader = new RecentUpdatesReader(
      {
        query: () =>
          new Promise((resolve) => {
            releaseLive = resolve;
          }),
        recentHistory: () =>
          new Promise((resolve) => {
            releaseHistory = resolve;
          }),
      },
      scope,
    );
    const pending = reader.start();
    if (historyFirst) releaseHistory(history([work(3), work(2)]));
    else releaseLive(page(1, [1, 2]));
    await Promise.resolve();
    assert.deepEqual(
      idsOf(reader.state.displayItems),
      historyFirst ? ["3", "2"] : ["1", "2"],
    );
    if (historyFirst) releaseLive(page(1, [1, 2]));
    else releaseHistory(history([work(3), work(2, { tags: ["AI作画"] })]));
    await pending;
    assert.deepEqual(
      idsOf(reader.state.displayItems),
      historyFirst ? ["3", "2", "1"] : ["1", "2", "3"],
    );
    if (!historyFirst)
      assert.deepEqual(reader.state.displayItems[1].tags, ["AI作画"]);
  });
}

test("failed refresh retains displayed order and retry resets it once, even with a late history", async () => {
  let fail = false,
    releaseHistory,
    holdHistory = false;
  let head = [1, 2];
  const reader = new RecentUpdatesReader(
    {
      query: async (_scope, request) => {
        if (fail) throw new SourceError("SOURCE_TIMEOUT");
        return page(request.page, request.page === 1 ? head : [9, 4]);
      },
      recentHistory: () =>
        holdHistory
          ? new Promise((resolve) => {
              releaseHistory = resolve;
            })
          : Promise.resolve(history([work(1), work(2), work(3), work(9)])),
    },
    scope,
  );
  await reader.start();
  await reader.loadNext();
  const original = reader.state.displayItems;
  fail = true;
  holdHistory = true;
  const refresh = reader.refresh();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(reader.state.displayItems, original);
  assert.equal(reader.state.phase, "error");
  fail = false;
  head = [2, 1];
  await reader.retry();
  // The previous live-only work (4) is outside the refreshed window. Saved works
  // retain their metadata and are reordered only by the successful user refresh.
  assert.deepEqual(idsOf(reader.state.displayItems), ["2", "1", "9", "3"]);
  releaseHistory(history([work(8), work(3), work(9), work(2), work(1)]));
  await refresh;
  assert.deepEqual(idsOf(reader.state.displayItems), ["2", "1", "9", "3", "8"]);
});

test("coalescing refresh during a pending next page cannot silently reorder saved history", async () => {
  let release;
  const calls = [];
  const reader = new RecentUpdatesReader(
    {
      recentHistory: async () => history([work(1), work(2), work(3), work(9)]),
      query: async (_scope, request) => {
        calls.push(request.page);
        if (request.page === 1) return page(1, [1, 2]);
        await new Promise((resolve) => {
          release = resolve;
        });
        return page(2, [9, 4]);
      },
    },
    scope,
  );
  await reader.start();
  const next = reader.loadNext();
  const refresh = reader.refresh();
  release();
  await Promise.all([next, refresh]);
  assert.deepEqual(idsOf(reader.state.displayItems), ["1", "2", "3", "9", "4"]);
  assert.deepEqual(calls, [1, 2]);
});
