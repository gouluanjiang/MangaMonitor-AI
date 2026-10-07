import test from "node:test";
import assert from "node:assert/strict";
import {
  browseScope,
  readBrowsePosition,
  saveBrowsePosition,
  resolveBrowseAnchor,
  readBrowseState,
  saveBrowseState,
  forgetBrowsePositions,
} from "../src/browse-session.ts";

test("closing one workspace releases all of its position variants without clearing another workspace or runtime preferences", () => {
  const closed = [
    browseScope("search", "tab-close", "JM"),
    browseScope("search", "tab-close", "Pica"),
  ];
  const active = browseScope("search", "tab-active", "JM");
  const position = {
    anchor: { key: "JM:1", offset: -12 },
    keys: ["JM:1", "Pica:1"],
    scroll: 30,
  };
  for (const scope of [...closed, active]) saveBrowsePosition(scope, position);
  saveBrowseState(active, "missing");
  forgetBrowsePositions(closed);
  for (const scope of closed)
    assert.equal(readBrowsePosition(scope), undefined);
  assert.equal(readBrowsePosition(active), position);
  assert.equal(
    readBrowseState(active, () => "all"),
    "missing",
  );
  forgetBrowsePositions(closed);
  assert.equal(readBrowsePosition(active), position);
});

test("browsing snapshots isolate source/query scopes and retain anchored work after earlier results disappear", () => {
  const scope = browseScope("author", "JM", "synthetic-a");
  const saved = {
    anchor: { key: "c", offset: -12 },
    keys: ["a", "b", "c", "d"],
    scroll: 720,
  };
  assert.equal(readBrowsePosition(scope), undefined);
  saveBrowsePosition(scope, saved);
  assert.equal(
    readBrowsePosition(browseScope("author", "Pica", "synthetic-a")),
    undefined,
  );
  assert.equal(
    readBrowsePosition(browseScope("author", "JM", "synthetic-b")),
    undefined,
  );
  assert.deepEqual(
    resolveBrowseAnchor(readBrowsePosition(scope), ["b", "c", "d"]),
    { key: "c", offset: -12 },
  );
});

test("disappearing anchor chooses the nearest surviving work instead of restoring an obsolete pixel offset", () => {
  const saved = {
    anchor: { key: "c", offset: 25 },
    keys: ["a", "b", "c", "d", "e"],
    scroll: 800,
  };
  assert.deepEqual(resolveBrowseAnchor(saved, ["a", "b", "d", "e"]), {
    key: "d",
    offset: 25,
  });
  assert.deepEqual(resolveBrowseAnchor(saved, ["a", "b"]), {
    key: "b",
    offset: 25,
  });
  assert.equal(resolveBrowseAnchor(saved, ["other"]), null);
  assert.equal(resolveBrowseAnchor(saved, []), null);
});

test("scope components cannot collide and runtime filters remain distinct without persistent storage", async () => {
  assert.notEqual(browseScope("a:b", "c"), browseScope("a", "b:c"));
  const key = browseScope("filter", "synthetic");
  assert.equal(
    readBrowseState(key, () => "all"),
    "all",
  );
  saveBrowseState(key, "missing");
  assert.equal(
    readBrowseState(key, () => "all"),
    "missing",
  );
  const freshRuntime = await import("../src/browse-session.ts?fresh-runtime");
  assert.equal(
    freshRuntime.readBrowseState(key, () => "all"),
    "all",
  );
  assert.equal(
    freshRuntime.readBrowsePosition(browseScope("author", "JM", "synthetic-a")),
    undefined,
  );
});

let freshCacheId = 0;
const freshCache = () =>
  import(`../src/browse-session.ts?bounded-${++freshCacheId}`);
const positionFor = (keys, index = 0) => ({
  anchor: { key: keys[index], offset: -12 },
  keys,
  scroll: 720,
});

test("old query variants obey entry/key budgets and recently read positions survive LRU eviction", async () => {
  const cache = await freshCache();
  const small = positionFor(["JM:1"]);
  for (let i = 0; i < cache.browseCacheLimits.positions; i++)
    cache.saveBrowsePosition(`query-${i}`, small);
  assert.equal(cache.readBrowsePosition("query-0"), small);
  cache.saveBrowsePosition("new-query", small);
  assert.equal(cache.readBrowsePosition("query-1"), undefined);
  assert.equal(cache.readBrowsePosition("query-0"), small);
  assert.equal(
    cache.browseCacheUsage().positions.entries,
    cache.browseCacheLimits.positions,
  );

  const keys = Array.from(
    { length: cache.browseCacheLimits.keysPerPosition },
    (_, i) => `JM:${i}`,
  );
  const count =
    Math.ceil(cache.browseCacheLimits.positionKeys / keys.length) + 4;
  for (let i = 0; i < count; i++)
    cache.saveBrowsePosition(`large-${i}`, positionFor(keys));
  const usage = cache.browseCacheUsage().positions;
  assert.ok(usage.entries <= cache.browseCacheLimits.positions);
  assert.ok(usage.keys <= cache.browseCacheLimits.positionKeys);
  assert.ok(usage.bytes <= cache.browseCacheLimits.positionBytes);
  assert.equal(cache.readBrowsePosition("large-0"), undefined);
  assert.ok(cache.readBrowsePosition(`large-${count - 1}`));
});

test("retained positions cap bytes and keep the anchor with its nearest surviving neighbors", async () => {
  const cache = await freshCache();
  const keys = Array.from(
    { length: 12000 },
    (_, i) => `Pica:${String(i).padStart(24, "0")}`,
  );
  const original = positionFor(keys, 6000);
  original.anchor.atTop = true;
  const saved = cache.saveBrowsePosition("middle", original);
  assert.ok(saved.keys.length <= cache.browseCacheLimits.keysPerPosition);
  assert.equal(saved.anchor, original.anchor);
  assert.ok(saved.keys.includes(keys[5999]) && saved.keys.includes(keys[6001]));
  assert.deepEqual(cache.resolveBrowseAnchor(saved, [keys[5999], keys[6001]]), {
    ...original.anchor,
    key: keys[6001],
  });
  assert.equal(cache.resolveBrowseAnchor(saved, [keys[0]]), null);

  const wide = positionFor(
    Array.from({ length: 2000 }, (_, i) => `${i}:` + "x".repeat(1000)),
    1000,
  );
  for (let i = 0; i < 24; i++) cache.saveBrowsePosition(`wide-${i}`, wide);
  const usage = cache.browseCacheUsage().positions;
  assert.ok(usage.bytes <= cache.browseCacheLimits.positionBytes);
  assert.ok(
    usage.keys < cache.browseCacheLimits.positionKeys,
    "the byte budget, not only the key budget, must evict",
  );
  assert.equal(cache.readBrowsePosition("wide-0"), undefined);
  const last = cache.readBrowsePosition("wide-23");
  assert.equal(last.anchor.key, wide.anchor.key);
  assert.ok(last.keys.length < wide.keys.length);
});

test("a mounted list can restore only its current bounded position after LRU eviction", async () => {
  const cache = await freshCache();
  const owner = Symbol("open-tab");
  const scope = "open-tab-current";
  const position = cache.saveBrowsePosition(
    scope,
    positionFor(["JM:1", "JM:2"], 1),
    { owner },
  );
  cache.saveBrowsePosition("open-tab-other-source", position, { owner });
  for (let i = 0; i < cache.browseCacheLimits.positions; i++)
    cache.saveBrowsePosition(`other-${i}`, position);
  assert.equal(cache.readBrowsePosition(scope), undefined);
  const fallback = { scope, position };
  assert.equal(cache.readBrowsePosition(scope, fallback), position);
  assert.equal(
    cache.readBrowsePosition("different-query", fallback),
    undefined,
  );
  cache.saveBrowsePosition(scope, position, { owner });
  cache.saveBrowsePosition("open-tab-other-source", position, { owner });
  cache.forgetBrowseOwner(owner);
  assert.equal(cache.readBrowsePosition(scope), undefined);
  assert.equal(cache.readBrowsePosition("open-tab-other-source"), undefined);
  assert.ok(
    cache.readBrowsePosition(`other-${cache.browseCacheLimits.positions - 1}`),
  );
});

test("retired sessions release positions/states and cannot revive through a mounted fallback or late cleanup", async () => {
  const cache = await freshCache();
  const jm = { source: "JM", sessionId: "old-session" };
  const nextJm = { source: "JM", sessionId: "new-session" };
  const pica = { source: "Pica", sessionId: "unchanged-session" };
  const oldContext = { sessions: cache.browseSessionKeys([jm]) };
  const picaContext = { sessions: cache.browseSessionKeys([pica]) };
  const both = { sessions: cache.browseSessionKeys([jm, pica]) };
  const position = positionFor(["JM:1"]);
  cache.retainBrowseSessions([jm, pica]);
  cache.saveBrowsePosition("old", position, oldContext);
  cache.saveBrowsePosition("combined", position, both);
  cache.saveBrowsePosition("pica", position, picaContext);
  cache.saveBrowsePosition("local-library", position);
  cache.saveBrowseState("old-filter", "missing", oldContext);
  cache.saveBrowseState("pica-filter", "owned", picaContext);

  cache.retainBrowseSessions([nextJm, pica]);
  assert.equal(cache.readBrowsePosition("old"), undefined);
  assert.equal(cache.readBrowsePosition("combined"), undefined);
  assert.equal(cache.readBrowsePosition("pica"), position);
  assert.equal(cache.readBrowsePosition("local-library"), position);
  assert.equal(
    cache.readBrowsePosition(
      "old",
      { scope: "old", position },
      oldContext.sessions,
    ),
    undefined,
  );
  cache.saveBrowsePosition("old", position, oldContext);
  cache.saveBrowseState("old-filter", "missing", oldContext);
  assert.equal(cache.readBrowsePosition("old"), undefined);
  assert.equal(
    cache.readBrowseState("old-filter", () => "all", oldContext),
    "all",
  );
  assert.equal(
    cache.readBrowseState("pica-filter", () => "all", picaContext),
    "owned",
  );
  cache.saveBrowsePosition("new", position, {
    sessions: cache.browseSessionKeys([nextJm]),
  });
  assert.equal(cache.readBrowsePosition("new"), position);
});

test("runtime filter states have entry and byte limits and oversized values are left with their component", async () => {
  const cache = await freshCache();
  for (let i = 0; i < cache.browseCacheLimits.states; i++)
    cache.saveBrowseState(`filter-${i}`, "missing");
  assert.equal(
    cache.readBrowseState("filter-0", () => "all"),
    "missing",
  );
  cache.saveBrowseState("new-filter", "owned");
  assert.equal(
    cache.readBrowseState("filter-1", () => "all"),
    "all",
  );
  assert.equal(
    cache.readBrowseState("filter-0", () => "all"),
    "missing",
  );
  for (let i = 0; i < 40; i++)
    cache.saveBrowseState(`query-${i}`, "q".repeat(8000));
  assert.ok(
    cache.browseCacheUsage().states.bytes <= cache.browseCacheLimits.stateBytes,
  );
  assert.ok(
    cache.browseCacheUsage().states.entries <= cache.browseCacheLimits.states,
  );
  assert.equal(
    cache.readBrowseState("query-0", () => ""),
    "",
  );
  const huge = "q".repeat(cache.browseCacheLimits.stateBytes);
  assert.equal(
    cache.readBrowseState("huge", () => huge),
    huge,
  );
  assert.equal(
    cache.readBrowseState("huge", () => "not-cached"),
    "not-cached",
  );
  cache.saveBrowsePosition(
    "x".repeat(cache.browseCacheLimits.positionBytes),
    positionFor(["JM:1"]),
  );
  assert.ok(
    cache.browseCacheUsage().positions.bytes <=
      cache.browseCacheLimits.positionBytes,
  );
});
