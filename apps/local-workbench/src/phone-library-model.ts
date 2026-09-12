import type { LibraryItem, LibrarySnapshot } from "./library-types.ts";
import type {
  PhoneLibraryEntry,
  PhoneLibrarySnapshot,
} from "./phone-library-types.ts";
import type { SourceWork } from "./source-types.ts";
import { createLibraryMatcher } from "./library-model.ts";
import type { SourceMatchPair } from "./source-matches-types.ts";
import { createSourceAliasResolver } from "./source-matches-model.ts";

/** Identity evidence preserves case, versions and every bracketed qualifier. */
export const phoneNameKey = (name: string) =>
  name
    .trim()
    .replace(/\.(zip|cbz|rar|7z)$/i, "")
    .trim()
    .normalize("NFC");
export interface PhoneLibraryRow {
  id: string;
  name: string;
  imported: boolean;
  manualEntries: PhoneLibraryEntry[];
}
export function phoneLibraryRows(
  snapshot: PhoneLibrarySnapshot,
): PhoneLibraryRow[] {
  const rows = new Map<string, PhoneLibraryRow>();
  snapshot.importedNames.forEach((name, index) => {
    const key = phoneNameKey(name);
    if (!rows.has(key))
      rows.set(key, {
        id: "imported-" + index,
        name,
        imported: true,
        manualEntries: [],
      });
  });
  for (const entry of snapshot.manualEntries) {
    const key = phoneNameKey(entry.name);
    const row = rows.get(key) ?? {
      id: entry.id,
      name: entry.name,
      imported: false,
      manualEntries: [],
    };
    row.manualEntries.push(entry);
    rows.set(key, row);
  }
  return [...rows.values()];
}
const referenceKey = (value: { source: string; workId: string }) =>
  value.source + ":" + value.workId;
function phoneIndex(
  phone: PhoneLibrarySnapshot,
  pairs: SourceMatchPair[] = [],
) {
  const aliases = createSourceAliasResolver(pairs);
  return {
    names: new Set([
      ...phone.importedNames.map(phoneNameKey),
      ...phone.manualEntries
        .filter((entry) => entry.reference === null)
        .map((entry) => phoneNameKey(entry.name)),
    ]),
    refs: new Set(
      phone.manualEntries.flatMap((entry) =>
        entry.reference ? aliases(entry.reference).map(referenceKey) : [],
      ),
    ),
  };
}
export function createPhoneItemMatcher(
  phone: PhoneLibrarySnapshot,
  pairs: SourceMatchPair[] = [],
) {
  const index = phoneIndex(phone, pairs);
  return (item: LibraryItem): "owned" | "downloaded" =>
    index.names.has(phoneNameKey(item.fileName)) ||
    (item.sourceRef !== null && index.refs.has(referenceKey(item.sourceRef)))
      ? "owned"
      : "downloaded";
}
export const phoneStatusForItem = (
  phone: PhoneLibrarySnapshot,
  item: LibraryItem,
  pairs: SourceMatchPair[] = [],
) => createPhoneItemMatcher(phone, pairs)(item);
export interface InventoryMatch {
  kind:
    | "owned"
    | "downloaded"
    | "candidate"
    | "missing"
    | "incomplete"
    | "unconfigured"
    | "unknown";
  items: LibraryItem[];
}
type WorkIdentity = Pick<SourceWork, "source" | "workId" | "title">;
export function createInventoryMatcher(
  library: LibrarySnapshot | undefined,
  phone: PhoneLibrarySnapshot,
  phoneReady = true,
  pairs: SourceMatchPair[] = [],
  matchesReady = true,
) {
  const currentPairs = matchesReady ? pairs : [];
  const local = createLibraryMatcher(library, currentPairs),
    directIndex = phoneIndex(phone),
    index = phoneIndex(phone, currentPairs),
    status = createPhoneItemMatcher(phone, currentPairs);
  return (work: WorkIdentity): InventoryMatch => {
    const match = local(work);
    if (!phoneReady) return { kind: "unknown", items: match.items };
    if (directIndex.refs.has(referenceKey(work)))
      return { kind: "owned", items: match.items };
    if (
      match.kind === "exact" &&
      match.items.some((item) => status(item) === "owned")
    )
      return { kind: "owned", items: match.items };
    if (!matchesReady) return { kind: "unknown", items: match.items };
    if (index.refs.has(referenceKey(work)))
      return { kind: "owned", items: match.items };
    if (match.kind === "exact")
      return {
        kind: "downloaded",
        items: match.items,
      };
    if (match.kind === "candidate" || index.names.has(phoneNameKey(work.title)))
      return { kind: "candidate", items: match.items };
    return {
      kind:
        match.kind === "unconfigured" &&
        (phone.importedAt !== null || phone.manualEntries.length > 0)
          ? "missing"
          : match.kind,
      items: match.items,
    };
  };
}
export const inventoryForWork = (
  library: LibrarySnapshot | undefined,
  phone: PhoneLibrarySnapshot,
  work: WorkIdentity,
) => createInventoryMatcher(library, phone)(work);
export const inventoryLabel = (match: InventoryMatch) =>
  ({
    owned: "已入库 · 手机名单",
    downloaded: "已下载 · 电脑文件",
    candidate: "同标题待确认",
    missing: "尚未匹配",
    incomplete: "电脑目录未读完",
    unconfigured: "尚未设置漫画库",
    unknown: "名单或关联未读取，状态待核对",
  })[match.kind];
