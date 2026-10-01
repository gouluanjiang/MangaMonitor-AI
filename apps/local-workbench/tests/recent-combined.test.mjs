import test from "node:test";
import assert from "node:assert/strict";
import {
  RecentUpdatesView,
  mergeCombinedRecent,
} from "../src/combined-recent.ts";
import { RecentUpdatesReader } from "../src/recent-updates.ts";
import { sourceWorkKey } from "../src/source-types.ts";
import { SourceError } from "../src/source-runtime.ts";

const scope = (source) => ({ source, sessionId: `synthetic-${source}` });
const work = (source, id, date = "2026-10-01", extra = {}) => ({
  source,
  workId: String(id),
  title: `Synthetic ${source} ${id}`,
  authors: ["Synthetic"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
  sourceUpdatedAt: date,
  ...extra,
});
const page = (source, number, items, more = true) => ({
  ...scope(source),
  page: number,
  items,
  hasMore: more,
  total: 12,
  pages: more ? 3 : number,
  folders: [],
});
const keys = (items) => items.map(sourceWorkKey);
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};

test("combined identity uses source plus ID, dates sort only new windows and metadata updates stay in place", () => {
  const jm = work("JM", 1, "2026-09-30");
  const pica = work("Pica", 1, "2026-10-01");
  const unknown = work("JM", 2, null);
  const first = mergeCombinedRecent([], [[jm, unknown], [pica]], true);
  assert.deepEqual(keys(first), keys([pica, jm, unknown]));
  const enriched = {
    ...jm,
    title: "Updated title",
    tags: ["中文"],
    sourceUpdatedAt: "2026-10-03",
  };
  const fresh = work("Pica", 3, "2026-10-04");
  const next = mergeCombinedRecent(
    first,
    [
      [enriched, unknown],
      [pica, fresh],
    ],
    false,
  );
  assert.deepEqual(keys(next), keys([pica, jm, unknown, fresh]));
  assert.equal(next[1].title, "Updated title");
  assert.deepEqual(next[1].tags, ["中文"]);
  assert.deepEqual(
    keys(
      mergeCombinedRecent(
        next,
        [
          [enriched, unknown],
          [pica, fresh],
        ],
        true,
      ),
    ),
    keys([fresh, enriched, pica, unknown]),
  );
  assert.equal(first[1].title, jm.title);
});

test("opening combined and single views shares bounded per-account reads and preserves single-source site order", async () => {
  const releases = { JM: deferred(), Pica: deferred() };
  const calls = [];
  const adapter = {
    query: async (requested, request) => {
      calls.push([requested.source, request.page]);
      await releases[requested.source].promise;
      return page(requested.source, 1, [
        work(requested.source, 1, null),
        work(requested.source, 2, "2026-10-02"),
      ]);
    },
  };
  const readers = ["JM", "Pica"].map(
    (source) => new RecentUpdatesReader(adapter, scope(source)),
  );
  const single = new RecentUpdatesView("Pica", readers);
  const both = new RecentUpdatesView("both", readers);
  const initial = single.start();
  const combined = both.start();
  void both.start();
  assert.deepEqual(calls, [
    ["Pica", 1],
    ["JM", 1],
  ]);
  releases.Pica.resolve();
  await initial;
  assert.equal(both.state.reading, true);
  assert.equal(both.state.canLoadNext, true);
  assert.equal(both.state.displayItems.length, 2);
  assert.deepEqual(
    single.state.displayItems.map((item) => item.workId),
    ["1", "2"],
  );
  releases.JM.resolve();
  await combined;
  assert.equal(both.state.displayItems.length, 4);
  assert.deepEqual(
    both.state.displayItems.map((item) => item.sourceUpdatedAt),
    ["2026-10-02", "2026-10-02", null, null],
  );
  await both.start();
  assert.equal(calls.length, 2);
});

test("a slow source cannot block another source's next page or turn partial results into complete", async () => {
  const slow = deferred();
  const calls = [];
  const adapter = {
    query: async (requested, request) => {
      calls.push([requested.source, request.page]);
      if (requested.source === "JM") {
        await slow.promise;
        throw new SourceError("SOURCE_TIMEOUT");
      }
      return page(
        "Pica",
        request.page,
        [work("Pica", request.page)],
        request.page < 2,
      );
    },
  };
  const readers = ["JM", "Pica"].map(
    (source) => new RecentUpdatesReader(adapter, scope(source)),
  );
  const both = new RecentUpdatesView("both", readers);
  const started = both.start();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(both.state.phase, "ready");
  assert.equal(both.state.reading, true);
  const pendingNext = both.loadNext();
  void both.loadNext();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(
    both.state.sources.find((item) => item.source === "Pica").state.phase,
    "complete",
  );
  assert.notEqual(both.state.phase, "complete");
  assert.deepEqual(calls, [
    ["JM", 1],
    ["Pica", 1],
    ["Pica", 2],
  ]);
  slow.resolve();
  await Promise.all([started, pendingNext]);
  assert.equal(both.state.phase, "error");
  assert.equal(both.state.displayItems.length, 2);
});

test("a failed refresh keeps valid records and retries only the failed source's page", async () => {
  let fail = false;
  let revision = 0;
  const calls = [];
  const adapter = {
    query: async (requested, request) => {
      calls.push([requested.source, request.page]);
      if (fail && requested.source === "JM")
        throw new SourceError("SOURCE_TIMEOUT");
      return page(
        requested.source,
        request.page,
        [work(requested.source, revision + 1)],
        false,
      );
    },
  };
  const readers = ["JM", "Pica"].map(
    (source) => new RecentUpdatesReader(adapter, scope(source)),
  );
  const both = new RecentUpdatesView("both", readers);
  await both.start();
  fail = true;
  revision = 1;
  await both.refresh();
  assert.notEqual(both.state.phase, "complete");
  assert.deepEqual(
    new Set(keys(both.state.displayItems)),
    new Set(keys([work("JM", 1), work("Pica", 2)])),
  );
  fail = false;
  await both.retry("JM");
  assert.equal(both.state.phase, "complete");
  assert.deepEqual(calls, [
    ["JM", 1],
    ["Pica", 1],
    ["JM", 1],
    ["Pica", 1],
    ["JM", 1],
  ]);
  assert.deepEqual(
    new Set(keys(both.state.displayItems)),
    new Set(keys([work("JM", 2), work("Pica", 2)])),
  );
});

test("late saved history enriches in place and never promotes a later-page work", async () => {
  const late = deferred();
  const adapter = {
    query: async (requested, request) =>
      page(requested.source, request.page, [
        work(requested.source, request.page, "2026-09-30"),
      ]),
    recentHistory: async (requested) => {
      await late.promise;
      return {
        ...requested,
        items: [
          work(requested.source, 1, "2026-10-05", { tags: ["中文"] }),
          work(requested.source, 9, "2026-10-06"),
        ],
        coverage: {},
        revision: 1,
      };
    },
  };
  const readers = ["JM", "Pica"].map(
    (source) => new RecentUpdatesReader(adapter, scope(source)),
  );
  const both = new RecentUpdatesView("both", readers);
  const started = both.start();
  await Promise.resolve();
  await Promise.resolve();
  await both.loadNext();
  const original = keys(both.state.displayItems);
  late.resolve();
  await started;
  assert.deepEqual(keys(both.state.displayItems).slice(0, 4), original);
  assert.equal(both.state.displayItems.length, 6);
  assert.deepEqual(both.state.displayItems[0].tags, ["中文"]);
  assert.equal(both.state.snapshot.items.length, 4);
});

test("disconnected or stale sessions stay partial and disposed views ignore late callbacks", async () => {
  const reader = new RecentUpdatesReader(
    { query: async () => page("Pica", 1, [work("Pica", 1)], false) },
    scope("Pica"),
  );
  const both = new RecentUpdatesView("both", [reader]);
  await both.start();
  assert.notEqual(both.state.phase, "complete");
  assert.equal(both.state.sources[0].state, null);
  assert.equal(both.state.snapshot.total, null);

  const wait = deferred();
  const stale = new RecentUpdatesReader(
    {
      query: async () => {
        await wait.promise;
        return {
          ...page("JM", 1, [work("JM", 8)], false),
          sessionId: "other-account",
        };
      },
    },
    scope("JM"),
  );
  const view = new RecentUpdatesView("both", [stale, reader]);
  const started = view.start();
  const previous = view.state;
  view.dispose();
  wait.resolve();
  await started;
  assert.equal(view.state, previous);
  assert.equal(stale.state.error.code, "STALE_SESSION");
  assert.equal(stale.state.displayItems.length, 0);
  // Disposing one view must not dispose the shared source reader.
  await both.refresh();
  assert.equal(both.state.displayItems.length, 1);
});
