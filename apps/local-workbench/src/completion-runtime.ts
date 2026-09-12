import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { parseLibraryReference } from "./library-model.ts";
import { validateSourceWork, SourceError } from "./source-runtime.ts";
import type { Source, SourceScope } from "./source-types.ts";
import type {
  CompletionAdapter,
  CompletionLanguage,
  CompletionMember,
  CompletionSettings,
  CompletionView,
  DiscoveryRun,
} from "./completion-types.ts";

const invalid = (): never => {
  throw new SourceError("COMPLETENESS_INVALID");
};
const object = (v: unknown): Record<string, unknown> =>
  v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : invalid();
const integer = (v: unknown): number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 0 ? v : invalid();
const str = (v: unknown, max = 1024): string =>
  typeof v === "string" && v.length <= max && !/[\x00-\x1f\x7f-\x9f]/.test(v)
    ? v
    : invalid();
const boolean = (v: unknown): boolean =>
  typeof v === "boolean" ? v : invalid();
const array = <T>(v: unknown, f: (x: unknown) => T, max = 20000): T[] =>
  Array.isArray(v) && v.length <= max ? v.map((x) => f(x)) : invalid();
const choice = <T extends string>(v: unknown, values: readonly T[]): T =>
  typeof v === "string" && values.includes(v as T) ? (v as T) : invalid();
const nullable = <T>(v: unknown, f: (x: unknown) => T): T | null =>
  v === null ? null : f(v);
const hash = (v: unknown): string =>
  /^[a-f0-9]{64}$/.test(str(v, 64)) ? (v as string) : invalid();
const source = (v: unknown): Source => choice(v, ["JM", "Pica"]);
const language = (v: unknown): CompletionLanguage =>
  choice(v, ["chinese", "japanese", "other", "unknown"]);
const reference = (v: unknown) => {
  const r = object(v),
    s = source(r.source),
    id = str(r.workId, 24);
  const parsed = parseLibraryReference(s, id);
  return parsed?.workId === id ? parsed : invalid();
};
export function completionMember(v: unknown): CompletionMember {
  const r = object(v);
  if (r.kind === "source")
    return { kind: "source", reference: reference(r.reference) };
  if (r.kind === "phone") return { kind: "phone", name: str(r.name, 4096) };
  if (r.kind === "computer")
    return { kind: "computer", itemId: hash(r.itemId) };
  return invalid();
}
const phase = (v: unknown) =>
  choice(v, ["checking", "complete", "partial", "cancelled", "error"]);
const code = (v: unknown) =>
  nullable(v, (x) =>
    /^[A-Z_0-9]{1,100}$/.test(str(x, 100)) ? (x as string) : invalid(),
  );
const scopesValue = (v: unknown): SourceScope[] => {
  const scopes = array(
    v,
    (x) => {
      const r = object(x);
      return { source: source(r.source), sessionId: str(r.sessionId, 128) };
    },
    2,
  );
  return scopes.length === 2 &&
    new Set(scopes.map((s) => s.source)).size === 2 &&
    scopes.every((s) => s.sessionId)
    ? scopes
    : invalid();
};
function runValue(v: unknown): DiscoveryRun {
  const r = object(v);
  return {
    id: str(r.id, 128),
    phase: phase(r.phase),
    currentAuthor: nullable(r.currentAuthor, str),
    currentSource: nullable(r.currentSource, source),
    currentPage: integer(r.currentPage),
    requestsUsed: integer(r.requestsUsed),
    completedScopes: integer(r.completedScopes),
    totalScopes: integer(r.totalScopes),
    errorCode: code(r.errorCode),
  };
}
export function validateCompletionView(
  v: unknown,
  expected: SourceScope[],
): CompletionView {
  const r = object(v),
    d = object(r.discovery),
    c = object(r.completeness),
    a = object(r.automatic),
    scopes = scopesValue(d.scopes);
  if (
    !scopes.every((s) =>
      expected.some(
        (e) => e.source === s.source && e.sessionId === s.sessionId,
      ),
    )
  )
    throw new SourceError("STALE_SESSION");
  const records = array(d.records, (x) => {
    const q = object(x);
    return {
      work: validateSourceWork(q.work),
      matchedAuthors: array(q.matchedAuthors, str, 1000),
      authorVerified: boolean(q.authorVerified),
      observedAt: integer(q.observedAt),
      scanId: str(q.scanId, 128),
    };
  });
  if (
    new Set(records.map((r) => r.work.source + ":" + r.work.workId)).size !==
    records.length
  )
    return invalid();
  const completeness = {
    revision: integer(c.revision),
    phoneRevision: integer(c.phoneRevision),
    libraryRevision: integer(c.libraryRevision),
    matchesRevision: integer(c.matchesRevision),
    discoveryRevision: integer(c.discoveryRevision),
    evidenceHash: hash(c.evidenceHash),
    groups: array(c.groups, (x) => {
      const q = object(x);
      const memberName = (x: unknown) => {
        const n = object(x);
        return {
          member: completionMember(n.member),
          name: str(n.name, 4096),
          language: language(n.language),
        };
      };
      return {
        groupId: hash(q.groupId),
        title: str(q.title, 4096),
        authors: array(q.authors, str, 1000),
        status: choice(q.status, [
          "missing",
          "downloaded",
          "owned_chinese",
          "waiting_translation",
          "translation_available",
          "translation_downloaded",
          "review_required",
          "unknown",
        ]),
        reasons: array(q.reasons, str, 100),
        sources: array(
          q.sources,
          (x) => {
            const s = object(x);
            return {
              reference: reference(s.reference),
              title: str(s.title, 2000),
              language: language(s.language),
              authorVerified: boolean(s.authorVerified),
            };
          },
          1000,
        ),
        phone: array(q.phone, memberName),
        computer: array(q.computer, memberName),
        eligible: nullable(q.eligible, (x) => {
          const e = object(x);
          return {
            groupId: hash(e.groupId),
            reference: reference(e.reference),
            kind: choice(e.kind, ["missing", "translation"]),
            evidenceHash: hash(e.evidenceHash),
          };
        }),
      };
    }),
  };
  if (completeness.discoveryRevision !== integer(d.revision)) return invalid();
  if (
    new Set(completeness.groups.map((g) => g.groupId)).size !==
    completeness.groups.length
  )
    return invalid();
  for (const group of completeness.groups) {
    const candidate = group.eligible;
    if (
      candidate &&
      (candidate.groupId !== group.groupId ||
        candidate.evidenceHash !== completeness.evidenceHash ||
        !group.sources.some(
          (s) =>
            s.reference.source === candidate.reference.source &&
            s.reference.workId === candidate.reference.workId &&
            s.authorVerified &&
            s.language === "chinese",
        ))
    )
      return invalid();
  }
  return {
    discovery: {
      scopes,
      revision: integer(d.revision),
      run: nullable(d.run, runValue),
      records,
      authors: array(
        d.authors,
        (x) => {
          const q = object(x);
          return {
            author: str(q.author),
            source: source(q.source),
            state: choice(q.state, [
              "idle",
              "checking",
              "complete",
              "partial",
              "cancelled",
              "error",
            ]),
            lastAttemptAt: nullable(q.lastAttemptAt, integer),
            lastCompleteAt: nullable(q.lastCompleteAt, integer),
            observedCount: integer(q.observedCount),
            pagesRead: integer(q.pagesRead),
            errorCode: code(q.errorCode),
          };
        },
        4000,
      ),
    },
    completeness,
    automatic: {
      runId: nullable(a.runId, (x) => str(x, 128)),
      phase: choice(a.phase, [
        "idle",
        "waiting",
        "enqueueing",
        "complete",
        "cancelled",
        "error",
      ]),
      queued: integer(a.queued),
      skipped: integer(a.skipped),
      errorCode: code(a.errorCode),
    },
  };
}
export function validateCompletionSettings(v: unknown): CompletionSettings {
  const r = object(v);
  return {
    revision: integer(r.revision),
    families: array(r.families, (x) => {
      const f = object(x);
      return {
        id: hash(f.id),
        members: array(f.members, completionMember, 50),
      };
    }),
    languages: array(r.languages, (x) => {
      const l = object(x);
      return {
        member: completionMember(l.member),
        language: language(l.language),
      };
    }),
  };
}
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createCompletionAdapter(
  options: { native?: boolean; invoke?: Invoke } = {},
): CompletionAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  const call = async (
    command: string,
    args?: Record<string, unknown>,
  ): Promise<unknown> => {
    if (!(options.native ?? isDesktopRuntime()))
      throw new SourceError("DESKTOP_REQUIRED");
    try {
      return await invoke(command, args);
    } catch (cause) {
      const c = (cause as { code?: unknown })?.code;
      throw new SourceError(
        typeof c === "string" && /^[A-Z_0-9]{1,100}$/.test(c)
          ? c
          : "COMPLETENESS_UNAVAILABLE",
      );
    }
  };
  return {
    read: async (scopes, recheckFiles = false) =>
      validateCompletionView(
        await call("completeness_read", {
          scopes: scopesValue(scopes),
          recheckFiles,
        }),
        scopes,
      ),
    start: async (scopes, authors, automatic, rootId, generation) =>
      validateCompletionView(
        await call("completeness_start", {
          scopes: scopesValue(scopes),
          authors: array(authors, str, 1000),
          automatic: boolean(automatic),
          rootId: nullable(rootId, hash),
          generation: integer(generation),
        }),
        scopes,
      ),
    cancel: async (runId) => {
      await call("completeness_cancel", { runId: str(runId, 128) });
    },
    settings: async () =>
      validateCompletionSettings(await call("completeness_settings_read")),
    family: async (revision, members) =>
      validateCompletionSettings(
        await call("completeness_family_confirm", {
          revision: integer(revision),
          members: array(members, completionMember, 50),
        }),
      ),
    unlink: async (revision, familyId) =>
      validateCompletionSettings(
        await call("completeness_family_unlink", {
          revision: integer(revision),
          familyId: hash(familyId),
        }),
      ),
    language: async (revision, member, value) =>
      validateCompletionSettings(
        await call("completeness_language_set", {
          revision: integer(revision),
          member: completionMember(member),
          language: nullable(value, language),
        }),
      ),
  };
}
export function completionError(cause: unknown): string {
  const c = (cause as { code?: unknown })?.code;
  if (
    c === "LOGIN_REQUIRED" ||
    c === "STALE_SESSION" ||
    c === "DISCOVERY_SCOPE_CHANGED"
  )
    return "账号已变化，请连接 JM 和哔咔后重新检查。";
  if (c === "REVISION_CONFLICT")
    return "名单或核对记录已变化，请刷新后重新操作。";
  if (c === "LIBRARY_NOT_CONFIGURED" || c === "COMPLETENESS_LIBRARY_NOT_READY")
    return "请先选择电脑漫画目录并完成目录读取。";
  return "本次检查或操作未完成，已有记录会保留。请刷新后重试。";
}
