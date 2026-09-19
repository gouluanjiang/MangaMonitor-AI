import test from "node:test";
import assert from "node:assert/strict";
import {
  createCompletionAdapter,
  validateDiscoverySnapshot,
} from "../src/completion-runtime.ts";
import { discoveryRecordLimit } from "../src/completion-types.ts";
import { createAuthorSearchAdapter } from "../src/author-search.ts";
import { readCompleteSearch } from "../src/source-search.ts";
import { authorQueryError } from "../src/author-query.ts";

const scopes = [
  { source: "JM", sessionId: "synthetic-jm" },
  { source: "Pica", sessionId: "synthetic-pica" },
];
const work = (source, n) => ({
  source,
  workId: source === "JM" ? String(n) : n.toString(16).padStart(24, "0"),
  title: "Synthetic " + n,
  authors: ["Writer"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});
const empty = () => ({
  scopes,
  revision: 0,
  authors: [],
  records: [],
  run: null,
});
const flush = () => new Promise((resolve) => setImmediate(resolve));

test("author query eligibility rejects placeholder and broad initials without rejecting real short names", () => {
  for (const name of ["N/A", " n/a ", "Ｎ／Ａ", "unknown", "作者不詳"])
    assert.equal(authorQueryError(name), "AUTHOR_QUERY_PLACEHOLDER");
  for (const name of ["P", "p", "Ｐ", "7", " ７ "])
    assert.equal(authorQueryError(name), "AUTHOR_QUERY_TOO_BROAD");
  for (const name of [
    "森",
    "あ",
    "AB",
    "NA",
    "Unknown Artist",
    "Example Circle (P)",
  ])
    assert.equal(authorQueryError(name), null);
});

test("ad-hoc author search blocks broad queries before IO and preserves the previous complete result", async () => {
  const calls = [];
  const adapter = createAuthorSearchAdapter({
    query: async (scope, query) => {
      calls.push(query.query);
      return {
        ...scope,
        items: [work(scope.source, 1)],
        page: 1,
        pages: 1,
        total: 1,
        hasMore: false,
        folders: [],
      };
    },
  });
  await adapter.start(scopes, ["森"]);
  await flush();
  const before = await adapter.read(scopes);
  assert.equal(before.run.phase, "complete");
  for (const name of ["N/A", "P", "Ｎ／Ａ", "Ｐ"])
    await assert.rejects(adapter.start(scopes, [name]), {
      code: authorQueryError(name),
    });
  assert.equal(calls.length, 2);
  assert.deepEqual(await adapter.read(scopes), before);
});

test("JM total-only pagination finishes exactly at the reported total without a spurious extra request", async () => {
  const calls = [],
    seen = [];
  await readCompleteSearch(
    {
      query: async (scope, query) => {
        calls.push(query.page);
        return {
          ...scope,
          items: [work(scope.source, query.page)],
          page: query.page,
          total: 2,
          pages: null,
          hasMore: null,
          folders: [],
        };
      },
    },
    scopes[0],
    "Writer",
    { current: () => true, onPage: (value) => seen.push(value) },
  );
  assert.deepEqual(calls, [1, 2]);
  assert.equal(seen.at(-1).complete, true);
});

test("opening author updates reads saved metadata only; explicit check invokes discovery without download authority", async () => {
  const calls = [];
  const adapter = createCompletionAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "discovery_start")
        return {
          runId: "scan-1",
          snapshot: {
            ...empty(),
            run: {
              id: "scan-1",
              phase: "checking",
              currentAuthor: null,
              currentSource: null,
              currentPage: 0,
              requestsUsed: 0,
              completedScopes: 0,
              totalScopes: 2,
              errorCode: null,
            },
          },
        };
      return empty();
    },
  });
  await adapter.read(scopes);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["discovery_read"],
  );
  await adapter.start(scopes, []);
  await adapter.cancel("scan-1");
  assert.deepEqual(
    calls.map((call) => call.command),
    ["discovery_read", "discovery_start", "discovery_cancel"],
  );
  assert.deepEqual(calls[1].args, {
    scopes,
    authors: [],
    mode: "incremental",
  });
  await adapter.start(scopes, ["Writer"], "full");
  assert.deepEqual(calls.at(-1).args, {
    scopes,
    authors: ["Writer"],
    mode: "full",
  });
  const beforeInvalid = calls.length;
  await assert.rejects(adapter.start(scopes, [], "unknown"));
  assert.equal(calls.length, beforeInvalid);
});

test("incremental scope markers retain the last full timestamp and reject malformed modes", () => {
  const range = {
    author: "Writer",
    source: "JM",
    state: "complete",
    lastAttemptAt: 200,
    lastCompleteAt: 100,
    observedCount: 10,
    pagesRead: 1,
    errorCode: null,
  };
  const legacy = validateDiscoverySnapshot(
    { ...empty(), authors: [range] },
    scopes,
  );
  assert.equal(legacy.authors[0].lastCheckedAt, 100);
  assert.equal(legacy.authors[0].lastCheckMode, null);
  const current = validateDiscoverySnapshot(
    {
      ...empty(),
      authors: [{ ...range, lastCheckedAt: 200, lastCheckMode: "incremental" }],
    },
    scopes,
  );
  assert.equal(current.authors[0].lastCompleteAt, 100);
  assert.equal(current.authors[0].lastCheckedAt, 200);
  assert.equal(current.authors[0].lastCheckMode, "incremental");
  for (const change of [{ lastCheckedAt: -1 }, { lastCheckMode: "complete" }])
    assert.throws(() =>
      validateDiscoverySnapshot(
        { ...empty(), authors: [{ ...range, ...change }] },
        scopes,
      ),
    );
});

test("catalog snapshots accept more than the former 20000 records without truncating and retain a hard upper bound", () => {
  const records = Array.from({ length: 20001 }, (_, index) => ({
    work: work("JM", index + 1),
    matchedAuthors: ["Writer"],
    authorVerified: true,
    observedAt: 100,
    scanId: "catalog",
  }));
  const snapshot = validateDiscoverySnapshot({ ...empty(), records }, scopes);
  assert.equal(snapshot.records.length, 20001);
  assert.equal(snapshot.records.at(-1).work.workId, "20001");
  assert.throws(() =>
    validateDiscoverySnapshot(
      { ...empty(), records: Array(discoveryRecordLimit + 1).fill(records[0]) },
      scopes,
    ),
  );
});

test("saved incremental checkpoints validate their bounded head and version", () => {
  const baseline = {
    queryVersion: 1,
    headIds: ["1", "2"],
    total: 2,
    establishedAt: 100,
  };
  const range = {
    author: "Writer",
    source: "JM",
    state: "complete",
    lastAttemptAt: 200,
    lastCompleteAt: 100,
    lastCheckedAt: 200,
    lastCheckMode: "incremental",
    observedCount: 10,
    pagesRead: 1,
    errorCode: null,
    baseline,
  };
  assert.deepEqual(
    validateDiscoverySnapshot({ ...empty(), authors: [range] }, scopes)
      .authors[0].baseline,
    baseline,
  );
  for (const change of [
    { queryVersion: 0 },
    { total: -1 },
    { total: 10 },
    { headIds: ["1", "1"] },
    { headIds: Array.from({ length: 21 }, (_, i) => String(i + 1)) },
  ])
    assert.throws(() =>
      validateDiscoverySnapshot(
        {
          ...empty(),
          authors: [{ ...range, baseline: { ...baseline, ...change } }],
        },
        scopes,
      ),
    );
});

test("old session or malformed discovery snapshots cannot replace current results", () => {
  assert.throws(() =>
    validateDiscoverySnapshot(
      { ...empty(), scopes: [{ ...scopes[0], sessionId: "old" }, scopes[1]] },
      scopes,
    ),
  );
  assert.throws(() =>
    validateDiscoverySnapshot(
      {
        ...empty(),
        records: [
          {
            work: work("JM", 1),
            matchedAuthors: [],
            authorVerified: true,
            observedAt: -1,
            scanId: "x",
          },
        ],
      },
      scopes,
    ),
  );
  assert.throws(() =>
    validateDiscoverySnapshot(
      { ...empty(), scopes: [scopes[0], scopes[0]] },
      scopes,
    ),
  );
});

test("complete search reads all 1200 results sequentially and gives final counts only after the final page", async () => {
  const calls = [],
    seen = [];
  let active = 0,
    maxActive = 0;
  const adapter = {
    query: async (scope, query) => {
      active++;
      maxActive = Math.max(maxActive, active);
      await flush();
      active--;
      calls.push(query.page);
      return {
        ...scope,
        page: query.page,
        pages: 60,
        total: 1200,
        hasMore: query.page < 60,
        folders: [],
        items: Array.from({ length: 20 }, (_, i) =>
          work(scope.source, (query.page - 1) * 20 + i + 1),
        ),
      };
    },
  };
  await readCompleteSearch(adapter, scopes[0], "Writer", {
    current: () => true,
    onPage: (value) => seen.push(value),
  });
  assert.equal(maxActive, 1);
  assert.equal(calls.length, 60);
  assert.equal(seen.at(-1).items.length, 1200);
  assert.equal(seen.at(-1).complete, true);
  assert.ok(seen.slice(0, -1).every((value) => !value.complete));
});

test("short, repeated and failed pages retain partial data and never report full coverage", async () => {
  for (const mode of ["short", "repeat", "network"]) {
    const seen = [];
    const adapter = {
      query: async (scope, query) => {
        if (mode === "network" && query.page === 2)
          throw new Error("synthetic");
        return {
          ...scope,
          page: query.page,
          pages: null,
          total: 3,
          hasMore: query.page < 2,
          folders: [],
          items: [work(scope.source, mode === "repeat" ? 1 : query.page)],
        };
      },
    };
    await assert.rejects(
      readCompleteSearch(adapter, scopes[0], "Writer", {
        current: () => true,
        onPage: (value) => seen.push(value),
      }),
    );
    assert.ok(seen.length);
    assert.ok(seen.every((value) => !value.complete));
  }
});

test("ad-hoc author lookup needs no following and visits both sources even when one fails", async () => {
  const calls = [];
  const adapter = createAuthorSearchAdapter({
    query: async (scope, query) => {
      calls.push([scope.source, query.page]);
      if (scope.source === "JM" && query.page === 2)
        throw new Error("synthetic");
      return {
        ...scope,
        page: query.page,
        pages: 2,
        total: 2,
        hasMore: query.page < 2,
        folders: [],
        items: [work(scope.source, query.page)],
      };
    },
  });
  await adapter.read(scopes);
  assert.equal(calls.length, 0);
  const start = await adapter.start(scopes, ["New Writer"]);
  assert.equal(start.run.phase, "checking");
  assert.equal(start.run.mode, "full");
  assert.equal(start.run.currentStrategy, "full");
  for (
    let attempt = 0;
    attempt < 10 && (await adapter.read(scopes)).run.phase === "checking";
    attempt++
  )
    await flush();
  const result = await adapter.read(scopes);
  assert.deepEqual(calls, [
    ["JM", 1],
    ["JM", 2],
    ["Pica", 1],
    ["Pica", 2],
  ]);
  assert.equal(result.run.phase, "partial");
  assert.equal(result.records.length, 3);
  assert.equal(
    result.authors.find((range) => range.source === "JM").state,
    "error",
  );
});

test("cancellation and account changes reject late search responses and stop further requests", async () => {
  let release,
    count = 0;
  const adapter = createAuthorSearchAdapter({
    query: async (scope, query) => {
      count++;
      await new Promise((resolve) => {
        release = resolve;
      });
      return {
        ...scope,
        page: query.page,
        pages: 3,
        total: 3,
        hasMore: true,
        folders: [],
        items: [work(scope.source, query.page)],
      };
    },
  });
  const started = await adapter.start(scopes, ["Writer"]);
  await adapter.cancel(started.run.id);
  release();
  await flush();
  assert.equal(count, 1);
  assert.equal((await adapter.read(scopes)).records.length, 0);
  await adapter.start(scopes, ["Writer"]);
  const newScopes = [{ ...scopes[0], sessionId: "new-account" }, scopes[1]];
  await adapter.read(newScopes);
  release();
  await flush();
  assert.equal((await adapter.read(newScopes)).records.length, 0);
  assert.equal(count, 2);
});
