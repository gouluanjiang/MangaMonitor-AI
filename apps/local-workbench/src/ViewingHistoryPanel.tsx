import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { useViewingHistory } from "./useViewingHistory.ts";
import type { HistoryIdentity } from "./history-runtime.ts";
import type { LibraryAdapter, LibrarySnapshot } from "./library-types.ts";
import type { AccountSummary, SourceAdapter } from "./source-types.ts";
import { sourceLabel } from "./source-types.ts";
import {
  getContentFilterRevision,
  subscribeContentFilter,
} from "./content-filter.ts";
import {
  historyCoverTarget,
  historyEntryVisible,
  historyIdentityKey,
} from "./history-covers.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { LibraryCover } from "./LibraryWorkbench.tsx";
import { useBrowseSession } from "./useBrowseSession.ts";
import { formatTimestamp } from "./work-dates.ts";
import "./viewing-history.css";
export function ViewingHistoryPanel({
  active,
  history,
  sourceAdapter,
  accounts,
  libraryAdapter,
  librarySnapshot,
  onOpen,
}: {
  active: boolean;
  history: ReturnType<typeof useViewingHistory>;
  sourceAdapter: SourceAdapter;
  accounts: AccountSummary[];
  libraryAdapter: LibraryAdapter;
  librarySnapshot: LibrarySnapshot;
  onOpen: (identity: HistoryIdentity) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [confirm, setConfirm] = useState(false);
  const [enabledDraft, setEnabledDraft] = useState<boolean | null>(null);
  useEffect(() => {
    if (active) void history.refresh();
  }, [active, history.refresh]);
  useSyncExternalStore(
    subscribeContentFilter,
    getContentFilterRevision,
    getContentFilterRevision,
  );
  const entries =
    history.snapshot?.entries.filter((entry) =>
      historyEntryVisible(entry.identity, librarySnapshot),
    ) ?? [];
  useBrowseSession({
    scope: "viewing-history",
    active,
    root,
    itemKeys: entries.map((entry) => historyIdentityKey(entry.identity)),
  });
  if (!active) return null;
  return (
    <div
      ref={root}
      className="viewing-history-panel source-workbench"
      data-testid="viewing-history"
    >
      <header className="page-heading source-heading">
        <div>
          <h1>浏览历史</h1>
          <p>最近打开的 100 本漫画。只有打开详情或成功进入阅读器才会记录。</p>
        </div>
      </header>
      <div className="source-actions">
        <label className="source-check">
          <input
            type="checkbox"
            checked={enabledDraft ?? history.snapshot?.enabled ?? true}
            disabled={
              !history.snapshot || history.busy || enabledDraft !== null
            }
            onChange={(event) => {
              const enabled = event.target.checked;
              setEnabledDraft(enabled);
              void history
                .setEnabled(enabled)
                .finally(() => setEnabledDraft(null));
            }}
          />
          记录浏览历史
        </label>
        {enabledDraft !== null && <span role="status">正在保存记录设置…</span>}
        <button
          className="button secondary"
          onClick={() => void history.refresh()}
          disabled={history.busy}
        >
          重新读取
        </button>
        <button
          className="button secondary"
          onClick={() => setConfirm(true)}
          disabled={history.busy || !history.snapshot?.entries.length}
        >
          清空历史
        </button>
      </div>
      {confirm && (
        <div role="alert">
          <p>清空这份浏览记录？漫画文件、下载、关注和阅读进度都会保留。</p>
          <button
            className="button secondary"
            onClick={() => {
              setConfirm(false);
              void history.clear();
            }}
          >
            确认清空
          </button>
          <button
            className="button secondary"
            onClick={() => setConfirm(false)}
          >
            取消
          </button>
        </div>
      )}
      {history.error && <p role="status">{history.error}</p>}
      {history.snapshot && !history.snapshot.enabled && (
        <p>记录已关闭，现有历史仍可查看。</p>
      )}
      {!entries.length && (
        <p>{history.snapshot ? "暂无浏览记录。" : "正在读取浏览历史…"}</p>
      )}
      <ol className="viewing-history-list">
        {entries.map((entry) => {
          const cover = historyCoverTarget(entry, accounts, librarySnapshot);
          const key = historyIdentityKey(entry.identity);
          return (
            <li key={key} data-browse-key={key}>
              <div
                className="viewing-history-cover"
                data-testid="history-cover"
              >
                {cover.kind === "source" ? (
                  <SourceCover
                    adapter={sourceAdapter}
                    scope={cover.scope}
                    work={cover.work}
                    resolveMissing
                  />
                ) : cover.kind === "library" ? (
                  <LibraryCover
                    adapter={libraryAdapter}
                    snapshot={librarySnapshot}
                    item={cover.item}
                  />
                ) : (
                  <div className="source-cover">
                    <span>{cover.message}</span>
                  </div>
                )}
              </div>
              <div className="viewing-history-copy">
                <button
                  className="text-button viewing-history-title"
                  onClick={() => onOpen(entry.identity)}
                >
                  {entry.title}
                </button>
                <span>
                  {entry.identity.kind === "library"
                    ? "漫画库"
                    : sourceLabel(entry.identity.source)}{" "}
                  · {formatTimestamp(entry.visitedAt)}
                </span>
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
