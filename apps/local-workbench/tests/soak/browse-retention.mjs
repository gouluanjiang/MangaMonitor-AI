// Standalone, synthetic retention diagnostic. No browser/build, native IPC,
// profile, credentials, source traffic, or library files are accessed.
// Run: node --expose-gc tests/soak/browse-retention.mjs
import {
  browseCacheLimits,
  browseCacheUsage,
  forgetBrowsePositions,
  readBrowsePosition,
  saveBrowsePosition,
} from "../../src/browse-session.ts";

if (!global.gc) throw new Error("Run this diagnostic with node --expose-gc");
const heap = async () => {
  await new Promise((resolve) => setImmediate(resolve));
  global.gc();
  return process.memoryUsage().heapUsed;
};
const scopes = [];
const before = await heap();
for (let n = 0; n < 160; n++) {
  const scope = JSON.stringify([
    "recent",
    "synthetic-session",
    `synthetic-query-${n}`,
    "all",
  ]);
  scopes.push(scope);
  const keys = Array.from(
    { length: 2000 },
    (_, i) => `Pica:${String(i + 1).padStart(24, "0")}`,
  );
  saveBrowsePosition(scope, {
    anchor: { key: keys[0], offset: 25 },
    keys,
    scroll: 0,
  });
}
const retained = await heap();
const stored = scopes.filter((scope) => readBrowsePosition(scope)).length;
const usage = browseCacheUsage();
forgetBrowsePositions(scopes);
const released = await heap();
console.log(
  JSON.stringify({
    diagnostic: "synthetic browse position retention",
    node: process.versions.node,
    scopes: scopes.length,
    itemsPerScope: 2000,
    storedAfterGc: stored,
    heapAddedMiB: +((retained - before) / 1048576).toFixed(2),
    heapAfterExplicitForgetDeltaMiB: +((released - before) / 1048576).toFixed(
      2,
    ),
    usage,
    limits: browseCacheLimits,
  }),
);
