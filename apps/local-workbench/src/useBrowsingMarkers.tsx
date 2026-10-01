import { useEffect, useMemo, useSyncExternalStore } from "react";
import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { accountScope, sourceLabel, sourceWorkKey } from "./source-types.ts";
import type {
  AccountSummary,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import type { RecentSourceState } from "./combined-recent.ts";
import type { DiscoverySnapshot } from "./completion-types.ts";
import { partitionAuthorRecords } from "./author-evidence.ts";
import {
  BrowsingMarkerTracker,
  validateBrowsingDocument,
} from "./browsing-markers.ts";
import type { BrowsingBaseline, BrowsingSurface } from "./browsing-markers.ts";
import "./browsing-markers.css";

interface Entry {
  scope: SourceScope;
  surface: BrowsingSurface;
  tracker: BrowsingMarkerTracker | null;
  loading: boolean;
  writing: boolean;
  error: string | null;
  saved: BrowsingBaseline | null;
  pending: BrowsingBaseline | null;
}
const entries = new Map<string, Entry>();
const listeners = new Set<() => void>();
let revision = 0;
const publish = () => {
  revision++;
  for (const listener of listeners) listener();
};
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
const snapshot = () => revision;
const keyFor = (account: AccountSummary, surface: BrowsingSurface) =>
  JSON.stringify([account.source, account.accountId, surface]);
const code = (error: unknown) => {
  const value = (error as { code?: unknown })?.code;
  return typeof value === "string" && /^[A-Z0-9_]{1,80}$/.test(value)
    ? value
    : "BROWSING_BASELINE_UNAVAILABLE";
};

async function save(entry: Entry) {
  if (entry.writing || !entry.pending) return;
  entry.writing = true;
  const next = entry.pending;
  const scope = entry.scope;
  entry.pending = null;
  try {
    validateBrowsingDocument(
      await invokeDesktop("browsing_markers_write", {
        ...scope,
        surface: entry.surface,
        baseline: next,
      }),
    );
    if (entry.scope.sessionId === scope.sessionId) {
      entry.saved = next;
      entry.error = null;
    }
  } catch (error) {
    if (entry.scope.sessionId === scope.sessionId) entry.error = code(error);
  } finally {
    entry.writing = false;
    publish();
    if (entry.pending) void save(entry);
  }
}

function getEntry(
  account: AccountSummary,
  surface: BrowsingSurface,
): Entry | null {
  const scope = accountScope(account);
  if (!scope || !account.accountId || !isDesktopRuntime()) return null;
  const key = keyFor(account, surface);
  let entry = entries.get(key);
  if (!entry) {
    entry = {
      scope,
      surface,
      tracker: null,
      loading: false,
      writing: false,
      error: null,
      saved: null,
      pending: null,
    };
    entries.set(key, entry);
  }
  if (entry.scope.sessionId !== scope.sessionId) {
    entry.scope = scope;
    entry.error = null;
    entry.loading = false;
    if (entry.tracker?.next && entry.tracker.next !== entry.saved) {
      entry.pending = entry.tracker.next;
      void save(entry);
    }
  }
  if (!entry.tracker && !entry.loading && !entry.error) {
    entry.loading = true;
    const current = entry;
    void invokeDesktop("browsing_markers_read", { ...scope, surface })
      .then((value) => {
        if (current.scope.sessionId !== scope.sessionId) return;
        const baseline = validateBrowsingDocument(value);
        current.tracker = new BrowsingMarkerTracker(baseline);
        current.saved = baseline;
      })
      .catch((error) => {
        if (current.scope.sessionId === scope.sessionId)
          current.error = code(error);
      })
      .finally(() => {
        if (current.scope.sessionId === scope.sessionId)
          current.loading = false;
        publish();
      });
  }
  return entry;
}

function observed(entry: Entry, before: number) {
  if (!entry.tracker || before === entry.tracker.revision) return;
  if (entry.tracker.next && entry.tracker.next !== entry.saved) {
    entry.pending = entry.tracker.next;
    void save(entry);
  }
  publish();
}

function result(accounts: AccountSummary[], surface: BrowsingSurface) {
  const keys = new Set<string>();
  const notes: string[] = [];
  for (const account of accounts) {
    if (!accountScope(account)) continue;
    const entry = entries.get(keyFor(account, surface));
    const label = sourceLabel(account.source);
    if (!account.accountId) {
      notes.push(`${label}账号身份尚未确认，新增暂不判定。`);
      continue;
    }
    if (entry?.error)
      notes.push(
        `${label}浏览基线暂不可用，已显示角标保留；下次浏览比较可能不完整。`,
      );
    else if (entry?.tracker?.limited)
      notes.push(`${label}浏览基线已达保存上限，超出范围暂不判定新增。`);
    else if (entry?.tracker?.status === "partial")
      notes.push(
        surface === "recent"
          ? `${label}尚未完整接回上次浏览的头部；未确认范围不标新增。`
          : `${label}作者目录仍有未确认范围；角标仅比较已保存作品。`,
      );
    else if (entry?.tracker?.status === "initial")
      notes.push(`${label}首次浏览已建立基线，现有作品不标新增。`);
    else if (!entry?.tracker || entry.tracker.status === "waiting")
      notes.push(`${label}正在读取浏览基线。`);
    for (const id of entry?.tracker?.newIds ?? [])
      keys.add(sourceWorkKey({ source: account.source, workId: id }));
  }
  const canRetry = accounts.some(
    (account) => !!entries.get(keyFor(account, surface))?.error,
  );
  const retry = () => {
    for (const account of accounts) {
      const entry = entries.get(keyFor(account, surface));
      if (!entry?.error || !accountScope(account)) continue;
      entry.error = null;
      if (entry.tracker?.next) {
        entry.pending = entry.tracker.next;
        void save(entry);
      }
    }
    publish();
  };
  return { keys, notes, retry: canRetry ? retry : undefined };
}

export function useRecentBrowsingMarkers(
  accounts: AccountSummary[],
  sources: RecentSourceState[],
  active: boolean,
  hasHistory: boolean,
) {
  const version = useSyncExternalStore(subscribe, snapshot);
  const accountKey = JSON.stringify(
    accounts.map(({ source, accountId, sessionId, state }) => [
      source,
      accountId,
      sessionId,
      state,
    ]),
  );
  useEffect(() => {
    if (!active) return;
    for (const { source, state } of sources) {
      const account = accounts.find((value) => value.source === source);
      if (!account) continue;
      const entry = getEntry(account, "recent");
      if (!entry?.tracker || !state) continue;
      const before = entry.tracker.revision;
      entry.tracker.observeRecent({
        liveIds: state.snapshot?.items.map((work) => work.workId) ?? null,
        savedIds: state.retainedItems?.map((work) => work.workId) ?? [],
        historyReady: !hasHistory || state.retainedItems !== undefined,
        hasIssues: !!state.snapshot?.issues?.length,
        reachedEnd: state.snapshot?.hasMore === false,
        failed: state.phase === "error" || !!state.historyError,
      });
      observed(entry, before);
    }
  }, [active, accountKey, sources, hasHistory, version]);
  return result(
    accounts.filter((account) =>
      sources.some((value) => value.source === account.source),
    ),
    "recent",
  );
}

export function useAuthorBrowsingMarkers(
  accounts: AccountSummary[],
  view: DiscoverySnapshot | null,
  active: boolean,
  readFailed: boolean,
) {
  const version = useSyncExternalStore(subscribe, snapshot);
  const accountKey = JSON.stringify(
    accounts.map(({ source, accountId, sessionId, state }) => [
      source,
      accountId,
      sessionId,
      state,
    ]),
  );
  // Filters, sort, and other-keyword tabs never alter the baseline's catalog scope.
  const confirmed = useMemo(
    () =>
      view
        ? partitionAuthorRecords(
            view.records,
            "",
            "all",
            view.authorPolicies,
            view.followedAuthors ?? [
              ...new Set(view.authors.map((range) => range.author)),
            ],
          ).confirmed
        : [],
    [view],
  );
  useEffect(() => {
    if (!active || !view || readFailed || view.historicalReadError) return;
    for (const account of accounts) {
      const entry = getEntry(account, "authors");
      if (!entry?.tracker) continue;
      const before = entry.tracker.revision;
      const incomplete =
        !!view.observationErrorCode ||
        view.authors.some(
          (range) =>
            range.source === account.source &&
            (range.state !== "complete" || !!range.issueCount),
        );
      entry.tracker.observeAuthors(
        [
          ...new Set(
            confirmed
              .filter((record) => record.work.source === account.source)
              .map((record) => record.work.workId),
          ),
        ],
        incomplete,
      );
      observed(entry, before);
    }
  }, [active, accountKey, view, confirmed, readFailed, version]);
  return result(accounts, "authors");
}

export function BrowsingNewBadge({
  work,
  marked,
}: {
  work: Pick<SourceWork, "source" | "workId">;
  marked: Set<string>;
}) {
  return marked.has(sourceWorkKey(work)) ? (
    <span
      className="browsing-new-badge"
      data-testid="browsing-new-badge"
      title="与上次浏览相比新增，本次启动期间保留"
    >
      新增
    </span>
  ) : null;
}

export function BrowsingMarkerNote({
  notes,
  surface,
  onRetry,
}: {
  notes: string[];
  surface: "recent" | "authors";
  onRetry?: () => void;
}) {
  return (
    <p
      className="source-muted browsing-marker-note"
      data-testid={`${surface}-browsing-note`}
    >
      新增角标与上次浏览比较，本次启动期间保留；下次启动更新基线。
      {surface === "authors" &&
        "目录新收录不代表新发表；与本次检查新发现、特别关注未读分别记录。"}
      {notes.length > 0 && <span>{notes.join(" ")}</span>}
      {onRetry && (
        <button className="text-button" onClick={onRetry}>
          重试浏览基线
        </button>
      )}
    </p>
  );
}
