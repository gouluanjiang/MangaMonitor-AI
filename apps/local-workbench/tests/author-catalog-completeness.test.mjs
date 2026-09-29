import test from "node:test";
import assert from "node:assert/strict";
import {
  createAuthorMembershipProjector,
  partitionAuthorRecords,
  workHasAuthor,
} from "../src/author-evidence.ts";
import { createAuthorCatalogIndex } from "../src/author-catalog-index.ts";
import { subscribeAuthorCatalogChanges } from "../src/author-catalog-events.ts";
import { createSourceAdapter } from "../src/source-runtime.ts";
import { readCompleteAuthorSearch } from "../src/source-search.ts";
import { RecentUpdatesReader } from "../src/recent-updates.ts";
import {
  isContentHidden,
  retainedContentTags,
  inheritContentTags,
  isOutsideJmAuthorScope,
} from "../src/content-filter.ts";

const jm = { source: "JM", sessionId: "synthetic-completeness-jm" };
const pica = { source: "Pica", sessionId: "synthetic-completeness-pica" };
const work = (id, authors = ["Writer"], extra = {}) => ({
  source: "JM",
  workId: String(id),
  title: "Synthetic work",
  authors,
  description: null,
  tags: [],
  categories: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
  ...extra,
});
const record = (id, authors, queries = []) => ({
  work: work(id, authors),
  matchedAuthors: queries,
  authorVerified: false,
  observedAt: 1,
  scanId: "synthetic-query",
});
const snapshot = (revision, records, extra = {}) => ({
  scopes: [jm, pica],
  revision,
  followingRevision: 1,
  policyRevision: 1,
  followedAuthors: ["Writer", "Coauthor"],
  authors: [],
  records,
  run: null,
  ...extra,
});
const page = (number, items, extra = {}) => ({
  ...jm,
  page: number,
  items,
  total: 2,
  pages: 2,
  hasMore: number < 2,
  folders: [],
  ...extra,
});
const policy = {
  ...jm,
  revision: 1,
  author: "Writer",
  queries: ["Writer"],
  verifiedAliases: [],
  queryFingerprint: "a".repeat(64),
};

test("current credited authors receive records found under removed queries or other entry points", () => {
  const records = [
    record(101, ["Writer、Coauthor"], ["Removed query"]),
    record(102, ["Studio (Writer)"], []),
    record(103, ["Different Writer"], ["Writer"]),
    record(104, [], ["Writer"]),
  ];
  const before = structuredClone(records);
  const all = partitionAuthorRecords(
    records,
    "",
    "all",
    [],
    ["Writer", "Coauthor"],
  );
  assert.deepEqual(
    all.confirmed.map((item) => item.work.workId),
    ["101", "102"],
  );
  assert.deepEqual(
    all.other.map((item) => item.work.workId),
    ["103", "104"],
  );
  assert.deepEqual(
    partitionAuthorRecords(
      records,
      "Coauthor",
      "JM",
      [],
      ["Writer", "Coauthor"],
    ).confirmed.map((item) => item.work.workId),
    ["101"],
  );
  assert.deepEqual(
    records,
    before,
    "projection must not rewrite discovery provenance or ownership receipts",
  );
});

test("indexed signatures retain exact aliases, group members and explicit credits without substring guesses", () => {
  const rules = [
    { ...policy, verifiedAliases: ["Alias"], exactCredits: ["Signed credit"] },
  ];
  const project = createAuthorMembershipProjector(
    ["Writer", "Group (Member)", "P", "N/A"],
    rules,
  );
  for (const authors of [
    ["Writer"],
    ["Group (Writer)"],
    ["Alias"],
    ["Signed credit"],
    ["Writer suffix"],
    ["Other"],
  ]) {
    assert.equal(
      project(work(1, authors)).authors.includes("Writer"),
      workHasAuthor(work(1, authors), "Writer", rules[0]),
    );
  }
  assert.deepEqual(project(work(1, ["Member"])).authors, ["Group (Member)"]);
  assert.deepEqual(project(work(1, ["Group"])).authors, []);
  assert.deepEqual(
    project(work(1, ["P"])).authors,
    ["P"],
    "a broad remote query is not a veto on a known exact signature",
  );
  assert.deepEqual(
    project(work(1, ["N/A"])).authors,
    [],
    "missing-author placeholders are not identities",
  );
});

test("catalog membership rejects stale follow/rule generations and clears hiding until durable reread", () => {
  const index = createAuthorCatalogIndex();
  index.remember(snapshot(10, [record(101, ["Writer"], [])]));
  assert.deepEqual([...index.read([jm])], ["JM:101"]);
  index.invalidate({ ...jm, revision: 11, followingRevision: 2 });
  assert.equal(
    index.read([jm]).size,
    0,
    "invalidation must never guess that an incoming work was committed",
  );
  assert.equal(
    index.remember(snapshot(10, [record(101, ["Writer"])], { scopes: [jm] })),
    false,
  );
  assert.equal(
    index.remember(
      snapshot(11, [record(101, ["Writer"])], {
        scopes: [jm],
        followingRevision: 2,
        followedAuthors: ["Coauthor"],
      }),
    ),
    true,
  );
  assert.equal(index.read([jm]).size, 0);
  index.invalidate({ ...jm, policyRevision: 3 });
  assert.equal(
    index.remember(
      snapshot(12, [record(101, ["Writer"])], {
        scopes: [jm],
        followingRevision: 2,
        policyRevision: 2,
      }),
    ),
    false,
  );
  assert.equal(index.read([{ ...jm, sessionId: "another-account" }]).size, 0);
});

test("author query supplementation does not consume raw pagination totals or lose a known old work", async () => {
  const calls = [],
    progress = [];
  const adapter = {
    authorPolicy: async () => policy,
    knownAuthorWorks: async () => ({
      ...jm,
      items: [work(101), work(103), work(104, ["Other"])],
      checkedAt: 9,
      discoveryRevision: 20,
    }),
    query: async (_scope, query) => {
      calls.push(query);
      return page(query.page, [work(query.page === 1 ? 101 : 102)]);
    },
  };
  await readCompleteAuthorSearch(adapter, jm, "Writer", {
    current: () => true,
    onPage: (value) => progress.push(value),
  });
  assert.deepEqual(
    calls.map((query) => [query.kind, query.page]),
    [
      ["author", 1],
      ["author", 2],
    ],
  );
  const final = progress.at(-1);
  assert.equal(final.complete, true);
  assert.equal(final.recordsRead, 2);
  assert.equal(final.page.total, 2);
  assert.deepEqual(
    final.items.map((item) => item.workId),
    ["101", "102"],
  );
  assert.deepEqual(
    final.historicalItems.map((item) => item.workId),
    ["103"],
  );
  assert.equal(final.historicalReadAt, 9);
});

test("partial history reread preserves previously returned works and exposes observation failure", async () => {
  let reads = 0;
  const progress = [];
  const source = createSourceAdapter({
    native: true,
    invoke: async () => ({
      ...jm,
      items: ++reads === 1 ? [work(103)] : [],
      checkedAt: 9,
      discoveryRevision: 20,
      historyComplete: reads === 1,
      observationErrorCode: reads === 1 ? null : "STORAGE_UNAVAILABLE",
    }),
  });
  await readCompleteAuthorSearch(
    {
      authorPolicy: async () => policy,
      knownAuthorWorks: source.knownAuthorWorks,
      query: async () =>
        page(1, [work(101)], { total: 1, pages: 1, hasMore: false }),
    },
    jm,
    "Writer",
    {
      current: () => true,
      onPage: (value) => progress.push(value),
    },
  );
  const final = progress.at(-1);
  assert.equal(final.historicalReadError, true);
  assert.equal(final.historicalObservationErrorCode, "STORAGE_UNAVAILABLE");
  assert.deepEqual(
    final.historicalItems.map((item) => item.workId),
    ["103"],
  );
  assert.equal(final.recordsRead, 1);
  assert.equal(final.metadataOnly, true);
});

test("query results notify only after durable catalog persistence and preserve readable pages on save failure", async () => {
  const changes = [],
    stop = subscribeAuthorCatalogChanges((change) => changes.push(change));
  let fail = false;
  const adapter = createSourceAdapter({
    native: true,
    invoke: async () =>
      page(1, [work(201)], {
        total: 1,
        pages: 1,
        hasMore: false,
        discoveryRevision: fail ? null : 217,
        observationErrorCode: fail ? "STORAGE_UNAVAILABLE" : null,
      }),
  });
  const query = { kind: "author", query: "Writer", page: 1, folderId: null };
  try {
    const ok = await adapter.query(jm, query);
    assert.equal(ok.discoveryRevision, 217);
    assert.equal(changes.length, 1);
    fail = true;
    const failed = await adapter.query(jm, query);
    assert.equal(failed.items[0].workId, "201");
    assert.equal(failed.observationErrorCode, "STORAGE_UNAVAILABLE");
    assert.equal(
      changes.length,
      1,
      "failed persistence cannot provide membership proof",
    );
  } finally {
    stop();
  }
});

test("retained nonfollowed recent works are browseable without inflating live-page progress", async () => {
  const calls = [];
  const reader = new RecentUpdatesReader(
    {
      recentHistory: async () => ({
        ...jm,
        revision: 1,
        items: [work(303, ["Nonfollowed"]), work(304, [])],
        coverage: {
          headIds: ["303"],
          checkedAt: 5,
          pagesRead: 8,
          reachedEnd: false,
          joinedPrevious: false,
          initialWindow: true,
          errorCode: null,
        },
      }),
      query: async (_scope, query) => {
        calls.push(query.page);
        return page(query.page, [work(query.page === 1 ? 301 : 302)]);
      },
    },
    jm,
  );
  await reader.start();
  assert.equal(reader.state.snapshot.items.length, 1);
  assert.equal(reader.state.retainedItems.length, 2);
  assert.equal(reader.state.snapshot.rawRecords, 1);
  await reader.loadNext();
  assert.equal(reader.state.phase, "complete");
  assert.equal(reader.state.snapshot.rawRecords, 2);
  assert.equal(reader.state.retainedCoverage.initialWindow, true);
  assert.deepEqual(calls, [1, 2]);
  reader.dispose();
});

test("a failed observation remains visible and a later successful retry can clear the warning", async () => {
  let fail = true;
  const reader = new RecentUpdatesReader(
    {
      query: async () =>
        page(1, [work(401)], {
          total: 1,
          pages: 1,
          hasMore: false,
          discoveryRevision: fail ? null : 12,
          observationErrorCode: fail ? "STORAGE_UNAVAILABLE" : null,
        }),
    },
    jm,
  );
  await reader.start();
  assert.deepEqual(reader.state.uncommittedIds, ["JM:401"]);
  assert.equal(reader.state.snapshot.items.length, 1);
  fail = false;
  await reader.refresh();
  assert.deepEqual(reader.state.uncommittedIds, []);
  assert.equal(reader.state.observationErrorCode, null);
});

test("JM explicit English category evidence survives compaction without hiding Pica English or unknown metadata", () => {
  const english = work(501, ["Writer"], { categories: ["English Manga"] });
  assert.equal(
    isContentHidden(english),
    false,
    "library and recent browsing do not silently exclude English works",
  );
  assert.equal(isOutsideJmAuthorScope(english), true);
  assert.equal(workHasAuthor(english, "Writer"), false);
  assert.equal(isContentHidden({ ...english, source: "Pica" }), false);
  assert.equal(isOutsideJmAuthorScope({ ...english, source: "Pica" }), false);
  assert.equal(isContentHidden(work(502, ["English Manga"])), false);
  assert.deepEqual(retainedContentTags(["English Manga", "普通标签"]), [
    "English Manga",
  ]);
  assert.deepEqual(inheritContentTags([], ["English Manga"]), [
    "English Manga",
  ]);
});
