import test from "node:test";
import assert from "node:assert/strict";
import {
  summarizeDiscoveryChanges,
  uniqueDiscoveryRecords,
} from "../src/discovery-summary.ts";

const runId = "a".repeat(64);
const priorRun = "b".repeat(64);
const record = (source, workId, firstDiscoveredRunId) => ({
  work: {
    source,
    workId,
    title: "Synthetic catalog work",
    authors: ["Synthetic author"],
    description: null,
    tags: [],
    favorite: null,
    chapterCount: null,
    pageCount: null,
    coverAvailable: false,
  },
  matchedAuthors: ["Synthetic author"],
  authorVerified: true,
  observedAt: 1800000000000,
  scanId: runId,
  ...(firstDiscoveredRunId ? { firstDiscoveredRunId } : {}),
});

test("discovery summaries use immutable first-discovery evidence and current inventory, never the last observation time", () => {
  const records = [
    record("JM", "1", runId),
    record("JM", "2", runId),
    record("Pica", "3", runId),
    record("Pica", "4", runId),
    record("JM", "5"),
    record("JM", "6", priorRun),
  ];
  const original = structuredClone(records);
  const kinds = new Map([
    ["1", "owned"],
    ["2", "missing"],
    ["3", "unknown"],
    ["4", "unconfigured"],
    ["5", "missing"],
    ["6", "missing"],
  ]);
  const inventory = (work) => ({ kind: kinds.get(work.workId), items: [] });
  assert.deepEqual(summarizeDiscoveryChanges(records, runId, inventory), {
    newTotal: 4,
    newOwned: 1,
    newMissing: 1,
    newUnknown: 2,
    historicalMissing: 2,
  });
  kinds.set("2", "owned");
  kinds.set("5", "owned");
  assert.deepEqual(summarizeDiscoveryChanges(records, runId, inventory), {
    newTotal: 4,
    newOwned: 2,
    newMissing: 0,
    newUnknown: 2,
    historicalMissing: 1,
  });
  assert.deepEqual(records, original);
});

test("overlapping author hits count once per source ID and conflicting or absent provenance remains historical in either order", () => {
  const current = record("JM", "1", runId);
  const duplicate = {
    ...structuredClone(current),
    matchedAuthors: ["Coauthor"],
  };
  const otherSource = record("Pica", "1", runId);
  assert.equal(
    uniqueDiscoveryRecords([current, duplicate, otherSource]).length,
    2,
  );
  const missing = () => ({ kind: "missing", items: [] });
  assert.equal(
    summarizeDiscoveryChanges([current, duplicate, otherSource], runId, missing)
      .newTotal,
    2,
  );
  for (const conflicting of [record("JM", "1", priorRun), record("JM", "1")]) {
    for (const records of [
      [current, conflicting, duplicate],
      [conflicting, duplicate, current],
    ]) {
      const original = structuredClone(records);
      assert.deepEqual(summarizeDiscoveryChanges(records, runId, missing), {
        newTotal: 0,
        newOwned: 0,
        newMissing: 0,
        newUnknown: 0,
        historicalMissing: 1,
      });
      assert.deepEqual(records, original);
    }
  }
});
