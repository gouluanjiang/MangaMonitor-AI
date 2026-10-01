import test from "node:test";
import assert from "node:assert/strict";
import { createAuthorSearchAdapter } from "../src/author-search.ts";
import { AuthorSearchScheduler } from "../src/author-search-scheduler.ts";
import { SourceError } from "../src/source-runtime.ts";

const scopes = [
  { source: "JM", sessionId: "jm-test" },
  { source: "Pica", sessionId: "pica-test" },
];
const flush = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};
const work = (scope, n, extra = {}) => ({
  source: scope.source,
  workId: scope.source === "JM" ? String(n) : n.toString(16).padStart(24, "0"),
  title: `Synthetic ${n}`,
  authors: ["Writer"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
  ...extra,
});
const policy = async (scope, author) => ({
  ...scope,
  revision: 0,
  author,
  queries: [author],
  verifiedAliases: [],
  exactCredits: [],
  queryFingerprint: "a".repeat(64),
});
const page = (scope, items) => ({
  ...scope,
  items,
  page: 1,
  pages: 1,
  total: items.length,
  hasMore: false,
  folders: [],
  timing: { queueMs: 2, sourceOperationMs: 5, localCommitMs: 1 },
});

test("cached results publish before either source finishes; parallel sources retain metadata and failed refresh data", async () => {
  const gates = [deferred(), deferred()];
  let fail = false;
  const called = [],
    revisions = [];
  const adapter = createAuthorSearchAdapter(
    {
      authorPolicy: policy,
      knownAuthorWorks: async (scope) => ({
        ...scope,
        items: [work(scope, 1)],
        checkedAt: 1000,
        historyComplete: true,
      }),
      query: async (scope) => {
        called.push(scope.source);
        await gates[scope.source === "JM" ? 0 : 1].promise;
        if (fail) throw new SourceError("SOURCE_TIMEOUT");
        return page(scope, [
          work(scope, 1, { title: "Updated metadata", tags: ["中文"] }),
          work(scope, 2),
        ]);
      },
    },
    new AuthorSearchScheduler(),
  );
  adapter.subscribe(() => revisions.push(1));
  const start = await adapter.start(scopes, ["Writer"]);
  await adapter.start(scopes, ["Writer"]);
  await flush();
  assert.equal(
    called.length,
    2,
    "a duplicate click must not schedule another search",
  );
  let result = await adapter.read(scopes);
  assert.equal(result.records.length, 2);
  assert.equal(result.run.phase, "checking");
  assert.ok(result.searchMetrics.firstRecordsMs !== null);
  assert.equal(result.searchMetrics.completedMs, null);
  gates[0].resolve();
  await flush();
  result = await adapter.read(scopes);
  assert.equal(result.run.phase, "checking");
  assert.equal(
    result.records.find((r) => r.work.source === "JM").work.title,
    "Updated metadata",
  );
  gates[1].resolve();
  await flush();
  result = await adapter.read(scopes);
  assert.equal(result.run.phase, "complete");
  assert.equal(result.records.length, 4);
  assert.equal(result.searchMetrics.sourceOperationMs, 10);
  assert.ok(revisions.length >= 3);
  fail = true;
  const next = await adapter.start(scopes, ["Writer"]);
  assert.equal(
    adapter.rendered(start.run.id, result.revision, 1, true),
    null,
    "late paint from an older run is ignored",
  );
  await flush();
  result = await adapter.read(scopes);
  assert.equal(result.run.phase, "partial");
  assert.equal(result.records.length, 4);
  assert.ok(
    adapter.rendered(next.run.id, result.revision, 1, true).firstVisibleMs !==
      null,
  );
});

test("closing a tab and changing account generations discard late responses without recreating results", async () => {
  for (const close of [true, false]) {
    const gate = deferred();
    let requests = 0;
    const adapter = createAuthorSearchAdapter(
      {
        authorPolicy: policy,
        query: async (scope) => {
          requests++;
          await gate.promise;
          return page(scope, [work(scope, 1)]);
        },
      },
      new AuthorSearchScheduler(),
    );
    await adapter.start(scopes, ["Writer"]);
    await flush();
    const nextScopes = close
      ? []
      : [{ ...scopes[0], sessionId: "another-account" }, scopes[1]];
    await adapter.read(nextScopes);
    gate.resolve();
    await flush();
    assert.equal((await adapter.read(nextScopes)).records.length, 0);
    assert.equal(requests, 2);
  }
});

test("source scheduler bounds concurrency, foreground wins the next slot and invalidated queues never execute", async () => {
  const scheduler = new AuthorSearchScheduler(),
    gates = [deferred(), deferred()];
  const started = [];
  let valid = true;
  const first = scheduler.run(
    "JM",
    () => false,
    () => true,
    async () => {
      started.push("background-running");
      await gates[0].promise;
    },
  );
  const background = scheduler.run(
    "Pica",
    () => false,
    () => true,
    async () => {
      started.push("background-next");
    },
  );
  const foreground = scheduler.run(
    "Pica",
    () => true,
    () => true,
    async () => {
      started.push("foreground");
      await gates[1].promise;
    },
  );
  const stale = scheduler.run(
    "JM",
    () => true,
    () => valid,
    async () => {
      started.push("stale");
    },
  );
  const rejected = assert.rejects(stale, { code: "SEARCH_CANCELLED" });
  valid = false;
  scheduler.wake();
  await rejected;
  assert.deepEqual(started, ["background-running", "foreground"]);
  gates[1].resolve();
  await foreground;
  await flush();
  assert.equal(started.length, 2, "at most one background request may run");
  gates[0].resolve();
  await Promise.all([first, background]);
  assert.deepEqual(started, [
    "background-running",
    "foreground",
    "background-next",
  ]);
});
