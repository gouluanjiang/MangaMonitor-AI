import { useEffect, useMemo, useState } from "react";
import { createAuthorCatalogIndex } from "./author-catalog-index.ts";
import { createCompletionAdapter } from "./completion-runtime.ts";
import type { DiscoverySnapshot } from "./completion-types.ts";
import { accountScope } from "./source-types.ts";
import type { AccountSummary } from "./source-types.ts";
import {
  subscribeAuthorCatalogChanges,
  notifyAuthorCatalogChanged,
} from "./author-catalog-events.ts";

const adapter = createCompletionAdapter();
const catalog = createAuthorCatalogIndex();
const pending = new Map<string, Promise<void>>();
const listeners = new Set<() => void>();

/** Only confirmed memberships count. Reading this local catalog never starts a scan. */
export function rememberAuthorCatalog(snapshot: DiscoverySnapshot) {
  if (catalog.remember(snapshot)) for (const listener of listeners) listener();
}

export function useAuthorCatalogMembership(
  accounts: AccountSummary[],
  active: boolean,
) {
  const [revision, redraw] = useState(0);
  const [refresh, requestRefresh] = useState(0);
  const scopes = accounts.flatMap((account) => {
    const scope = accountScope(account);
    return scope ? [scope] : [];
  });
  const key = JSON.stringify(scopes);
  const identityKey = JSON.stringify(
    accounts.map((account) => [
      account.source,
      account.accountId,
      account.sessionId,
    ]),
  );
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    catalog.retainScopes(scopes);
  }, [key]);
  useEffect(() => {
    const listener = () => redraw((value) => value + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
  useEffect(() => {
    return subscribeAuthorCatalogChanges((change) => {
      if (
        !scopes.some(
          (scope) =>
            scope.source === change.source &&
            scope.sessionId === change.sessionId,
        )
      )
        return;
      catalog.invalidate(change);
      redraw((value) => value + 1);
      requestRefresh((value) => value + 1);
    });
  }, [key, identityKey]);
  useEffect(() => {
    if (!active || scopes.length !== 2) return;
    let alive = true,
      watchingAuthor = false,
      watchingRecent = false;
    let authorFailures = 0,
      recentFailures = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      const [authorProgress, recentProgress] = await Promise.allSettled([
        adapter.progress(scopes),
        adapter.recentCheckProgress?.() ?? Promise.resolve(null),
      ]);
      if (!alive) return;
      let finished = false;
      if (authorProgress.status === "fulfilled") {
        authorFailures = 0;
        const checking = authorProgress.value.run?.phase === "checking";
        finished ||= watchingAuthor && !checking;
        watchingAuthor = checking;
      } else if (watchingAuthor && ++authorFailures > 3) {
        watchingAuthor = false;
        setFailed(true);
      }
      if (recentProgress.status === "fulfilled") {
        recentFailures = 0;
        const checking = recentProgress.value?.phase === "checking";
        finished ||= watchingRecent && !checking;
        watchingRecent = checking;
      } else if (watchingRecent && ++recentFailures > 3) {
        watchingRecent = false;
        setFailed(true);
      }
      // The updates panel stops polling when hidden. Observe only the small
      // progress DTOs here, and reload the durable catalog once per finished run.
      if (finished)
        for (const scope of scopes) notifyAuthorCatalogChanged(scope);
      if (watchingAuthor || watchingRecent)
        timer = setTimeout(() => void poll(), 5000);
    };
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [active, key, identityKey]);
  useEffect(() => {
    if (!active || scopes.length !== 2) return;
    let alive = true;
    setFailed(false);
    // Native + response notifications may arrive together. Coalesce them and
    // serialize local full-catalog reads instead of copying the catalog twice.
    const timer = setTimeout(
      () => {
        const predecessor = pending.get(identityKey);
        const read = (predecessor ?? Promise.resolve())
          .catch(() => {})
          .then(async () => {
            if (!alive) return;
            const next = await adapter.read(scopes, false);
            if (alive) rememberAuthorCatalog(next);
          });
        pending.set(identityKey, read);
        void read
          .catch(() => {
            if (alive) setFailed(true);
          })
          .finally(() => {
            if (pending.get(identityKey) === read) pending.delete(identityKey);
          });
      },
      refresh ? 150 : 0,
    );
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [active, key, identityKey, refresh]);
  const known = useMemo(() => catalog.read(scopes), [key, revision]);
  return { known, failed };
}
