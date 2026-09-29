import { useEffect, useMemo, useState } from "react";
import { createAuthorCatalogIndex } from "./author-catalog-index.ts";
import { createCompletionAdapter } from "./completion-runtime.ts";
import type { DiscoverySnapshot } from "./completion-types.ts";
import { accountScope } from "./source-types.ts";
import type { AccountSummary } from "./source-types.ts";

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
  const scopes = accounts.flatMap((account) => {
    const scope = accountScope(account);
    return scope ? [scope] : [];
  });
  const key = JSON.stringify(scopes);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const listener = () => redraw((value) => value + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
  useEffect(() => {
    if (!active || !scopes.length) return;
    let alive = true;
    setFailed(false);
    let read = pending.get(key);
    if (!read) {
      read = adapter.read(scopes, false).then(rememberAuthorCatalog);
      pending.set(key, read);
      void read
        .finally(() => {
          pending.delete(key);
        })
        .catch(() => {});
    }
    void read.catch(() => {
      if (alive) setFailed(true);
    });
    return () => {
      alive = false;
    };
  }, [active, key]);
  const known = useMemo(() => catalog.read(scopes), [key, revision]);
  return { known, failed };
}
