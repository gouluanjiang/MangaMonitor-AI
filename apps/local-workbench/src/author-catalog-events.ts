import type { SourceScope } from "./source-types.ts";
import { isDesktopRuntime } from "./runtime.ts";

export interface AuthorCatalogChange extends SourceScope {
  revision?: number;
  followingRevision?: number;
  policyRevision?: number;
}
const listeners = new Set<(change: AuthorCatalogChange) => void>();
const seen = new Map<string, string>();
let nativeListening = false;

/** This announces a durable change; response items are never membership proof. */
export function notifyAuthorCatalogChanged(change: AuthorCatalogChange): void {
  const key = JSON.stringify([change.source, change.sessionId]);
  const version = JSON.stringify([
    change.revision,
    change.followingRevision,
    change.policyRevision,
  ]);
  if (change.revision !== undefined && seen.get(key) === version) return;
  seen.set(key, version);
  for (const listener of listeners) listener(change);
}

export function subscribeAuthorCatalogChanges(
  listener: (change: AuthorCatalogChange) => void,
): () => void {
  listeners.add(listener);
  if (!nativeListening && isDesktopRuntime()) {
    nativeListening = true;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<AuthorCatalogChange>("author-catalog-changed", ({ payload }) => {
          if (
            !payload ||
            !["JM", "Pica"].includes(payload.source) ||
            typeof payload.sessionId !== "string" ||
            !payload.sessionId ||
            [
              payload.revision,
              payload.followingRevision,
              payload.policyRevision,
            ].some(
              (value) =>
                value !== undefined &&
                (!Number.isSafeInteger(value) || value < 0),
            )
          )
            return;
          notifyAuthorCatalogChanged(payload);
        }),
      )
      .catch(() => {
        nativeListening = false;
      });
  }
  return () => {
    listeners.delete(listener);
  };
}
