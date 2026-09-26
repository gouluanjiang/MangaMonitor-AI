import test from "node:test";
import assert from "node:assert/strict";
import {
  createCompletionAdapter,
  validateDiscoverySnapshot,
  validateDiscoveryProgress,
  unfinishedRangeMessage,
  authorCatalogAt,
} from "../src/completion-runtime.ts";
import { discoveryRecordLimit } from "../src/completion-types.ts";
import { createAuthorSearchAdapter } from "../src/author-search.ts";
import {
  readCompleteSearch,
  readCompleteAuthorSearch,
} from "../src/source-search.ts";
import { authorQueryError } from "../src/author-query.ts";
import { partitionAuthorRecords } from "../src/author-evidence.ts";
import { validateSourceWork } from "../src/source-runtime.ts";

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
const defaultAuthorPolicy = async (scope, author) => ({
  ...scope,
  revision: 0,
  author,
  queries: [author],
  verifiedAliases: [],
  exactCredits: [],
  queryFingerprint: "a".repeat(64),
});

const changeSummary = (overrides = {}) => ({
  id: "a".repeat(64),
  startedAt: 1000,
  finishedAt: 2000,
  phase: "complete",
  mode: "incremental",
  onlyUnfinished: false,
  firstCatalog: false,
  allFollowed: true,
  authorCount: 1,
  totalScopes: 2,
  attemptedScopes: 2,
  completeScopes: 2,
  ...overrides,
});

test("discovery summaries preserve legacy history and explicit first-discovery identities through a cold read", () => {
  const record = {
    work: work("JM", 1),
    matchedAuthors: ["Writer"],
    authorVerified: true,
    observedAt: 2000,
    scanId: "a".repeat(64),
  };
  const legacy = validateDiscoverySnapshot(
    { ...empty(), records: [record] },
    scopes,
  );
  assert.equal(legacy.lastCheck, null);
  assert.equal(legacy.records[0].firstDiscoveredRunId, undefined);
  const saved = {
    ...empty(),
    lastCheck: changeSummary(),
    records: [
      record,
      { ...record, work: work("JM", 2), firstDiscoveredRunId: "a".repeat(64) },
    ],
  };
  const restored = validateDiscoverySnapshot(
    JSON.parse(JSON.stringify(saved)),
    scopes,
  );
  assert.deepEqual(restored.lastCheck, saved.lastCheck);
  assert.equal(
    restored.records[0].firstDiscoveredRunId,
    undefined,
    "a fresh observation of historical metadata is not first discovery",
  );
  assert.equal(restored.records[1].firstDiscoveredRunId, saved.lastCheck.id);
  assert.equal(
    restored.run,
    null,
    "saved summaries survive without a live run",
  );
});

test("summary progress stays catalog-free and accepts explicit partial or interrupted coverage", () => {
  for (const phase of [
    "checking",
    "partial",
    "cancelled",
    "error",
    "interrupted",
  ]) {
    const summary = changeSummary({
      phase,
      finishedAt: null,
      completeScopes: 0,
      attemptedScopes: 1,
      onlyUnfinished: true,
    });
    const input = { ...empty(), recordCount: 400000, lastCheck: summary };
    Object.defineProperty(input, "records", {
      get() {
        throw Error("progress must not read the catalog");
      },
    });
    const parsed = validateDiscoveryProgress(input, scopes);
    assert.deepEqual(parsed.lastCheck, summary);
    assert.equal("records" in parsed, false);
  }
});

test("invalid change summary claims or first-discovery identifiers cannot replace current results", () => {
  for (const patch of [
    { id: "not-a-run" },
    { startedAt: -1 },
    { finishedAt: 999 },
    { mode: "automatic" },
    { phase: "idle" },
    { firstCatalog: "true" },
    { onlyUnfinished: 1 },
    { allFollowed: null },
    { authorCount: 0 },
    { authorCount: 2001 },
    { authorCount: 3 },
    { totalScopes: 3 },
    { totalScopes: 0 },
    { attemptedScopes: 3 },
    { completeScopes: 3 },
    { completeScopes: 1 },
    { finishedAt: null },
    { phase: "checking" },
  ]) {
    assert.throws(
      () =>
        validateDiscoverySnapshot(
          { ...empty(), lastCheck: changeSummary(patch) },
          scopes,
        ),
      (e) => e.code === "DISCOVERY_INVALID",
    );
  }
  const record = {
    work: work("JM", 1),
    matchedAuthors: ["Writer"],
    authorVerified: true,
    observedAt: 2000,
    scanId: "a".repeat(64),
  };
  for (const marker of ["", "a".repeat(65), "A".repeat(64), 123]) {
    assert.throws(
      () =>
        validateDiscoverySnapshot(
          {
            ...empty(),
            records: [{ ...record, firstDiscoveredRunId: marker }],
          },
          scopes,
        ),
      (e) => e.code === "DISCOVERY_INVALID",
    );
  }
});

test("catalog coverage follows current query baselines without deleting historical dates", async () => {
  const policy = {
    ...(await defaultAuthorPolicy(scopes[0], "Writer")),
    queries: ["Writer", "WriterAlias"],
  };
  const range = {
    author: "Writer",
    source: "JM",
    state: "partial",
    lastCompleteAt: 100,
    errorCode: "SOURCE_TIMEOUT",
  };
  assert.equal(
    authorCatalogAt(range),
    100,
    "legacy historical catalog remains known",
  );
  assert.equal(
    authorCatalogAt(range, [policy]),
    null,
    "changed query set is not covered by old time",
  );
  range.queryFingerprint = policy.queryFingerprint;
  range.queryBaselines = [
    { query: "Writer", baseline: { establishedAt: 100 } },
  ];
  assert.equal(authorCatalogAt(range, [policy]), null);
  range.queryBaselines.push({
    query: "WriterAlias",
    baseline: { establishedAt: 200 },
  });
  range.lastCompleteAt = null;
  assert.equal(
    authorCatalogAt(range, [policy]),
    100,
    "queries completed at different times establish catalog coverage",
  );
  assert.equal(
    authorCatalogAt(range, [{ ...policy, queryFingerprint: "b".repeat(64) }]),
    null,
  );
});

test("author search preserves raw source spellings, reads every term and deduplicates overlap only after traversal", async () => {
  const calls = [],
    seen = [],
    policies = [];
  const queries = ["Writer～ Name", "Writer Name"];
  await readCompleteAuthorSearch(
    {
      authorPolicy: async (scope, author) => ({
        ...(await defaultAuthorPolicy(scope, author)),
        queries,
        verifiedAliases: ["WriterName"],
      }),
      query: async (scope, request) => {
        calls.push([request.query, request.page]);
        const ids = request.query === queries[0] ? [1, 2] : [2, 3];
        return {
          ...scope,
          items: [work(scope.source, ids[request.page - 1])],
          page: request.page,
          total: 2,
          pages: 2,
          hasMore: request.page < 2,
          folders: [],
        };
      },
    },
    scopes[0],
    "Displayed Writer",
    {
      current: () => true,
      onPolicy: (policy) => policies.push(policy),
      onPage: (progress) => seen.push(progress),
    },
  );
  assert.equal(policies.length, 1);
  assert.deepEqual(calls, [
    [queries[0], 1],
    [queries[0], 2],
    [queries[1], 1],
    [queries[1], 2],
  ]);
  assert.deepEqual(
    seen.map((p) => p.complete),
    [false, false, false, true],
  );
  assert.deepEqual(
    seen.at(-1).items.map((item) => item.workId),
    ["1", "2", "3"],
  );
  assert.equal(
    seen.at(-1).page.total,
    null,
    "overlapping source totals are not an exact work total",
  );
  assert.equal(seen.at(-1).page.pages, null);
  assert.equal(seen.at(-1).queryIndex, 2);
  assert.equal(seen.at(-1).queryCount, 2);
});

test("a failed later query retains earlier results but never claims the full author range complete", async () => {
  const seen = [];
  await assert.rejects(
    readCompleteAuthorSearch(
      {
        authorPolicy: async (scope, author) => ({
          ...(await defaultAuthorPolicy(scope, author)),
          queries: ["Writer", "Other spelling"],
        }),
        query: async (scope, request) => {
          if (request.query === "Other spelling")
            throw new Error("synthetic source failure");
          return {
            ...scope,
            items: [work(scope.source, 1)],
            page: 1,
            total: 1,
            pages: 1,
            hasMore: false,
            folders: [],
          };
        },
      },
      scopes[0],
      "Writer",
      {
        current: () => true,
        onPolicy() {},
        onPage: (progress) => seen.push(progress),
      },
    ),
  );
  assert.equal(seen.length, 1);
  assert.equal(seen[0].complete, false);
  assert.deepEqual(
    seen[0].items.map((item) => item.workId),
    ["1"],
  );
});

test("a later query with missing author metadata cannot erase the same work's earlier explicit credit", async () => {
  const seen = [];
  await readCompleteAuthorSearch(
    {
      authorPolicy: async (scope, author) => ({
        ...(await defaultAuthorPolicy(scope, author)),
        queries: ["Writer", "Other spelling"],
      }),
      query: async (scope, request) => ({
        ...scope,
        items: [
          {
            ...work(scope.source, 1),
            authors: request.query === "Writer" ? ["Writer"] : [],
          },
        ],
        page: 1,
        total: 1,
        pages: 1,
        hasMore: false,
        folders: [],
      }),
    },
    scopes[0],
    "Writer",
    {
      current: () => true,
      onPolicy() {},
      onPage: (progress) => seen.push(progress),
    },
  );
  assert.equal(seen.at(-1).complete, true);
  assert.equal(seen.at(-1).items.length, 1);
  assert.deepEqual(seen.at(-1).items[0].authors, ["Writer"]);
});

test("cancel between complete query terms stops the next query and keeps the combined range incomplete", async () => {
  const calls = [],
    seen = [];
  let current = true;
  await readCompleteAuthorSearch(
    {
      authorPolicy: async (scope, author) => ({
        ...(await defaultAuthorPolicy(scope, author)),
        queries: ["Writer", "Other spelling"],
      }),
      query: async (scope, request) => {
        calls.push(request.query);
        return {
          ...scope,
          items: [work(scope.source, 1)],
          page: 1,
          total: 1,
          pages: 1,
          hasMore: false,
          folders: [],
        };
      },
    },
    scopes[0],
    "Writer",
    {
      current: () => current,
      onPolicy() {},
      onPage(progress) {
        seen.push(progress);
        current = false;
      },
    },
  );
  assert.deepEqual(calls, ["Writer"]);
  assert.equal(seen[0].complete, false);
});

test("isolated rows from multiple query terms preserve each original source position and query", async () => {
  const seen = [];
  await readCompleteAuthorSearch(
    {
      authorPolicy: async (scope, author) => ({
        ...(await defaultAuthorPolicy(scope, author)),
        queries: ["Writer", "Other spelling"],
      }),
      query: async (scope, request) => ({
        ...scope,
        items: [],
        page: 1,
        pages: 1,
        total: 1,
        hasMore: false,
        folders: [],
        issues: [
          { page: 1, index: 1, workId: null, code: "SOURCE_ITEM_INVALID" },
        ],
      }),
    },
    scopes[0],
    "Writer",
    {
      current: () => true,
      onPolicy() {},
      onPage: (progress) => seen.push(progress),
    },
  );
  assert.equal(
    seen.at(-1).complete,
    true,
    "pagination completion does not clear isolated issue evidence",
  );
  assert.deepEqual(
    seen.at(-1).issues.map((issue) => [issue.query, issue.page, issue.index]),
    [
      ["Writer", 1, 1],
      ["Other spelling", 1, 1],
    ],
  );
});

test("progress validates metadata without traversing catalog records and rejects replaced sessions", async () => {
  const raw = { ...empty(), recordCount: 400000, otherRecordCount: 350000 };
  Object.defineProperty(raw, "records", {
    get() {
      throw new Error("catalog must not be read");
    },
  });
  const progress = validateDiscoveryProgress(raw, scopes);
  assert.equal(progress.recordCount, 400000);
  assert.equal(progress.otherRecordCount, 350000);
  assert.equal("records" in progress, false);
  assert.throws(
    () =>
      validateDiscoveryProgress(
        {
          ...progress,
          scopes: [{ ...scopes[0], sessionId: "replaced" }, scopes[1]],
        },
        scopes,
      ),
    { code: "STALE_SESSION" },
  );
  for (const bad of [-1, 1.5, "100"])
    assert.throws(() =>
      validateDiscoveryProgress({ ...progress, recordCount: bad }, scopes),
    );
  const calls = [];
  const adapter = createCompletionAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return { ...empty(), recordCount: 0 };
    },
  });
  await adapter.progress(scopes);
  assert.deepEqual(calls, [
    { command: "discovery_progress", args: { scopes } },
  ]);
});

test("discovery contracts keep legacy baselines readable and carry per-query baselines and source-specific attribution policies", () => {
  const baseline = {
    queryVersion: 1,
    headIds: ["123"],
    total: 1,
    establishedAt: 10,
  };
  const range = {
    author: "Displayed Writer",
    source: "JM",
    state: "complete",
    lastAttemptAt: 10,
    lastCompleteAt: 10,
    lastCheckedAt: 10,
    lastCheckMode: "full",
    observedCount: 1,
    pagesRead: 1,
    errorCode: null,
    baseline,
  };
  const old = validateDiscoverySnapshot(
    { ...empty(), authors: [range] },
    scopes,
  );
  assert.deepEqual(old.authors[0].baseline, baseline);
  assert.deepEqual(old.authorPolicies, []);
  const policy = {
    source: "JM",
    author: range.author,
    queries: ["Writer～"],
    verifiedAliases: ["Writer"],
    exactCredits: ["WriterCollaborator"],
    queryFingerprint: "a".repeat(64),
  };
  const current = validateDiscoverySnapshot(
    {
      ...empty(),
      authorPolicies: [policy],
      authors: [
        {
          ...range,
          queryFingerprint: policy.queryFingerprint,
          queryBaselines: [{ query: "Writer～", baseline }],
        },
      ],
    },
    scopes,
  );
  assert.deepEqual(current.authorPolicies, [policy]);
  assert.equal(current.authors[0].queryFingerprint, policy.queryFingerprint);
  assert.deepEqual(current.authors[0].queryBaselines, [
    { query: "Writer～", baseline },
  ]);
  for (const changed of [
    { queryFingerprint: "invalid" },
    { queryFingerprint: null },
    { queryBaselines: [{ query: "", baseline }] },
    {
      queryBaselines: [
        { query: "Writer～", baseline },
        { query: "Writer～", baseline },
      ],
    },
  ])
    assert.throws(() =>
      validateDiscoverySnapshot(
        { ...current, authors: [{ ...current.authors[0], ...changed }] },
        scopes,
      ),
    );
  const partial = validateDiscoverySnapshot(
    {
      ...current,
      authors: [
        {
          ...current.authors[0],
          state: "partial",
          baseline: null,
          errorCode: "SOURCE_ITEMS_PARTIAL",
          pagesRead: 2,
          issueCount: 1,
          pagesComplete: true,
          issueSamples: [
            {
              query: "Another spelling",
              page: 1,
              index: 1,
              workId: null,
              code: "SOURCE_ITEM_INVALID",
            },
          ],
        },
      ],
    },
    scopes,
  );
  assert.equal(partial.authors[0].state, "partial");
  assert.deepEqual(
    partial.authors[0].queryBaselines,
    [{ query: "Writer～", baseline }],
    "a clean term can retain its baseline while another term remains unresolved",
  );
});

test("cold keyword results require an explicit local read and legacy snapshots retain their complete records", async () => {
  assert.equal(validateDiscoverySnapshot(empty(), scopes).includesOther, true);
  const saved = {
    ...empty(),
    records: [],
    includesOther: false,
    otherRecordCount: 65000,
  };
  assert.equal(
    validateDiscoverySnapshot(saved, scopes).otherRecordCount,
    65000,
  );
  assert.throws(() =>
    validateDiscoverySnapshot({ ...saved, includesOther: "false" }, scopes),
  );
  const calls = [];
  const adapter = createCompletionAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return { ...saved, includesOther: args.includeOther };
    },
  });
  await adapter.read(scopes);
  await adapter.read(scopes, true);
  assert.deepEqual(calls, [
    { command: "discovery_read", args: { scopes, includeOther: false } },
    { command: "discovery_read", args: { scopes, includeOther: true } },
  ]);
});

test("unfinished checks have their own explicit command and keep author scope selection", async () => {
  const calls = [];
  const adapter = createCompletionAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return {
        runId: "unfinished",
        snapshot: {
          ...empty(),
          run: {
            id: "unfinished",
            phase: "checking",
            currentAuthor: null,
            currentSource: null,
            currentPage: 0,
            requestsUsed: 0,
            completedScopes: 0,
            totalScopes: 1,
            errorCode: null,
          },
        },
      };
    },
  });
  await adapter.startUnfinished(scopes, ["Writer"]);
  assert.deepEqual(calls, [
    {
      command: "discovery_start_unfinished",
      args: { scopes, authors: ["Writer"] },
    },
  ]);
  const snapshot = await adapter.startUnfinished(scopes, ["Writer"]);
  assert.equal(snapshot.run.storageWarningCode, null);
  assert.equal(
    validateDiscoverySnapshot(
      {
        ...snapshot,
        run: {
          ...snapshot.run,
          storageWarningCode: "DISCOVERY_CHECKPOINT_FAILED",
        },
      },
      scopes,
    ).run.storageWarningCode,
    "DISCOVERY_CHECKPOINT_FAILED",
  );
  for (const bad of ["x".repeat(200), "bad code", 1])
    assert.throws(() =>
      validateDiscoverySnapshot(
        { ...snapshot, run: { ...snapshot.run, storageWarningCode: bad } },
        scopes,
      ),
    );
});

test("unfinished ranges distinguish pending work, interruption and actual source failures", () => {
  const range = {
    author: "Writer",
    source: "JM",
    state: "partial",
    lastAttemptAt: 1,
    lastCompleteAt: null,
    observedCount: 0,
    pagesRead: 0,
    errorCode: null,
  };
  for (const [errorCode, text] of [
    ["SOURCE_CONNECTION_FAILED", "连接失败"],
    ["SOURCE_TIMEOUT", "超时"],
    ["SOURCE_REQUEST_FAILED", "请求失败"],
    ["SOURCE_RESPONSE_INVALID", "响应格式异常"],
    ["DISCOVERY_PAGINATION_CHANGED", "分页结果发生变化"],
    ["DISCOVERY_LIMIT", "保存上限"],
    ["DISCOVERY_INTERRUPTED", "上次检查中断"],
  ])
    assert.ok(unfinishedRangeMessage({ ...range, errorCode }).includes(text));
  assert.equal(
    unfinishedRangeMessage({ ...range, state: "idle" }),
    "尚未开始检查",
  );
  assert.equal(
    unfinishedRangeMessage({ ...range, state: "checking" }),
    "正在读取",
  );
});

test("author query eligibility rejects placeholder and broad initials without rejecting real short names", () => {
  for (const name of ["N/A", " n/a ", "Ｎ／Ａ", "unknown", "作者不詳"])
    assert.equal(authorQueryError(name), "AUTHOR_QUERY_PLACEHOLDER");
  for (const name of ["P", "p", "Ｐ", "7", " ７ "])
    assert.equal(authorQueryError(name), "AUTHOR_QUERY_TOO_BROAD");
  for (const name of [
    "森",
    "K",
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
    authorPolicy: defaultAuthorPolicy,
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

test("saved issue diagnostics validate sampling, identity, ordering and incomplete evidence", () => {
  const issue = {
    page: 1,
    index: 1,
    workId: "1",
    code: "SOURCE_ITEM_METADATA_MISSING",
  };
  const range = {
    author: "Writer",
    source: "JM",
    state: "partial",
    lastAttemptAt: 10,
    lastCompleteAt: null,
    observedCount: 1,
    pagesRead: 1,
    errorCode: "SOURCE_ITEMS_PARTIAL",
    issueCount: 1,
    issueSamples: [issue],
    pagesComplete: true,
  };
  const parsed = validateDiscoverySnapshot(
    { ...empty(), authors: [range] },
    scopes,
  ).authors[0];
  assert.equal(parsed.issueCount, 1);
  assert.equal(
    unfinishedRangeMessage(parsed),
    "分页已读完，1 条来源记录待核对",
  );
  for (const change of [
    { state: "complete" },
    { issueCount: 0 },
    { issueSamples: [] },
    { pagesRead: 0 },
    { issueSamples: [{ ...issue, workId: null }] },
    { issueSamples: [{ ...issue, raw: "private" }] },
    { issueCount: 2, issueSamples: [{ ...issue, index: 2 }, issue] },
    { issueSamples: [{ ...issue, workId: "abc" }] },
    { issueCount: 100001 },
    {
      baseline: { queryVersion: 1, headIds: ["1"], total: 1, establishedAt: 1 },
    },
  ])
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

test("a bad later-page record is isolated while every valid author work is preserved", async () => {
  const calls = [],
    pageSizes = [];
  const adapter = createAuthorSearchAdapter({
    authorPolicy: defaultAuthorPolicy,
    query: async (scope, query) => {
      calls.push([scope.source, query.page]);
      const count = scope.source === "JM" ? (query.page === 1 ? 80 : 70) : 0;
      const items = Array.from({ length: count }, (_, index) => {
        const id = (query.page - 1) * 80 + index + 1;
        const item = work(scope.source, id);
        return validateSourceWork(item);
      }).filter((item) => item.workId !== "98");
      pageSizes.push(items.length);
      return {
        ...scope,
        page: query.page,
        pages: scope.source === "JM" ? 2 : 1,
        total: scope.source === "JM" ? 150 : 0,
        hasMore: scope.source === "JM" && query.page === 1,
        folders: [],
        items,
        issues:
          scope.source === "JM" && query.page === 2
            ? [
                {
                  page: 2,
                  index: 18,
                  workId: "98",
                  code: "SOURCE_ITEM_METADATA_MISSING",
                },
              ]
            : [],
      };
    },
  });
  await adapter.start(scopes, ["Writer"]);
  for (
    let attempt = 0;
    attempt < 10 && (await adapter.read(scopes)).run.phase === "checking";
    attempt++
  )
    await flush();
  const result = validateDiscoverySnapshot(await adapter.read(scopes), scopes);
  assert.equal(result.run.phase, "partial");
  assert.deepEqual(calls, [
    ["JM", 1],
    ["JM", 2],
    ["Pica", 1],
  ]);
  assert.deepEqual(pageSizes, [80, 69, 0]);
  assert.equal(result.records.length, 149);
  assert.equal(
    result.authors.find((range) => range.source === "JM").observedCount,
    149,
  );
  assert.equal(
    result.authors.find((range) => range.source === "JM").pagesRead,
    2,
  );
  const partition = partitionAuthorRecords(result.records, "Writer", "JM");
  assert.equal(partition.confirmed.length, 149);
  assert.equal(partition.other.length, 0);
  const range = result.authors.find((range) => range.source === "JM");
  assert.equal(range.issueCount, 1);
  assert.equal(range.pagesComplete, true);
  assert.equal(range.state, "partial");
  assert.equal(range.lastCompleteAt, null);
  assert.equal(range.errorCode, "SOURCE_ITEMS_PARTIAL");
  assert.deepEqual(range.issueSamples, [
    { page: 2, index: 18, workId: "98", code: "SOURCE_ITEM_METADATA_MISSING" },
  ]);
});

test("search traverses all-issue pages and retains diagnostics when resuming", async () => {
  const seen = [],
    calls = [];
  const adapter = {
    query: async (scope, query) => {
      calls.push(query.page);
      return {
        ...scope,
        page: query.page,
        total: 3,
        pages: 3,
        hasMore: query.page < 3,
        folders: [],
        items: query.page === 2 ? [] : [work(scope.source, query.page)],
        issues:
          query.page === 2
            ? [{ page: 2, index: 1, workId: null, code: "SOURCE_ITEM_INVALID" }]
            : [],
      };
    },
  };
  let keepReading = true;
  await readCompleteSearch(adapter, scopes[0], "Writer", {
    current: () => keepReading,
    onPage(value) {
      seen.push(value);
      if (value.page.page === 2) keepReading = false;
    },
  });
  assert.deepEqual(calls, [1, 2]);
  const paused = seen.at(-1);
  assert.equal(paused.recordsRead, 2);
  await readCompleteSearch(adapter, scopes[0], "Writer", {
    current: () => true,
    fromPage: 3,
    items: paused.items,
    issues: paused.issues,
    recordsRead: paused.recordsRead,
    onPage: (value) => seen.push(value),
  });
  assert.deepEqual(calls, [1, 2, 3]);
  assert.equal(seen.at(-1).complete, true);
  assert.equal(seen.at(-1).recordsRead, 3);
  assert.equal(seen.at(-1).items.length, 2);
  assert.equal(seen.at(-1).issues.length, 1);
});

test("a new isolated row cannot disguise repeated normal identities on a later search page", async () => {
  const seen = [];
  await assert.rejects(
    readCompleteSearch(
      {
        query: async (scope, query) => ({
          ...scope,
          items: [work(scope.source, 1)],
          issues:
            query.page === 2
              ? [
                  {
                    page: 2,
                    index: 2,
                    workId: "2",
                    code: "SOURCE_ITEM_INVALID",
                  },
                ]
              : [],
          page: query.page,
          total: 3,
          pages: 2,
          hasMore: query.page === 1,
          folders: [],
        }),
      },
      scopes[0],
      "Writer",
      { current: () => true, onPage: (value) => seen.push(value) },
    ),
    { code: "SEARCH_INCOMPLETE" },
  );
  assert.equal(seen.at(-1).complete, false);
  assert.equal(seen.at(-1).items.length, 1);
});

test("ad-hoc author lookup needs no following and visits both sources even when one fails", async () => {
  const calls = [];
  const adapter = createAuthorSearchAdapter({
    authorPolicy: defaultAuthorPolicy,
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
    authorPolicy: defaultAuthorPolicy,
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
  await flush();
  await adapter.cancel(started.run.id);
  release();
  await flush();
  assert.equal(count, 1);
  assert.equal((await adapter.read(scopes)).records.length, 0);
  await adapter.start(scopes, ["Writer"]);
  await flush();
  const newScopes = [{ ...scopes[0], sessionId: "new-account" }, scopes[1]];
  await adapter.read(newScopes);
  release();
  await flush();
  assert.equal((await adapter.read(newScopes)).records.length, 0);
  assert.equal(count, 2);
});
