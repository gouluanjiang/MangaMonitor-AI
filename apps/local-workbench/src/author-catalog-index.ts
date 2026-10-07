import { partitionAuthorRecords } from "./author-evidence.ts";
import type { DiscoverySnapshot } from "./completion-types.ts";
import { sourceWorkKey } from "./source-types.ts";
import type { SourceScope } from "./source-types.ts";
import type { AuthorCatalogChange } from "./author-catalog-events.ts";

/** A local read may finish after a newer scan snapshot has already arrived. */
export function createAuthorCatalogIndex() {
  const entries = new Map<
    string,
    {
      revision: number;
      followingRevision: number;
      policyRevision: number;
      keys: ReadonlySet<string>;
    }
  >();
  const minimum = new Map<string, AuthorCatalogChange>();
  let current: Set<string> | null = null;
  return {
    retainScopes(scopes: SourceScope[]) {
      current = new Set(scopes.map((scope) => JSON.stringify(scope)));
      for (const key of entries.keys())
        if (!current.has(key)) entries.delete(key);
      for (const key of minimum.keys())
        if (!current.has(key)) minimum.delete(key);
    },
    invalidate(change: AuthorCatalogChange) {
      const key = JSON.stringify({
        source: change.source,
        sessionId: change.sessionId,
      });
      if (current && !current.has(key)) return;
      const previous = minimum.get(key);
      minimum.set(key, {
        ...change,
        revision: Math.max(previous?.revision ?? 0, change.revision ?? 0),
        followingRevision: Math.max(
          previous?.followingRevision ?? 0,
          change.followingRevision ?? 0,
        ),
        policyRevision: Math.max(
          previous?.policyRevision ?? 0,
          change.policyRevision ?? 0,
        ),
      });
      entries.delete(key);
    },
    remember(snapshot: DiscoverySnapshot): boolean {
      const scopes = snapshot.scopes.filter((scope) => {
        const key = JSON.stringify(scope),
          previous = entries.get(key),
          floor = minimum.get(key);
        return (
          (!current || current.has(key)) &&
          snapshot.revision >=
            Math.max(previous?.revision ?? 0, floor?.revision ?? 0) &&
          (snapshot.followingRevision ?? 0) >=
            Math.max(
              previous?.followingRevision ?? 0,
              floor?.followingRevision ?? 0,
            ) &&
          (snapshot.policyRevision ?? 0) >=
            Math.max(previous?.policyRevision ?? 0, floor?.policyRevision ?? 0)
        );
      });
      if (!scopes.length) return false;
      const confirmed = partitionAuthorRecords(
        snapshot.records,
        "",
        "all",
        snapshot.authorPolicies,
        snapshot.followedAuthors ??
          (snapshot.authors.length
            ? [...new Set(snapshot.authors.map((range) => range.author))]
            : undefined),
      ).confirmed;
      for (const scope of scopes) {
        entries.set(JSON.stringify(scope), {
          revision: snapshot.revision,
          followingRevision: snapshot.followingRevision ?? 0,
          policyRevision: snapshot.policyRevision ?? 0,
          keys: new Set(
            confirmed
              .filter((record) => record.work.source === scope.source)
              .map((record) => sourceWorkKey(record.work)),
          ),
        });
      }
      return true;
    },
    read(scopes: SourceScope[]): ReadonlySet<string> {
      return new Set(
        scopes.flatMap((scope) => [
          ...(entries.get(JSON.stringify(scope))?.keys ?? []),
        ]),
      );
    },
  };
}
