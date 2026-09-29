import { partitionAuthorRecords } from "./author-evidence.ts";
import type { DiscoverySnapshot } from "./completion-types.ts";
import { sourceWorkKey } from "./source-types.ts";
import type { SourceScope } from "./source-types.ts";

/** A local read may finish after a newer scan snapshot has already arrived. */
export function createAuthorCatalogIndex() {
  const entries = new Map<
    string,
    {
      revision: number;
      keys: ReadonlySet<string>;
    }
  >();
  return {
    remember(snapshot: DiscoverySnapshot): boolean {
      const scopes = snapshot.scopes.filter((scope) => {
        const previous = entries.get(JSON.stringify(scope));
        return !previous || snapshot.revision >= previous.revision;
      });
      if (!scopes.length) return false;
      const confirmed = partitionAuthorRecords(
        snapshot.records,
        "",
        "all",
        snapshot.authorPolicies,
      ).confirmed;
      for (const scope of scopes) {
        entries.set(JSON.stringify(scope), {
          revision: snapshot.revision,
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
