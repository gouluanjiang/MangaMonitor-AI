import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useReaderAccess, sourceReaderRequest } from "./reader-access.tsx";
import type { ReactNode } from "react";
import type {
  AccountSummary,
  Source,
  SourceAdapter,
  SourceWork,
} from "./source-types.ts";
import { accountScope, sourceLabel, sourceWorkKey } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { DownloadInventorySnapshot } from "./download-types.ts";
import { downloadSelectionLimit } from "./download-types.ts";
import {
  createInventoryMatcher,
  inventoryFilterLabels,
  inventoryFilterMatches,
  inventoryLabel,
  inventoryScopeNote,
} from "./inventory-model.ts";
import type { InventoryFilter } from "./inventory-model.ts";
import { sourceErrorMessage } from "./source-runtime.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { SourceLanguageBadge } from "./SourceLanguageBadge.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import { SourceIssues } from "./SourceIssues.tsx";
import { formatWorkDate } from "./work-dates.ts";
import { RecentUpdatesReader } from "./recent-updates.ts";
import type { RecentUpdatesState } from "./recent-updates.ts";
import { bindRecentUpdatesScroll } from "./recent-scroll.ts";
import { CoverInteraction } from "./reader-access.tsx";
import { AuthorLinks } from "./AuthorLinks.tsx";
import { FloatingSelection } from "./FloatingSelection.tsx";
import { useBrowseSession, useBrowseSessionState } from "./useBrowseSession.ts";
import { DownloadWorkButton } from "./DownloadWorkButton.tsx";
import { isContentHidden, rememberContentWork } from "./content-filter.ts";
import { useAuthorCatalogMembership } from "./author-catalog-membership.ts";
import { subscribeAuthorCatalogChanges } from "./author-catalog-events.ts";
import { mergeSourceWorks } from "./source-types.ts";

export function RecentUpdatesPanel({
  active,
  accounts,
  adapter,
  library,
  inventorySnapshot,
  inventoryReady,
  density,
  navigation,
  onOpen,
  onDownload,
  onDownloadMany,
  onAccounts,
}: {
  active: boolean;
  accounts: AccountSummary[];
  adapter: SourceAdapter;
  library: LibrarySnapshot;
  inventorySnapshot: DownloadInventorySnapshot;
  inventoryReady: boolean;
  density: 5 | 7 | 9;
  navigation: ReactNode;
  onOpen(work: SourceWork): void;
  onDownload(work: SourceWork): void;
  onDownloadMany(works: SourceWork[]): Promise<string[]> | void;
  onAccounts(): void;
}) {
  const readerAccess = useReaderAccess();
  const root = useRef<HTMLElement>(null);
  const membership = useAuthorCatalogMembership(accounts, active);
  const [source, setSource] = useState<Source>("Pica");
  const scope = accountScope(
    accounts.find((account) => account.source === source),
  );
  const scopeKey = JSON.stringify(scope);
  const [observed, setObserved] = useState<{
    key: string;
    adapter: SourceAdapter;
    reader: RecentUpdatesReader;
    state: RecentUpdatesState;
  } | null>(null);
  const current =
    observed?.key === scopeKey && observed.adapter === adapter
      ? observed
      : null;
  const reader = current?.reader ?? null;
  const state = current?.state ?? null;
  const data = state?.snapshot ?? null;
  const busy = state?.phase === "reading";
  const [filter, setFilter] = useBrowseSessionState<InventoryFilter>(
    "recent-filter:" + scopeKey,
    "all",
  );
  const [query, setQuery] = useBrowseSessionState(
    "recent-query:" + scopeKey,
    "",
  );
  const [selectionMode, setSelectionMode] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const sentinel = useRef<HTMLDivElement>(null);
  const grid = useRef<SourceGridHandle>(null);
  const readers = useRef(
    new Map<string, { adapter: SourceAdapter; reader: RecentUpdatesReader }>(),
  );
  const currentView = useRef({ active, scopeKey });
  currentView.current = { active, scopeKey };
  useEffect(() => {
    setSelection([]);
    setSelectionMode(false);
    if (!scope) {
      setObserved(null);
      return;
    }
    // Create inside the effect: StrictMode cleanup must not permanently dispose
    // the memoized reader reused by its second setup.
    let retained = readers.current.get(scopeKey);
    if (retained && retained.adapter !== adapter) {
      retained.reader.dispose();
      retained = undefined;
    }
    const next = retained?.reader ?? new RecentUpdatesReader(adapter, scope);
    readers.current.set(scopeKey, { adapter, reader: next });
    let previous: RecentUpdatesState["snapshot"] = null;
    const unsubscribe = next.subscribe((state) => {
      const snapshot = state.snapshot;
      snapshot?.items.forEach(rememberContentWork);
      if (
        currentView.current.active &&
        currentView.current.scopeKey === scopeKey &&
        snapshot &&
        previous &&
        (snapshot.page > previous.page ||
          (snapshot.page === previous.page &&
            snapshot.items.some(
              (work, index) => work.tags !== previous?.items[index]?.tags,
            ))) &&
        snapshot.items.length >= previous.items.length &&
        previous.items.every(
          (work, index) =>
            sourceWorkKey(work) === sourceWorkKey(snapshot.items[index]),
        )
      ) {
        // Capture when the response arrives, not when it was requested: the
        // reader may have moved elsewhere while waiting. The grid restores
        // this card after commit and yields to any new scroll input.
        let anchor = grid.current?.capture() ?? null;
        if (anchor) {
          const index = snapshot.items.findIndex(
            (work) => sourceWorkKey(work) === anchor?.key,
          );
          if (index >= 0 && isContentHidden(snapshot.items[index])) {
            const neighbor = snapshot.items
              .slice(index + 1)
              .find((work) => !isContentHidden(work));
            anchor = neighbor
              ? (grid.current?.capture(sourceWorkKey(neighbor)) ?? null)
              : null;
          }
        }
        grid.current?.restore(anchor);
      }
      previous = snapshot;
      setObserved({ key: scopeKey, adapter, reader: next, state });
    });
    return () => {
      unsubscribe();
    };
  }, [adapter, scopeKey]);
  useEffect(
    () => () => {
      for (const value of readers.current.values()) value.reader.dispose();
      readers.current.clear();
    },
    [],
  );
  useEffect(() => {
    if (active) void reader?.start();
    else setSelection([]);
  }, [reader, active]);
  useEffect(() => {
    if (!active || !reader || !scope) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = subscribeAuthorCatalogChanges((change) => {
      if (
        change.source !== scope.source ||
        change.sessionId !== scope.sessionId
      )
        return;
      clearTimeout(timer);
      timer = setTimeout(() => void reader.refreshHistory(), 500);
    });
    return () => {
      unsubscribe();
      clearTimeout(timer);
    };
  }, [reader, active, scopeKey]);

  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const liveKeys = new Set(data?.items.map(sourceWorkKey) ?? []);
  const retained = (state?.retainedItems ?? []).filter(
    (work) => !liveKeys.has(sourceWorkKey(work)),
  );
  // Keep live order, but do not let a light row erase locally retained labels.
  const enriched = mergeSourceWorks(
    state?.retainedItems ?? [],
    data?.items ?? [],
  );
  const byKey = new Map(enriched.map((work) => [sourceWorkKey(work), work]));
  const displayItems = [...(data?.items ?? []), ...retained].map(
    (work) => byKey.get(sourceWorkKey(work)) ?? work,
  );
  const hiddenCount = displayItems.filter(isContentHidden).length;
  const uncommitted = new Set(state?.uncommittedIds ?? []);
  const eligible = displayItems.filter(
    (work) =>
      !isContentHidden(work) &&
      (uncommitted.has(sourceWorkKey(work)) ||
        !membership.known.has(sourceWorkKey(work))),
  );
  const searched = eligible.filter((work) =>
    [work.title, ...work.authors]
      .join(" ")
      .normalize("NFKC")
      .toLocaleLowerCase()
      .includes(terms),
  );
  const visible = searched.filter((work) =>
    inventoryFilterMatches(inventory(work), filter),
  );
  const counts = Object.fromEntries(
    (Object.keys(inventoryFilterLabels) as InventoryFilter[]).map((value) => [
      value,
      searched.filter((work) => inventoryFilterMatches(inventory(work), value))
        .length,
    ]),
  );
  const selected = visible.filter(
    (work) =>
      selection.includes(sourceWorkKey(work)) &&
      inventory(work).kind !== "owned",
  );
  useBrowseSession({
    scope: JSON.stringify(["recent", scopeKey, query, filter]),
    active,
    root,
    grid,
    itemKeys: visible.map(sourceWorkKey),
  });

  useEffect(() => {
    const target = sentinel.current;
    const main = target?.closest("main");
    if (!active || !reader || !target || !main) return;
    return bindRecentUpdatesScroll(main, target, reader);
  }, [active, reader, terms, filter]);
  useEffect(() => {
    // A suppressed source page is not an end-of-feed signal. Continue only when
    // no browseable record exists; text/inventory filters never trigger a crawl.
    if (
      active &&
      reader &&
      state?.phase === "ready" &&
      data?.hasMore === true &&
      !eligible.length &&
      !terms &&
      filter === "all"
    ) {
      const timer = setTimeout(() => void reader.loadNext(), 0);
      return () => clearTimeout(timer);
    }
  }, [
    active,
    reader,
    state?.phase,
    data?.page,
    eligible.length,
    terms,
    filter,
  ]);
  const clearSelection = () => {
    setSelection([]);
    setSelectionMode(false);
  };
  const error = state?.error ? sourceErrorMessage(state.error) : "";
  return (
    <section
      ref={root}
      className={
        "source-workbench recent-updates" +
        (selected.length ? " has-source-selection" : "")
      }
      data-testid="recent-panel"
      hidden={!active}
    >
      <div className="page-heading source-heading">
        <div>
          <h1>最近更新</h1>
          <p>浏览 JM 与哔咔的新近作品，按网站提供的顺序展示。</p>
        </div>
      </div>
      {navigation}
      <div className="source-toolbar source-page-tools">
        <div className="source-toolbar-leading">
          <label>
            来源{" "}
            <select
              aria-label="最近更新来源"
              value={source}
              onChange={(event) => {
                setSource(event.target.value as Source);
                clearSelection();
              }}
            >
              <option value="Pica">哔咔</option>
              <option value="JM">JM</option>
            </select>
          </label>
          <button
            className="text-button"
            disabled={!scope || busy}
            onClick={() => {
              clearSelection();
              void reader?.refresh();
            }}
          >
            刷新最近更新
          </button>
        </div>
        <div className="source-search">
          <input
            type="search"
            aria-label="筛选已读取最近更新"
            placeholder="筛选已读取作品或作者…"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              clearSelection();
            }}
          />
        </div>
      </div>
      {!scope ? (
        <div className="source-empty">
          <p>请先连接{sourceLabel(source)}账号。</p>
          <button onClick={onAccounts}>前往账号设置</button>
        </div>
      ) : (
        <>
          {error && (
            <p role="alert" className="source-notice">
              {error} {data ? "已读取列表保留。" : "请点击重试读取。"}
            </p>
          )}
          {state?.observationErrorCode && (
            <p
              role="alert"
              className="source-notice"
              data-testid="recent-unsaved"
            >
              本页已经读取，但作者目录补录尚未保存；作品继续保留在此处。请重试读取，保存成功后才会移入作者更新。
            </p>
          )}
          {state?.historyError && (
            <p role="alert" className="source-notice">
              近期补漏历史暂未读取成功；当前来源结果保留，可刷新最近更新重试。
            </p>
          )}
          <div
            className="source-tabs"
            role="group"
            aria-label="最近更新入库筛选"
          >
            {(Object.keys(inventoryFilterLabels) as InventoryFilter[]).map(
              (value) => (
                <button
                  key={value}
                  aria-pressed={filter === value}
                  onClick={() => {
                    setFilter(value);
                    clearSelection();
                  }}
                >
                  {inventoryFilterLabels[value]} {counts[value]}
                </button>
              ),
            )}
          </div>
          <p className="page-summary" data-testid="recent-counts">
            已读取 {data?.items.length ?? 0} 部
            {data?.total !== null && data?.total !== undefined
              ? ` / 来源报告 ${data.total} 条`
              : ""}{" "}
            · 已入库 {counts.owned} 部 · 未入库 {counts.missing} 部 · 当前显示{" "}
            {visible.length} 部。统计仅覆盖已读取的 {data?.page ?? 0} 页。
            {(data?.duplicates ?? 0) > 0
              ? `已合并 ${data!.duplicates} 条重复记录。`
              : ""}
            {retained.length > 0
              ? ` 另有已保存近期作品 ${retained.length} 部，未计入本次网站分页。`
              : ""}
          </p>
          <p className="source-muted" data-testid="content-preference-summary">
            按明确 BL／耽美、AI 标签{source === "JM" ? "及女性向标签" : ""}隐藏{" "}
            {hiddenCount} 部。
            仅使用已有标签；标签未知的作品正常显示，不额外读取详情。
          </p>
          <SourceIssues
            source={source}
            issues={data?.issues}
            pagesComplete={state?.phase === "complete"}
            testId="recent-issues"
          />
          <details className="page-scope-details">
            <summary>浏览范围与排序说明 · 仅筛选已读取作品</summary>
            <p className="source-muted" data-testid="recent-order-note">
              按来源最新顺序浏览，日期以网站提供为准；不保证每次章节更新都会排到前面。筛选仅覆盖已读取范围。
            </p>
            {data && (
              <p className="source-muted">
                最近读取：{new Date(data.updatedAt).toLocaleString()} ·{" "}
                {inventoryScopeNote}
              </p>
            )}
            {state?.retainedCoverage && (
              <p
                className="source-muted"
                data-testid="recent-retained-coverage"
              >
                已保存近期补漏范围：{state.retainedCoverage.pagesRead} 页
                {state.retainedCoverage.checkedAt
                  ? ` · ${new Date(state.retainedCoverage.checkedAt).toLocaleString()}`
                  : ""}
                {state.retainedCoverage.errorCode
                  ? " · 范围未完成，已有记录保留。"
                  : state.retainedCoverage.reachedEnd
                    ? " · 本次入口分页已读完。"
                    : state.retainedCoverage.joinedPrevious
                      ? " · 已衔接上次已确认范围。"
                      : state.retainedCoverage.initialWindow
                        ? " · 已建立初始窗口，更早历史未覆盖。"
                        : " · 已保存部分范围，未证明完整。"}
                不代表全站历史作品均已覆盖。
              </p>
            )}
          </details>
          {visible.length > 0 && (
            <div className="source-toolbar">
              <div className="source-toolbar-leading">
                {selectionMode && (
                  <button
                    className="text-button"
                    onClick={() =>
                      setSelection(
                        visible
                          .filter((work) => inventory(work).kind !== "owned")
                          .map(sourceWorkKey),
                      )
                    }
                  >
                    全选当前筛选范围
                  </button>
                )}
              </div>
            </div>
          )}
          <VirtualSourceGrid<SourceWork>
            ref={grid}
            items={visible}
            density={density}
            itemKey={sourceWorkKey}
            testId="recent-grid"
            renderItem={(work) => (
              <article
                className="source-card"
                data-testid={"recent-work-" + sourceWorkKey(work)}
              >
                <div className="source-card-cover">
                  <CoverInteraction
                    className="source-cover-button source-language-cover"
                    title={work.title}
                    request={sourceReaderRequest(scope, work)}
                    onDetails={() => onOpen(work)}
                    selectionMode={selectionMode}
                    selected={selection.includes(sourceWorkKey(work))}
                    onToggleSelection={() => {
                      if (inventory(work).kind !== "owned")
                        setSelection((previous) =>
                          previous.includes(sourceWorkKey(work))
                            ? previous.filter(
                                (key) => key !== sourceWorkKey(work),
                              )
                            : [...previous, sourceWorkKey(work)],
                        );
                    }}
                  >
                    <SourceCover adapter={adapter} scope={scope} work={work} />
                    <SourceLanguageBadge
                      tags={work.tags}
                      work={work}
                      scope={scope}
                    />
                  </CoverInteraction>
                  {selectionMode && (
                    <input
                      type="checkbox"
                      aria-label={"选择 " + work.title}
                      checked={selection.includes(sourceWorkKey(work))}
                      disabled={inventory(work).kind === "owned"}
                      onChange={(event) =>
                        setSelection((previous) =>
                          event.target.checked
                            ? [...previous, sourceWorkKey(work)]
                            : previous.filter(
                                (key) => key !== sourceWorkKey(work),
                              ),
                        )
                      }
                    />
                  )}
                </div>
                <h3>
                  <button onClick={() => onOpen(work)}>{work.title}</button>
                </h3>
                <AuthorLinks authors={work.authors} />
                <p className="source-card-state">
                  {inventoryLabel(inventory(work))}
                </p>
                <p
                  className="source-card-date"
                  title={
                    formatWorkDate(work.sourceUpdatedAt, true) ?? undefined
                  }
                >
                  {formatWorkDate(work.sourceUpdatedAt)
                    ? `更新：${formatWorkDate(work.sourceUpdatedAt)}`
                    : "更新时间未知"}
                </p>
                <DownloadWorkButton
                  work={work}
                  owned={inventory(work).kind === "owned"}
                  ready={!!library.rootId}
                  onClick={() => onDownload(work)}
                />
              </article>
            )}
          />
          {!busy && !visible.length && (
            <p className="source-empty">
              {data
                ? data.items.length === 0 &&
                  state?.phase === "complete" &&
                  !data.issues?.length
                  ? "来源当前没有返回作品。"
                  : "已读取范围内没有符合筛选的作品。"
                : "尚未读取最近更新。"}
            </p>
          )}
          <div
            ref={sentinel}
            className="collection-status"
            data-testid="recent-sentinel"
          >
            <p role="status" data-testid="recent-progress">
              {busy
                ? "正在读取最近更新…"
                : state?.phase === "error"
                  ? "读取已停止，已读内容保留。"
                  : state?.phase === "limited"
                    ? "达到 20000 条、1000 页的浏览上限；已读内容保留，可刷新后重新浏览。"
                    : state?.phase === "complete"
                      ? data?.issues?.length
                        ? "来源本次返回的分页已读完，仍有记录待核对。"
                        : "来源本次返回的分页已读完。"
                      : terms || filter !== "all"
                        ? "仅筛选已读取范围；向下滚动或点击读取下一页可继续查找。"
                        : data?.hasMore === null
                          ? "来源未确认后续范围，可点击读取下一页继续。"
                          : "向下滚动或点击读取下一页继续浏览。"}
            </p>
            {state?.phase === "error" ? (
              <button
                className="text-button"
                onClick={() => void reader?.retry()}
              >
                重试读取
              </button>
            ) : (
              state?.phase !== "complete" &&
              state?.phase !== "limited" && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => void reader?.loadNext()}
                >
                  读取下一页
                </button>
              )
            )}
          </div>
          <FloatingSelection
            visible={active}
            active={selectionMode}
            selectedCount={selected.length}
            onEnter={() => setSelectionMode(true)}
            onCancel={clearSelection}
            disabled={
              !library.rootId || selected.length > downloadSelectionLimit
            }
            onDownload={() => {
              void Promise.resolve(
                onDownloadMany(
                  selected.filter((work) => !isContentHidden(work)),
                ),
              ).then((keys) => {
                if (keys)
                  setSelection((previous) =>
                    previous.filter((key) => !keys.includes(key)),
                  );
              });
            }}
          >
            <span>当前已读取范围</span>
          </FloatingSelection>
          {membership.failed && (
            <p className="source-muted">
              作者更新目录暂未读到，当前去重范围可能不完整。
            </p>
          )}
        </>
      )}
    </section>
  );
}
