import test from "node:test";
import assert from "node:assert/strict";
import { ContentCheckPool } from "../src/content-check.ts";

const DAY = 24 * 60 * 60 * 1000;
const scope = { source: "JM", sessionId: "synthetic-content-check" };
const work = (id, overrides = {}) => ({
  source: "JM",
  workId: String(id),
  title: `Synthetic ${id}`,
  authors: ["Writer"],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
  sourceUpdatedAt: "2026-09-29",
  ...overrides,
});
const result = (item, overrides = {}) => ({
  ...scope,
  items: [item],
  page: 1,
  total: 1,
  pages: 1,
  hasMore: false,
  folders: [],
  ...overrides,
});
const settle = async () => {
  // Drain only promise continuations; no wall-clock waits or network resources.
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
function harness() {
  const calls = [],
    resolved = [];
  let now = 1000;
  const pool = new ContentCheckPool(
    {
      query(requested, query) {
        let resolve, reject;
        const pending = new Promise((yes, no) => {
          resolve = yes;
          reject = no;
        });
        calls.push({ requested, query, resolve, reject });
        return pending;
      },
    },
    scope,
    (item) => resolved.push(item),
    () => now,
  );
  return {
    pool,
    calls,
    resolved,
    time(value) {
      now = value;
    },
  };
}

test("catalog inspection does not request details; only subscribed unknown works do", async () => {
  const h = harness();
  const items = Array.from({ length: 200 }, (_, i) => work(`demand-${i}`));
  items.forEach((item) => assert.equal(h.pool.state(item).phase, "waiting"));
  h.pool.seed(items, [items[0].workId]);
  assert.equal(h.calls.length, 0);
  assert.equal(h.pool.verified(items[0]), true);
  assert.equal(h.pool.verified(items[1]), false);
  const cached = h.pool.watch(items[0], () => {});
  const phases = [];
  const stop = h.pool.watch(items[1], (state) => phases.push(state.phase));
  assert.equal(h.calls.length, 1);
  assert.deepEqual(h.calls[0].requested, scope);
  assert.deepEqual(h.calls[0].query, {
    kind: "detail",
    query: items[1].workId,
    folderId: null,
    page: 1,
  });
  h.calls[0].resolve(result(items[1]));
  await settle();
  assert.deepEqual(phases, ["waiting", "checking", "ready"]);
  assert.equal(h.pool.verified(items[1]), true);
  assert.equal(h.calls.length, 1);
  cached();
  stop();
  h.pool.dispose();
});

test("two requests run at once, duplicate subscribers share work, and offscreen queued work is cancelled", async () => {
  const h = harness();
  const items = [1, 2, 3, 4].map((id) => work(`queue-${id}`));
  const stops = items.map((item) => h.pool.watch(item, () => {}));
  const duplicate = h.pool.watch(items[0], () => {});
  assert.deepEqual(
    h.calls.map((call) => call.query.query),
    [items[0].workId, items[1].workId],
  );
  stops[2]();
  h.calls[0].resolve(result(items[0]));
  await settle();
  assert.deepEqual(
    h.calls.map((call) => call.query.query),
    [items[0].workId, items[1].workId, items[3].workId],
  );
  h.calls[1].resolve(result(items[1]));
  h.calls[2].resolve(result(items[3]));
  await settle();
  assert.equal(h.calls.length, 3);
  const back = h.pool.watch(items[2], () => {});
  assert.equal(h.calls.length, 4);
  h.calls[3].resolve(result(items[2]));
  await settle();
  stops.forEach((stop) => stop());
  duplicate();
  back();
  h.pool.dispose();
});

test("failure stays retryable without automatic requests, including leaving and returning to the viewport", async () => {
  const h = harness(),
    item = work("manual-retry");
  const stop = h.pool.watch(item, () => {});
  const error = new Error("SYNTHETIC_OFFLINE");
  h.calls[0].reject(error);
  await settle();
  assert.equal(h.pool.state(item).phase, "error");
  assert.equal(h.pool.state(item).error, error);
  assert.equal(h.pool.verified(item), false);
  stop();
  const back = h.pool.watch(item, () => {});
  await settle();
  assert.equal(h.calls.length, 1);
  h.pool.retry(item);
  assert.equal(h.calls.length, 2);
  h.pool.retry(item);
  assert.equal(h.calls.length, 2);
  h.calls[1].resolve(result(item));
  await settle();
  assert.equal(h.pool.verified(item), true);
  back();
  h.pool.dispose();
});

test("wrong source, session, identity, item count or partial detail cannot verify a work", async () => {
  const invalid = [
    (item) => result(item, { source: "Pica" }),
    (item) => result(item, { sessionId: "another-session" }),
    (item) => result({ ...item, source: "Pica" }),
    (item) => result({ ...item, workId: "another-work" }),
    () => result(null, { items: [] }),
    (item) => result(item, { items: [item, item] }),
    (item) => result(item, { issues: [{ code: "SOURCE_ITEM_INVALID" }] }),
  ];
  for (const [index, reply] of invalid.entries()) {
    const h = harness(),
      item = work(`invalid-${index}`);
    h.pool.watch(item, () => {});
    h.calls[0].resolve(reply(item));
    await settle();
    assert.equal(h.pool.state(item).phase, "error");
    assert.equal(h.pool.state(item).error.code, "INVALID_RESPONSE");
    assert.equal(h.pool.verified(item), false);
    assert.deepEqual(h.resolved, []);
    h.pool.dispose();
  }
});

test("dispose ignores late responses and does not start queued work or notify stale subscribers", async () => {
  const h = harness();
  const items = [1, 2, 3].map((id) => work(`disposed-${id}`));
  const phases = [];
  items.forEach((item) =>
    h.pool.watch(item, (state) => phases.push(state.phase)),
  );
  const before = [...phases];
  h.pool.dispose();
  h.calls[0].resolve(result(items[0]));
  h.calls[1].reject(new Error("late failure"));
  await settle();
  assert.equal(h.calls.length, 2);
  assert.deepEqual(phases, before);
  assert.deepEqual(h.resolved, []);
  assert.equal(h.pool.verified(items[0]), false);
});

test("a newer list version in flight cannot be released by the older detail response", async () => {
  const h = harness();
  const first = work("version-race");
  const second = { ...first, sourceUpdatedAt: "2026-09-30" };
  const phases = [];
  const stop = h.pool.watch(first, (state) => phases.push(state.phase));
  const next = h.pool.watch(second, (state) => phases.push(state.phase));
  assert.equal(h.calls.length, 2);
  assert.equal(h.pool.verified(second), false);
  h.calls[0].resolve(result(first));
  await settle();
  assert.equal(h.pool.verified(second), false);
  assert.equal(h.pool.verified(first), false);
  assert.equal(h.pool.state(second).phase, "checking");
  assert.equal(phases.includes("ready"), false);
  assert.deepEqual(h.resolved, []);
  h.calls[1].resolve(result(second));
  await settle();
  assert.equal(h.pool.verified(second), true);
  assert.equal(h.pool.verified(first), false);
  assert.deepEqual(
    h.resolved.map((item) => item.sourceUpdatedAt),
    [second.sourceUpdatedAt],
  );
  stop();
  next();
  h.pool.dispose();
});

test("late older details cannot overwrite an already verified newer generation", async () => {
  const h = harness();
  const first = work("reverse-race");
  const second = { ...first, sourceUpdatedAt: "2026-09-30" };
  h.pool.watch(first, () => {});
  h.pool.watch(second, () => {});
  h.calls[1].resolve(result(second));
  await settle();
  h.calls[0].resolve(result({ ...first, tags: ["AI"] }));
  await settle();
  assert.equal(h.pool.verified(second), true);
  assert.deepEqual(h.pool.state(second).work.tags, []);
  assert.equal(h.resolved.length, 1);
  h.pool.dispose();
});

test("saved proof keeps its absolute expiry, expired proof is ignored and future proof is capped at one day", () => {
  const h = harness(),
    saved = work("saved-expiry"),
    capped = work("capped-expiry");
  h.pool.seed([saved], [saved.workId], 1100);
  assert.equal(h.pool.verified(saved), true);
  h.time(1099);
  h.pool.seed([saved], [saved.workId], 1100);
  h.time(1100);
  assert.equal(h.pool.verified(saved), false);
  h.pool.seed([saved], [saved.workId], 1100);
  assert.equal(h.pool.state(saved).phase, "waiting");
  h.pool.seed([capped], [capped.workId], 1100 + DAY * 10);
  h.time(1100 + DAY - 1);
  assert.equal(h.pool.verified(capped), true);
  h.time(1100 + DAY);
  assert.equal(h.pool.verified(capped), false);
  assert.equal(h.calls.length, 0);
  h.pool.dispose();
});

test("live proof expires without fetching offscreen, then requests a fresh check when watched", async () => {
  const h = harness(),
    item = work("live-expiry");
  const stop = h.pool.watch(item, () => {});
  h.calls[0].resolve(result(item));
  await settle();
  stop();
  h.time(1000 + DAY);
  assert.equal(h.pool.verified(item), false);
  assert.equal(h.pool.state(item).phase, "waiting");
  assert.equal(h.calls.length, 1);
  h.pool.watch(item, () => {});
  assert.equal(h.calls.length, 2);
  h.calls[1].resolve(result(item));
  await settle();
  assert.equal(h.pool.verified(item), true);
  h.pool.dispose();
});

test("explicit blocked metadata never qualifies for selection, whether listed, seeded or learned from detail", async () => {
  const h = harness();
  const listed = work("blocked-list", { tags: ["BL"] });
  const seeded = work("blocked-seed", { categories: ["女性向"] });
  const unknown = work("blocked-detail");
  h.pool.watch(listed, () => {});
  h.pool.seed([seeded], [seeded.workId]);
  h.pool.watch(seeded, () => {});
  assert.equal(h.calls.length, 0);
  h.pool.watch(unknown, () => {});
  h.calls[0].resolve(result({ ...unknown, categories: ["AI"] }));
  await settle();
  assert.equal(h.pool.state(unknown).phase, "ready");
  assert.deepEqual(
    [listed, seeded, unknown].filter((item) => h.pool.verified(item)),
    [],
  );
  h.pool.dispose();
});
