import type { DiscoverySnapshot } from "./completion-types.ts";
import type { InventoryMatch } from "./inventory-model.ts";
import type { SourceWork } from "./source-types.ts";
import { sourceWorkKey } from "./source-types.ts";

type DiscoveryRecord = DiscoverySnapshot["records"][number];

/** One work per source; conflicting provenance is never counted as new. */
export function uniqueDiscoveryRecords(records: DiscoveryRecord[]) {
  const unique = new Map<string, DiscoveryRecord>();
  for (const record of records) {
    const key = sourceWorkKey(record.work);
    const previous = unique.get(key);
    if (!previous) unique.set(key, record);
    else if (previous.firstDiscoveredRunId !== record.firstDiscoveredRunId) {
      const { firstDiscoveredRunId: _firstRun, ...historical } = previous;
      unique.set(key, historical);
    }
  }
  return [...unique.values()];
}

export function summarizeDiscoveryChanges(
  records: DiscoveryRecord[],
  runId: string,
  inventory: (work: SourceWork) => InventoryMatch,
) {
  const counts = {
    newTotal: 0,
    newOwned: 0,
    newMissing: 0,
    newUnknown: 0,
    historicalMissing: 0,
  };
  for (const record of uniqueDiscoveryRecords(records)) {
    const kind = inventory(record.work).kind;
    if (record.firstDiscoveredRunId === runId) {
      counts.newTotal++;
      if (kind === "owned") counts.newOwned++;
      else if (kind === "missing") counts.newMissing++;
      else counts.newUnknown++;
    } else if (kind === "missing") counts.historicalMissing++;
  }
  return counts;
}
