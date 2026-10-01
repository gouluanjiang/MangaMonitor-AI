import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { AccountSummary, SourceScope } from "./source-types.ts";
import type { WorkReference } from "./booklists.ts";
import { accountScope } from "./source-types.ts";
import { invokeDesktop } from "./runtime.ts";
import { isContentHidden } from "./content-filter.ts";
import { subscribeSourceVisits } from "./work-visits.ts";
import { subscribeAuthorCatalogChanges } from "./author-catalog-events.ts";
import {
  specialRunMessage,
  validateSpecialRun,
  validateSpecialSnapshot,
} from "./special-runtime.ts";
import type { SpecialRun, SpecialSnapshot } from "./special-runtime.ts";

export function useSpecialFollows(
  accounts: AccountSummary[],
  native: boolean,
  notify: (message: string) => void,
) {
  const scopes = accounts
    .map(accountScope)
    .filter((scope): scope is SourceScope => scope !== null);
  const key = JSON.stringify(scopes);
  const current = useRef({ key, scopes, notify });
  current.current = { key, scopes, notify };
  const [saved, setSaved] = useState<{
    key: string;
    value: SpecialSnapshot;
  } | null>(null);
  const [runState, setRun] = useState<{
    key: string;
    value: SpecialRun;
  } | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  const notified = useRef(new Set<number>());
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const snapshot = saved?.key === key ? saved.value : null;
  const run = runState?.key === key ? runState.value : snapshot?.run;
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const applyRun = useCallback((value: SpecialRun, capturedKey: string) => {
    if (!alive.current || current.current.key !== capturedKey) return;
    setRun({ key: capturedKey, value });
    if (
      value.id &&
      value.finishedAt &&
      value.phase !== "idle" &&
      value.phase !== "cancelled" &&
      !notified.current.has(value.id)
    ) {
      notified.current.add(value.id);
      current.current.notify(specialRunMessage(value));
    }
  }, []);
  const request = useCallback(
    (command: string, args: Record<string, unknown> = {}) => {
      const captured = current.current;
      const task = chain.current
        .catch(() => {})
        .then(async () => {
          if (
            !alive.current ||
            current.current.key !== captured.key ||
            captured.scopes.length !== 2
          )
            return;
          try {
            const raw = await invokeDesktop<unknown>(command, {
              scopes: captured.scopes,
              ...args,
            });
            if (!alive.current || current.current.key !== captured.key) return;
            if (command === "special_start" || command === "special_cancel")
              applyRun(validateSpecialRun(raw), captured.key);
            else {
              const value = validateSpecialSnapshot(raw, captured.scopes);
              setSaved({ key: captured.key, value });
              applyRun(value.run, captured.key);
            }
            setError("");
          } catch {
            if (alive.current && current.current.key === captured.key)
              setError("特别关注状态暂未读取，请重试；已有记录保留。");
          }
        });
      chain.current = task;
      return task;
    },
    [applyRun],
  );
  useEffect(() => {
    setError("");
    if (native && scopes.length === 2) void request("special_read");
  }, [key, native, request]);
  useEffect(() => {
    if (
      !native ||
      scopes.length !== 2 ||
      !run ||
      !["waiting", "checking"].includes(run.phase)
    )
      return;
    let cancelled = false;
    const capturedKey = key;
    const timer = setTimeout(async () => {
      try {
        const next = validateSpecialRun(
          await invokeDesktop<unknown>("special_progress"),
        );
        if (cancelled || current.current.key !== capturedKey) return;
        applyRun(next, capturedKey);
        if (!["waiting", "checking"].includes(next.phase))
          await request("special_read");
      } catch {
        if (!cancelled)
          setError("检查进度暂未读取；刷新状态可重试，不代表后台已停止。");
      }
    }, 2500);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [run, key, native, request, applyRun]);
  useEffect(
    () =>
      subscribeSourceVisits((visit) => {
        if (
          !native ||
          !current.current.scopes.some(
            (scope) =>
              scope.source === visit.scope.source &&
              scope.sessionId === visit.scope.sessionId,
          )
        )
          return;
        if (
          snapshotRef.current?.updates.some(
            (update) =>
              !update.readAt &&
              update.work.source === visit.reference.source &&
              update.work.workId === visit.reference.workId,
          )
        )
          void request("special_mark_read", { identity: visit.reference });
      }),
    [native, request],
  );
  useEffect(
    () =>
      subscribeAuthorCatalogChanges((change) => {
        if (
          native &&
          change.followingRevision !== undefined &&
          current.current.scopes.some(
            (scope) =>
              scope.source === change.source &&
              scope.sessionId === change.sessionId,
          )
        )
          void request("special_read");
      }),
    [native, request],
  );
  async function set(author: string, enabled: boolean) {
    if (busy) return;
    setBusy(true);
    try {
      await request("special_set", { author, enabled });
    } finally {
      setBusy(false);
    }
  }
  return {
    snapshot,
    run,
    error,
    busy,
    connected: native && scopes.length === 2,
    unread:
      snapshot?.updates.filter(
        (update) => update.readAt === null && !isContentHidden(update.work),
      ).length ?? 0,
    set,
    enabled: (author: string) =>
      snapshot?.authors.some((row) => row.author === author && row.enabled) ??
      false,
    refresh: () => request("special_read"),
    start: () => request("special_start"),
    cancel: () => request("special_cancel"),
    markRead: (identity: WorkReference | null = null) =>
      request("special_mark_read", { identity }),
  };
}
type SpecialState = ReturnType<typeof useSpecialFollows>;
export const SpecialFollowsContext = createContext<SpecialState | null>(null);
export function SpecialFollowButton({ author }: { author: string }) {
  const state = useContext(SpecialFollowsContext);
  if (!state) return null;
  const enabled = state.enabled(author);
  return (
    <button
      type="button"
      className="text-button"
      disabled={!state.connected || state.busy}
      aria-pressed={enabled}
      onClick={() => void state.set(author, !enabled)}
      title="取消特别关注仍保留普通关注；首次建立基线不把旧作算作新作"
    >
      {enabled ? "取消特别关注" : "设为特别关注"}
    </button>
  );
}
