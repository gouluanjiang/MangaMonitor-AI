import test from "node:test";
import assert from "node:assert/strict";
import {
  browseScope,
  readBrowsePosition,
  saveBrowsePosition,
  resolveBrowseAnchor,
  readBrowseState,
  saveBrowseState,
} from "../src/browse-session.ts";

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
