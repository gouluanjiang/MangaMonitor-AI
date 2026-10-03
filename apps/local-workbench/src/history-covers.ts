import type { HistoryEntry, HistoryIdentity } from "./history-runtime.ts";
import type { LibraryItem, LibrarySnapshot } from "./library-types.ts";
import type {
  AccountSummary,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import { accountScope, sourceLabel } from "./source-types.ts";
import {
  isBlockedTagged,
  isContentHidden,
  isJmFemaleTag,
} from "./content-filter.ts";

export function historyIdentityKey(identity: HistoryIdentity): string {
  return identity.kind === "source"
    ? JSON.stringify([identity.kind, identity.source, identity.workId])
    : JSON.stringify([identity.kind, identity.rootId, identity.entryId]);
}

export function historyLibraryItem(
  identity: HistoryIdentity,
  library: LibrarySnapshot,
): LibraryItem | undefined {
  return identity.kind === "library" && identity.rootId === library.rootId
    ? library.items.find((item) => item.id === identity.entryId)
    : undefined;
}

/** Reuse explicit tag evidence only. Unknown history entries stay visible. */
export function historyEntryVisible(
  identity: HistoryIdentity,
  library: LibrarySnapshot,
): boolean {
  if (identity.kind === "source") return !isContentHidden(identity);
  const item = historyLibraryItem(identity, library);
  if (!item) return true;
  const references = [
    ...(item.sourceRef ? [item.sourceRef] : []),
    ...(item.links ?? []).map((link) => link.reference),
  ];
  return (
    !isBlockedTagged(item.tags) &&
    !references.some((reference) => isContentHidden(reference)) &&
    !(
      references.some((reference) => reference.source === "JM") &&
      item.tags.some(isJmFemaleTag)
    )
  );
}

export type HistoryCoverTarget =
  | { kind: "source"; scope: SourceScope; work: SourceWork }
  | { kind: "library"; item: LibraryItem }
  | { kind: "unavailable"; message: string };

/** Resolve identities against current scopes; never guess by title or persist URLs. */
export function historyCoverTarget(
  entry: HistoryEntry,
  accounts: readonly AccountSummary[],
  library: LibrarySnapshot,
): HistoryCoverTarget {
  const identity = entry.identity;
  if (identity.kind === "library") {
    const item = historyLibraryItem(identity, library);
    if (item?.state === "unreadable")
      return {
        kind: "unavailable",
        message:
          item.errorCode === "LIBRARY_RECYCLED"
            ? "文件已移到回收站"
            : "此文件暂时无法读取",
      };
    return item
      ? { kind: "library", item }
      : {
          kind: "unavailable",
          message:
            identity.rootId === library.rootId
              ? "当前漫画库中未找到此文件"
              : "此记录来自其他漫画库目录",
        };
  }
  const scope = accountScope(
    accounts.find((account) => account.source === identity.source),
  );
  if (!scope)
    return {
      kind: "unavailable",
      message: `连接 ${sourceLabel(identity.source)} 后读取封面`,
    };
  return {
    kind: "source",
    scope,
    // A render-only identity projection for the existing scoped cover loader.
    // The history does not claim fresh metadata or create query/action authority.
    work: {
      source: identity.source,
      workId: identity.workId,
      title: entry.title,
      authors: [],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: null,
      pageCount: null,
      coverAvailable: false,
    },
  };
}
