import test from "node:test";
import assert from "node:assert/strict";
import {
  BrowsingMarkerTracker,
  MAX_BROWSING_IDS,
  validateBrowsingBaseline,
  validateBrowsingDocument,
} from "../src/browsing-markers.ts";

const baseline = (knownIds, headIds = knownIds, reachedEnd = false) => ({
  knownIds,
  headIds,
  reachedEnd,
});
const recent = (liveIds, extra = {}) => ({
  liveIds,
  savedIds: [],
  historyReady: true,
  hasIssues: false,
  reachedEnd: false,
  failed: false,
  ...extra,
});

test("first activation waits for saved recent data and seeds old catalog without any badges", () => {
  const tracker = new BrowsingMarkerTracker(null);
  tracker.observeRecent(recent(["new-head"], { historyReady: false }));
  assert.equal(tracker.next, null);
  tracker.observeRecent(
    recent(["new-head"], { savedIds: ["old-1", "old-2", "old-3"] }),
  );
  assert.deepEqual(tracker.next.knownIds, [
    "old-1",
    "old-2",
    "old-3",
    "new-head",
  ]);
  tracker.observeRecent(
    recent(["new-head", "older-tail"], {
      savedIds: ["old-1", "old-2", "old-3"],
    }),
  );
  assert.equal(tracker.newIds.size, 0);
  assert.equal(tracker.status, "initial");
});

test("unseen historical tail pages never become new, while a joined continuous prefix does", () => {
  const tracker = new BrowsingMarkerTracker(
    baseline(["head", "known-old"], ["head"]),
  );
  tracker.observeRecent(
    recent(["fresh", "known-old", "head", "never-paged-tail"], {
      savedIds: ["historic-only"],
    }),
  );
  assert.deepEqual([...tracker.newIds], ["fresh"]);
  assert.ok(tracker.next.knownIds.includes("never-paged-tail"));
  assert.ok(tracker.next.knownIds.includes("historic-only"));
  tracker.observeRecent(recent(["head", "tail-2"]));
  assert.deepEqual([...tracker.newIds], ["fresh"]);
});

test("a delayed join stays uncertain and preserves baseline until reached by ordinary paging", () => {
  const before = baseline(["head"], ["head"]);
  const tracker = new BrowsingMarkerTracker(before);
  tracker.observeRecent(recent(["fresh-1", "fresh-2"]));
  assert.equal(tracker.status, "partial");
  assert.equal(tracker.newIds.size, 0);
  assert.deepEqual(tracker.next, before);
  tracker.observeRecent(recent(["fresh-1", "fresh-2", "head", "old-tail"]));
  assert.deepEqual([...tracker.newIds], ["fresh-1", "fresh-2"]);
  assert.equal(tracker.status, "ready");
});

test("partial source failures and isolated invalid rows do not claim continuity", () => {
  const tracker = new BrowsingMarkerTracker(baseline(["head"]));
  for (const extra of [
    { failed: true },
    { hasIssues: true },
    { historyReady: false },
  ]) {
    tracker.observeRecent(recent(["fresh", "head"], extra));
    assert.equal(tracker.newIds.size, 0);
    assert.deepEqual(tracker.next.knownIds, ["head"]);
  }
  tracker.observeRecent(recent(["fresh", "head"]));
  tracker.observeRecent(recent(["fresh", "head"], { failed: true }));
  assert.deepEqual([...tracker.newIds], ["fresh"]);
  assert.equal(tracker.status, "partial");
});

test("missing old head and a genuinely empty ended feed are different baselines", () => {
  const unknown = new BrowsingMarkerTracker(baseline(["old"], []));
  unknown.observeRecent(recent(["head"]));
  assert.equal(unknown.status, "partial");
  assert.equal(unknown.newIds.size, 0);
  assert.deepEqual(unknown.next.headIds, ["head"]);
  const reopened = new BrowsingMarkerTracker(unknown.next);
  reopened.observeRecent(recent(["head", "old-tail"]));
  assert.equal(reopened.newIds.size, 0);
  reopened.observeRecent(recent(["fresh", "head", "old-tail"]));
  assert.deepEqual([...reopened.newIds], ["fresh"]);
  const empty = new BrowsingMarkerTracker(baseline([], [], true));
  empty.observeRecent(recent(["first-work"]));
  assert.deepEqual([...empty.newIds], ["first-work"]);
});

test("saving future baseline does not clear launch badges; next launch adopts it", () => {
  const tracker = new BrowsingMarkerTracker(baseline(["head"]));
  tracker.observeRecent(recent(["fresh", "head"]));
  const saved = structuredClone(tracker.next);
  tracker.observeRecent(recent(["head"]));
  assert.deepEqual([...tracker.newIds], ["fresh"]);
  const restarted = new BrowsingMarkerTracker(saved);
  restarted.observeRecent(recent(["fresh", "head"]));
  assert.equal(restarted.newIds.size, 0);
});

test("author baselines compare all catalog identities and do not depend on manual run IDs", () => {
  const first = new BrowsingMarkerTracker(null);
  first.observeAuthors(["old-1", "old-2"], false);
  assert.equal(first.newIds.size, 0);
  const next = new BrowsingMarkerTracker(first.next);
  next.observeAuthors(["old-2", "old-1", "added"], true);
  assert.deepEqual([...next.newIds], ["added"]);
  assert.equal(next.status, "partial");
  next.observeAuthors(["old-1"], false);
  assert.deepEqual([...next.newIds], ["added"]);
  assert.ok(next.next.knownIds.includes("old-2"));
});

test("source-account-surface trackers cannot cross-contaminate identical work IDs", () => {
  const jm = new BrowsingMarkerTracker(baseline(["1"]));
  const pica = new BrowsingMarkerTracker(baseline(["2"]));
  const otherAccount = new BrowsingMarkerTracker(null);
  jm.observeAuthors(["1", "2"], false);
  pica.observeAuthors(["1", "2"], false);
  otherAccount.observeAuthors(["1", "2"], false);
  assert.deepEqual([...jm.newIds], ["2"]);
  assert.deepEqual([...pica.newIds], ["1"]);
  assert.equal(otherAccount.newIds.size, 0);
});

test("repeat observations are stable and do not queue duplicate saves or render notifications", () => {
  const tracker = new BrowsingMarkerTracker(baseline(["head"]));
  const observation = recent(["fresh", "head"]);
  tracker.observeRecent(observation);
  const next = tracker.next,
    revision = tracker.revision;
  tracker.observeRecent(observation);
  assert.equal(tracker.next, next);
  assert.equal(tracker.revision, revision);
});

test("over-limit catalog does not evict old identities or manufacture new badges", () => {
  const tracker = new BrowsingMarkerTracker(baseline(["head"]));
  tracker.observeAuthors(
    Array.from({ length: MAX_BROWSING_IDS + 1 }, (_, i) => String(i)),
    false,
  );
  assert.equal(tracker.status, "partial");
  assert.equal(tracker.limited, true);
  assert.equal(tracker.newIds.size, 0);
  assert.deepEqual(tracker.next, baseline(["head"]));
});

test("accumulated identities also respect the bound without pruning old baseline evidence", () => {
  const ids = Array.from({ length: MAX_BROWSING_IDS }, (_, i) => String(i));
  const tracker = new BrowsingMarkerTracker(baseline(ids, ["0"]));
  tracker.observeAuthors(["brand-new"], false);
  assert.equal(tracker.limited, true);
  assert.equal(tracker.newIds.size, 0);
  assert.deepEqual(tracker.next.knownIds, ids);
});

test("unknown versions, malformed IDs, duplicate IDs and heads outside baseline are rejected", () => {
  assert.throws(() =>
    validateBrowsingDocument({
      revision: 1,
      value: { version: 2, baseline: null },
    }),
  );
  assert.throws(() => validateBrowsingBaseline(baseline(["../unsafe"])));
  assert.throws(() => validateBrowsingBaseline(baseline(["1", "1"])));
  assert.throws(() => validateBrowsingBaseline(baseline(["1"], ["2"])));
  assert.deepEqual(
    validateBrowsingDocument({
      revision: 0,
      value: { version: 1, baseline: null },
    }),
    null,
  );
});
