import type { LibraryReference } from "./library-types.ts";
import type { SourceMatchPair } from "./source-matches-types.ts";

const referenceKey = (reference: LibraryReference) =>
  reference.source + ":" + reference.workId;

/** Only explicit, one-to-one confirmations add aliases. Titles never enter this index. */
export function createSourceAliasResolver(pairs: SourceMatchPair[] = []) {
  const aliases = new Map<string, LibraryReference>();
  for (const pair of pairs) {
    aliases.set(referenceKey(pair.jm), pair.pica);
    aliases.set(referenceKey(pair.pica), pair.jm);
  }
  return (reference: LibraryReference): LibraryReference[] => {
    const other = aliases.get(referenceKey(reference));
    return other ? [reference, other] : [reference];
  };
}

export const findSourceMatch = (
  pairs: SourceMatchPair[],
  reference: LibraryReference,
) =>
  pairs.find((pair) =>
    reference.source === "JM"
      ? pair.jm.workId === reference.workId
      : pair.pica.workId === reference.workId,
  );
