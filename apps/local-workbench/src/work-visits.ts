import type { SourceScope, SourceWork } from "./source-types.ts";
import type { WorkReference } from "./booklists.ts";

export interface SourceVisit {
  scope: SourceScope;
  reference: WorkReference;
  work?: SourceWork;
}
const listeners = new Set<(visit: SourceVisit) => void>();
/** Deliberate detail/reader navigation only; never called by covers or queries. */
export function recordSourceVisit(visit: SourceVisit): void {
  for (const listener of listeners) listener(visit);
}
export function subscribeSourceVisits(listener: (visit: SourceVisit) => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
