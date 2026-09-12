import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { parseLibraryReference } from "./library-model.ts";
import { LibraryError } from "./library-runtime.ts";
import { SourceError } from "./source-runtime.ts";
import { accountScope } from "./source-types.ts";
import type { LibraryReference } from "./library-types.ts";
import type { AccountSummary, Source, SourceAdapter } from "./source-types.ts";
import type {
  SourceMatchWork,
  SourceMatchesAdapter,
  SourceMatchesSnapshot,
} from "./source-matches-types.ts";

const invalid = (): never => {
  throw new LibraryError("SOURCE_MATCH_INVALID");
};
const object = (value: unknown): Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : invalid();
const integer = (value: unknown): number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : invalid();
function matchWork(value: unknown, source: Source): SourceMatchWork {
  const raw = object(value);
  if (
    raw.source !== source ||
    typeof raw.workId !== "string" ||
    typeof raw.title !== "string" ||
    !raw.title.trim() ||
    [...raw.title].length > 1024 ||
    /[\x00-\x1f\x7f-\x9f]/.test(raw.title)
  )
    return invalid();
  const reference = parseLibraryReference(source, raw.workId);
  if (reference?.workId !== raw.workId) return invalid();
  return { ...reference, title: raw.title };
}
export function validateSourceMatchesSnapshot(
  value: unknown,
): SourceMatchesSnapshot {
  const raw = object(value);
  if (!Array.isArray(raw.pairs) || raw.pairs.length > 10000) return invalid();
  const ids = new Set<string>(),
    jmIds = new Set<string>(),
    picaIds = new Set<string>();
  const pairs = raw.pairs.map((value) => {
    const pair = object(value);
    if (
      typeof pair.id !== "string" ||
      !/^[a-f0-9]{64}$/.test(pair.id) ||
      pair.evidence !== "manual"
    )
      return invalid();
    const jm = matchWork(pair.jm, "JM"),
      pica = matchWork(pair.pica, "Pica");
    if (ids.has(pair.id) || jmIds.has(jm.workId) || picaIds.has(pica.workId))
      return invalid();
    ids.add(pair.id);
    jmIds.add(jm.workId);
    picaIds.add(pica.workId);
    return {
      id: pair.id,
      jm,
      pica,
      confirmedAt: integer(pair.confirmedAt),
      evidence: "manual" as const,
    };
  });
  return { revision: integer(raw.revision), pairs };
}

type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createSourceMatchesAdapter(
  options: { invoke?: Invoke; native?: boolean } = {},
): SourceMatchesAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  const call = async (command: string, args: Record<string, unknown> = {}) => {
    if (!(options.native ?? isDesktopRuntime()))
      throw new LibraryError("DESKTOP_REQUIRED");
    let value: unknown;
    try {
      value = await invoke(command, args);
    } catch (cause) {
      const code = (cause as { code?: unknown })?.code;
      throw new LibraryError(
        typeof code === "string" && /^[A-Z_]{1,80}$/.test(code)
          ? code
          : "SOURCE_MATCH_UNAVAILABLE",
      );
    }
    return validateSourceMatchesSnapshot(value);
  };
  return {
    read: () => call("source_matches_read"),
    confirm: (revision, jm, pica) =>
      call("source_matches_confirm", {
        revision: integer(revision),
        jm: matchWork(jm, "JM"),
        pica: matchWork(pica, "Pica"),
      }),
    unlink: (revision, pairId) => {
      if (!/^[a-f0-9]{64}$/.test(pairId)) return invalid();
      return call("source_matches_unlink", {
        revision: integer(revision),
        pairId,
      });
    },
  };
}

export function sourceMatchesErrorMessage(cause: unknown) {
  const code = (cause as { code?: unknown })?.code;
  if (code === "SOURCE_MATCH_CONFLICT")
    return "其中一本已关联另一部作品。请先核对并解除原关联。";
  if (code === "REVISION_CONFLICT" || code === "SOURCE_MATCH_NOT_FOUND")
    return "关联记录已有变化，请重新读取后再次确认。";
  if (code === "SOURCE_MATCH_INVALID")
    return "作品编号或关联内容无法确认，请重新读取作品。";
  if (code === "SOURCE_MATCH_LIMIT")
    return "关联记录已达到 10,000 对，请先整理已有记录。";
  return "跨来源关联暂时无法读取或保存，请重新读取后重试。";
}

/** A user-requested detail lookup previews the exact opposite ID before confirmation. */
export async function lookupSourceMatchWork(
  adapter: SourceAdapter,
  accounts: AccountSummary[],
  reference: LibraryReference,
): Promise<SourceMatchWork> {
  const parsed = parseLibraryReference(reference.source, reference.workId);
  if (!parsed || parsed.workId !== reference.workId) return invalid();
  const scope = accountScope(
    accounts.find((account) => account.source === reference.source),
  );
  if (!scope) throw new SourceError("LOGIN_REQUIRED");
  const result = await adapter.query(scope, {
    kind: "detail",
    query: reference.workId,
    folderId: null,
    page: 1,
  });
  if (result.source !== scope.source || result.sessionId !== scope.sessionId)
    throw new SourceError("STALE_SESSION");
  if (
    result.items.length !== 1 ||
    result.items[0].source !== reference.source ||
    result.items[0].workId !== reference.workId
  )
    throw new SourceError("INVALID_RESPONSE");
  return matchWork(result.items[0], reference.source);
}
