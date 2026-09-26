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
