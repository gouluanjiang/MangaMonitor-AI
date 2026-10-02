import type { SourceScope, SourceWork } from "./source-types.ts";

type Listener = (scope: SourceScope, work: SourceWork) => void;
const listeners = new Set<Listener>();

/** Successful, explicitly opened details only; this never fetches or retains data. */
export function notifySourceDetail(scope: SourceScope, work: SourceWork) {
  if (work.source !== scope.source) return;
  for (const listener of listeners) listener(scope, work);
}

export function subscribeSourceDetails(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
