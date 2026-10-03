import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  readCompleteSearch,
  readCompleteAuthorSearch,
} from "../src/source-search.ts";
import {
  createSourceAdapter,
  SourceError,
  validateSourcePage,
} from "../src/source-runtime.ts";
import { RecentUpdatesReader } from "../src/recent-updates.ts";

const scope = { source: "JM", sessionId: "synthetic-jm-boundary" };
const work = (id, source = "JM") => ({
  source,
  workId: source === "JM" ? String(id) : id.toString(16).padStart(24, "0"),
  title: "Synthetic boundary work " + id,
  authors: ["Synthetic Writer"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});
const edge = (item) =>
  item
    ? {
        workId: item.workId,
        fingerprint: createHash("sha256")
          .update(JSON.stringify(item))
          .digest("hex"),
      }
    : null;
function page(
  number,
  ids,
  { total = 6, pages = 3, issues = [], ...overrides } = {},
) {
  const items = ids.map((id) => work(id));
  const rawCount = items.length + issues.length;
  return {
    ...scope,
    page: number,
    items,
    issues,
    total,
    pages,
    hasMore: pages === null ? null : number < pages,
    folders: [],
    jmSearchBoundary: {
      first: issues.some((issue) => issue.index === 1) ? null : edge(items[0]),
      last: issues.some((issue) => issue.index === rawCount)
        ? null
        : edge(items.at(-1)),
    },
    ...overrides,
  };
}
function adapterFor(pages, calls = []) {
  return createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      assert.equal(command, "source_query");
      assert.equal(args.kind, "search");
      calls.push([args.query, args.page]);
      return structuredClone(pages[args.page - 1]);
    },
  });
}

test("keyword and tag browsing fetch one requested page and can resume with pagination evidence", async () => {
  for (const requestKind of ["search", "tag"]) {
    const calls = [],
      seen = [];
    const adapter = createSourceAdapter({
      native: true,
      invoke: async (command, args) => {
        assert.equal(command, "source_query");
        assert.equal(args.kind, requestKind);
        calls.push(args.page);
        return page(args.page, args.page === 1 ? [1, 2] : [3, 4], {
          total: 4,
          pages: 2,
        });
      },
    });
    await readCompleteSearch(adapter, scope, "Fixture tag", {
      current: () => true,
      onPage: (value) => seen.push(value),
      requestKind,
      pageLimit: 1,
    });
    assert.deepEqual(calls, [1]);
    assert.equal(seen[0].complete, false);
    const previous = seen[0];
    await readCompleteSearch(adapter, scope, "Fixture tag", {
      current: () => true,
      onPage: (value) => seen.push(value),
      requestKind,
      pageLimit: 1,
      fromPage: 2,
      items: previous.items,
      recordsRead: previous.recordsRead,
      issues: previous.issues,
      pagination: previous.pagination,
    });
    assert.deepEqual(calls, [1, 2]);
    assert.equal(seen[1].complete, true);
    assert.equal(seen[1].items.length, 4);
  }
});

test("one-page browsing still rejects contradictory pagination rather than calling it a pause", async () => {
  const adapter = adapterFor([
    page(1, [1, 2], { total: 9, pages: 2, hasMore: false }),
  ]);
  await assert.rejects(
    readCompleteSearch(adapter, scope, "Fixture", {
      current: () => true,
      onPage() {},
      pageLimit: 1,
    }),
    /SEARCH_INCOMPLETE/,
  );
});

test("Pica category browsing is forwarded distinctly from tags and JM cannot submit a category", async () => {
  const pica = { source: "Pica", sessionId: "pica-category-fixture" };
  const calls = [];
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push(args);
      return {
        ...pica,
        page: 1,
        pages: 1,
        total: 1,
        hasMore: false,
        folders: [],
        items: [
          { ...work(1, "Pica"), tags: ["Category"], categories: ["Category"] },
        ],
      };
    },
  });
  const seen = [];
  await readCompleteSearch(adapter, pica, "Category", {
    current: () => true,
    onPage: (value) => seen.push(value),
    requestKind: "category",
    pageLimit: 1,
  });
  assert.equal(calls[0].kind, "category");
  assert.equal(seen[0].complete, true);
  assert.deepEqual(seen[0].items[0].categories, ["Category"]);
  await assert.rejects(
    adapter.query(scope, {
      kind: "category",
      query: "Category",
      folderId: null,
      page: 1,
    }),
    { code: "INVALID_INPUT" },
  );
  assert.equal(calls.length, 1);
});
const readOptions = (seen) => ({
  current: () => true,
  onPage: (value) => seen.push(value),
});

test("JM exact adjacent raw boundary rows count once toward total but twice toward the fetch budget", async () => {
  for (const pages of [3, null]) {
    const seen = [],
      calls = [];
    await readCompleteSearch(
      adapterFor(
        [
          page(1, [1, 2, 3], { pages }),
          page(2, [3, 4, 5], { pages }),
          page(3, [5, 6], { pages }),
        ],
        calls,
      ),
      scope,
      "Synthetic Writer",
      readOptions(seen),
    );
    assert.deepEqual(
      calls.map((call) => call[1]),
      [1, 2, 3],
    );
    assert.deepEqual(
      seen.map((value) => value.recordsRead),
      [3, 5, 6],
    );
    assert.deepEqual(
      seen.map((value) => value.pagination.rawFetched),
      [3, 6, 8],
    );
    assert.deepEqual(
      seen.at(-1).items.map((item) => item.workId),
      ["1", "2", "3", "4", "5", "6"],
    );
    assert.equal(seen.at(-1).complete, true);
    assert.equal(seen[0].pagination.previousPage.page, 1);
    assert.equal(seen[0].pagination.previousPage.boundary.last.workId, "3");
  }
});

test("boundary exceptions reject hash/projection conflicts, other repeats, missing evidence and unknown totals", async () => {
  for (const mode of [
    "hash",
    "projection",
    "interior",
    "older",
    "multiple",
    "missing-current",
    "missing-previous",
    "unknown-total",
    "no-progress",
    "first-issue",
    "issue-collision",
  ]) {
    const first = page(1, [1, 2, 3], { total: 5, pages: 2 });
    const second = page(2, [3, 4, 5], { total: 5, pages: 2 });
    if (mode === "hash")
      second.jmSearchBoundary.first.fingerprint = "f".repeat(64);
    if (mode === "projection")
      second.items[0].title = "Conflicting synthetic projection";
    if (mode === "interior")
      Object.assign(second, page(2, [4, 3, 5], { total: 5, pages: 2 }));
    if (mode === "older")
      Object.assign(second, page(2, [1, 4, 5], { total: 5, pages: 2 }));
    if (mode === "multiple")
      Object.assign(second, page(2, [3, 1, 4], { total: 5, pages: 2 }));
    if (mode === "missing-current") delete second.jmSearchBoundary;
    if (mode === "missing-previous") delete first.jmSearchBoundary;
    if (mode === "unknown-total") {
      first.total = second.total = null;
      delete first.jmSearchBoundary;
      delete second.jmSearchBoundary;
    }
    if (mode === "no-progress")
      Object.assign(second, page(2, [3], { total: 5, pages: 2 }));
    if (mode === "first-issue")
      Object.assign(
        second,
        page(2, [3, 4], {
          total: 5,
          pages: 2,
          issues: [
            { page: 2, index: 1, workId: null, code: "SOURCE_ITEM_INVALID" },
          ],
        }),
      );
    if (mode === "issue-collision")
      Object.assign(
        second,
        page(2, [3, 4], {
          total: 5,
          pages: 2,
          issues: [
            { page: 2, index: 3, workId: "1", code: "SOURCE_ITEM_INVALID" },
          ],
        }),
      );
    const seen = [];
    await assert.rejects(
      readCompleteSearch(
        adapterFor([first, second]),
        scope,
        "Synthetic Writer",
        readOptions(seen),
      ),
      { code: "SEARCH_INCOMPLETE" },
      mode,
    );
    assert.ok(
      seen.every((value) => !value.complete),
      mode,
    );
  }
});

test("raw coverage and an accepted boundary never mask a short unique total or pagination contradictions", async () => {
  for (const mode of [
    "short",
    "total-drift",
    "terminal-has-more",
    "early-terminal",
  ]) {
    const first = page(1, [1, 2], { total: 4, pages: 2 });
    const second = page(2, [2, 3], { total: 4, pages: 2 });
    if (mode === "total-drift") second.total = 3;
    if (mode === "terminal-has-more") {
      first.total = second.total = 3;
      second.hasMore = true;
    }
    if (mode === "early-terminal") first.hasMore = false;
    const seen = [];
    await assert.rejects(
      readCompleteSearch(
        adapterFor([first, second]),
        scope,
        "Synthetic Writer",
        readOptions(seen),
      ),
      { code: "SEARCH_INCOMPLETE" },
      mode,
    );
    assert.ok(
      seen.every((value) => !value.complete),
      mode,
    );
    if (mode === "short") {
      assert.equal(seen.at(-1).recordsRead, 3);
      assert.equal(seen.at(-1).pagination.rawFetched, 4);
    }
    if (mode === "total-drift") assert.equal(seen.length, 1);
  }
});

test("network resume carries previous raw edge evidence and does not reset the raw-row budget", async () => {
  const pages = [page(1, [1, 2, 3]), page(2, [3, 4, 5]), page(3, [5, 6])];
  const seen = [],
    calls = [];
  let fail = true;
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (_command, args) => {
      calls.push(args.page);
      if (args.page === 3 && fail) {
        fail = false;
        throw new SourceError("SOURCE_UNAVAILABLE");
      }
      return structuredClone(pages[args.page - 1]);
    },
  });
  await assert.rejects(
    readCompleteSearch(adapter, scope, "Synthetic Writer", readOptions(seen)),
  );
  const paused = seen.at(-1);
  assert.equal(paused.recordsRead, 5);
  assert.equal(paused.pagination.rawFetched, 6);
  const resume = (pagination = paused.pagination) =>
    readCompleteSearch(adapter, scope, "Synthetic Writer", {
      ...readOptions(seen),
      fromPage: 3,
      items: paused.items,
      issues: paused.issues,
      recordsRead: paused.recordsRead,
      pagination,
    });
  await resume();
  assert.deepEqual(calls, [1, 2, 3, 3]);
  assert.equal(seen.at(-1).complete, true);
  assert.equal(seen.at(-1).recordsRead, 6);
  assert.equal(seen.at(-1).pagination.rawFetched, 8);
  assert.equal(paused.pagination.previousPage.page, 2);
  const before = seen.length;
  await assert.rejects(resume({ ...paused.pagination, rawFetched: 19999 }), {
    code: "SEARCH_LIMIT_REACHED",
  });
  assert.equal(
    seen.length,
    before,
    "raw budget failure publishes no over-budget page",
  );
  await assert.rejects(
    resume({
      ...paused.pagination,
      previousPage: { ...paused.pagination.previousPage, page: 1 },
    }),
    { code: "SEARCH_INCOMPLETE" },
  );
  await assert.rejects(
    resume({
      ...paused.pagination,
      previousPage: {
        ...paused.pagination.previousPage,
        query: "Another synthetic query",
      },
    }),
    { code: "SEARCH_INCOMPLETE" },
  );
});

test("each approved author alias has independent boundary evidence, effective total and raw count", async () => {
  const seen = [],
    calls = [];
  const pages = {
    "Synthetic Writer": [
      page(1, [1, 2], { total: 3, pages: 2 }),
      page(2, [2, 3], { total: 3, pages: 2 }),
    ],
    "Synthetic Alias": [
      page(1, [3, 4], { total: 3, pages: 2 }),
      page(2, [4, 5], { total: 3, pages: 2 }),
    ],
  };
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      if (command === "source_author_policy")
        return {
          ...scope,
          revision: 0,
          author: args.author,
          queries: ["Synthetic Writer", "Synthetic Alias"],
          verifiedAliases: [],
          queryFingerprint: "a".repeat(64),
        };
      assert.equal(command, "source_query");
      calls.push([args.query, args.page]);
      return structuredClone(pages[args.query][args.page - 1]);
    },
  });
  await readCompleteAuthorSearch(
    adapter,
    scope,
    "Synthetic Writer",
    readOptions(seen),
  );
  assert.equal(calls.length, 4);
  assert.deepEqual(
    seen.map((value) => value.pagination.rawFetched),
    [2, 4, 2, 4],
  );
  assert.deepEqual(
    seen.map((value) => value.pagination.previousPage.page),
    [1, 2, 1, 2],
  );
  assert.deepEqual(
    seen.map((value) => value.pagination.previousPage.query),
    [
      "Synthetic Writer",
      "Synthetic Writer",
      "Synthetic Alias",
      "Synthetic Alias",
    ],
  );
  assert.deepEqual(
    seen.at(-1).items.map((item) => item.workId),
    ["1", "2", "3", "4", "5"],
  );
  assert.equal(seen.at(-1).recordsRead, 6);
  assert.equal(seen.at(-1).complete, true);
});

test("an accepted boundary retains isolated issue evidence without borrowing an issue slot as an edge", async () => {
  const issue = {
    page: 2,
    index: 3,
    workId: null,
    code: "SOURCE_ITEM_INVALID",
  };
  const seen = [];
  await readCompleteSearch(
    adapterFor([
      page(1, [1, 2], { total: 4, pages: 2 }),
      page(2, [2, 3], { total: 4, pages: 2, issues: [issue] }),
    ]),
    scope,
    "Synthetic Writer",
    readOptions(seen),
  );
  assert.equal(
    seen.at(-1).complete,
    true,
    "pagination completes without clearing partial-item diagnostics",
  );
  assert.deepEqual(seen.at(-1).issues, [issue]);
  assert.equal(seen.at(-1).recordsRead, 4);
  assert.equal(seen.at(-1).pagination.rawFetched, 5);
  assert.equal(seen.at(-1).pagination.previousPage.boundary.last, null);
});

test("JM raw boundary DTOs are restricted to ordered lists, exact-shaped and tied to the original issue slots", async () => {
  const value = page(1, [1, 2], { total: 2, pages: 1 });
  const parsed = validateSourcePage(value, scope, false, false, true);
  assert.deepEqual(parsed.jmSearchBoundary, value.jmSearchBoundary);
  assert.notEqual(parsed.jmSearchBoundary, value.jmSearchBoundary);
  assert.throws(
    () =>
      validateSourcePage({ ...value, total: null }, scope, false, false, true),
    { code: "INVALID_RESPONSE" },
  );
  for (const mode of [
    "missing-first",
    "extra-field",
    "bad-hash",
    "upper-hash",
    "wrong-id",
    "extra-edge-field",
    "null-valid-edge",
  ]) {
    const broken = structuredClone(value);
    if (mode === "missing-first") delete broken.jmSearchBoundary.first;
    if (mode === "extra-field") broken.jmSearchBoundary.raw = "not allowed";
    if (mode === "bad-hash")
      broken.jmSearchBoundary.first.fingerprint = "short";
    if (mode === "upper-hash")
      broken.jmSearchBoundary.first.fingerprint = "A".repeat(64);
    if (mode === "wrong-id") broken.jmSearchBoundary.first.workId = "2";
    if (mode === "extra-edge-field")
      broken.jmSearchBoundary.first.raw = "not allowed";
    if (mode === "null-valid-edge") broken.jmSearchBoundary.first = null;
    assert.throws(
      () => validateSourcePage(broken, scope, false, false, true),
      { code: "INVALID_RESPONSE" },
      mode,
    );
  }
  for (const [favorite, accumulated] of [
    [true, false],
    [false, true],
  ]) {
    assert.throws(
      () => validateSourcePage(value, scope, favorite, accumulated, true),
      { code: "INVALID_RESPONSE" },
    );
  }
  const issues = [1, 3].map((index) => ({
    page: 1,
    index,
    workId: null,
    code: "SOURCE_ITEM_INVALID",
  }));
  const middle = page(1, [2], { total: 3, pages: 1, issues });
  assert.deepEqual(
    validateSourcePage(middle, scope, false, false, true).jmSearchBoundary,
    { first: null, last: null },
  );
  middle.jmSearchBoundary.first = edge(middle.items[0]);
  assert.throws(() => validateSourcePage(middle, scope, false, false, true), {
    code: "INVALID_RESPONSE",
  });
  assert.deepEqual(
    validateSourcePage(
      page(1, [], { total: 0, pages: 0 }),
      scope,
      false,
      false,
      true,
    ).jmSearchBoundary,
    { first: null, last: null },
  );
  const single = page(1, [1], { total: 1, pages: 1 });
  single.jmSearchBoundary.last.fingerprint = "f".repeat(64);
  assert.throws(() => validateSourcePage(single, scope, false, false, true), {
    code: "INVALID_RESPONSE",
  });
  for (const kind of ["favorites", "detail", "ranking"]) {
    const adapter = createSourceAdapter({
      native: true,
      invoke: async () => value,
    });
    await assert.rejects(
      adapter.query(scope, {
        kind,
        query: "Synthetic",
        folderId: null,
        page: 1,
      }),
      { code: "INVALID_RESPONSE" },
    );
  }
  const picaScope = { source: "Pica", sessionId: "synthetic-pica-boundary" };
  const picaPage = {
    ...value,
    ...picaScope,
    items: [work(1, "Pica"), work(2, "Pica")],
  };
  assert.throws(
    () => validateSourcePage(picaPage, picaScope, false, false, true),
    { code: "INVALID_RESPONSE" },
  );
});

test("native JM recent pages accept raw boundaries, merge overlap and retain loaded works on a malformed next boundary", async () => {
  const pages = [
    page(1, [1, 2], { total: 4, pages: null }),
    page(2, [2, 3], { total: 4, pages: null }),
    page(3, [3, 4], { total: 4, pages: null }),
  ];
  pages[2].jmSearchBoundary.first.workId = "99";
  const calls = [];
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      assert.equal(command, "source_query");
      assert.equal(args.kind, "recent");
      assert.equal(args.query, "");
      assert.equal(args.folderId, null);
      assert.equal(args.reverse, undefined);
      calls.push(args.page);
      return structuredClone(pages[args.page - 1]);
    },
  });
  // Exercise the native query adapter and the browsing reader together; local
  // supplementary history has separate concurrency and failure regressions.
  const reader = new RecentUpdatesReader({ query: adapter.query }, scope);
  await reader.start();
  assert.equal(reader.state.phase, "ready");
  assert.deepEqual(
    reader.state.snapshot.items.map((item) => item.workId),
    ["1", "2"],
  );
  await reader.loadNext();
  assert.equal(reader.state.phase, "ready");
  assert.deepEqual(
    reader.state.snapshot.items.map((item) => item.workId),
    ["1", "2", "3"],
  );
  assert.equal(reader.state.snapshot.duplicates, 1);
  const retained = reader.state.snapshot;
  await reader.loadNext();
  assert.equal(reader.state.phase, "error");
  assert.equal(reader.state.error.code, "INVALID_RESPONSE");
  assert.equal(reader.state.snapshot, retained);
  assert.deepEqual(calls, [1, 2, 3]);
});

test("Pica search duplicates remain incomplete without the JM-only boundary exception", async () => {
  const picaScope = { source: "Pica", sessionId: "synthetic-pica-boundary" };
  const pages = [
    [1, 2],
    [2, 3],
  ].map((ids, index) => {
    const result = page(index + 1, ids, { total: 3, pages: 2 });
    delete result.jmSearchBoundary;
    return {
      ...result,
      ...picaScope,
      items: ids.map((id) => work(id, "Pica")),
    };
  });
  const seen = [];
  await assert.rejects(
    readCompleteSearch(
      adapterFor(pages),
      picaScope,
      "Synthetic Writer",
      readOptions(seen),
    ),
    { code: "SEARCH_INCOMPLETE" },
  );
  assert.ok(seen.every((value) => !value.complete));
});
