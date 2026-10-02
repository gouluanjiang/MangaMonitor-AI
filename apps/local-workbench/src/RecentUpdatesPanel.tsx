import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useReaderAccess, sourceReaderRequest } from "./reader-access.tsx";
import type { ReactNode } from "react";
import type {
  AccountSummary,
  Source,
  SourceAdapter,
  SourceScope,
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
import { RecentUpdatesView } from "./combined-recent.ts";
import type { RecentSourceChoice, RecentViewState } from "./combined-recent.ts";
import { bindRecentUpdatesScroll } from "./recent-scroll.ts";
import { CoverInteraction } from "./reader-access.tsx";
import { AuthorLinks } from "./AuthorLinks.tsx";
import { FloatingSelection } from "./FloatingSelection.tsx";
import { useBrowseSession, useBrowseSessionState } from "./useBrowseSession.ts";
import { DownloadWorkButton } from "./DownloadWorkButton.tsx";
import { isContentHidden, rememberContentWork } from "./content-filter.ts";
import { useAuthorCatalogMembership } from "./author-catalog-membership.ts";
import { subscribeAuthorCatalogChanges } from "./author-catalog-events.ts";
import { subscribeSourceDetails } from "./source-detail-events.ts";
import {
  BrowsingMarkerNote,
  BrowsingNewBadge,
  useRecentBrowsingMarkers,
} from "./useBrowsingMarkers.tsx";

const noRecentSources: RecentViewState["sources"] = [];

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
  const [source, setSource] = useState<RecentSourceChoice>("Pica");
  const selectedSources: Source[] =
    source === "both" ? ["JM", "Pica"] : [source];
  const scopes = selectedSources.flatMap((selectedSource) => {
    const value = accountScope(
      accounts.find((account) => account.source === selectedSource),
    );
    return value ? [value] : [];
  });
  const scopeKey = JSON.stringify([source, scopes]);
  const scopeFor = (work: SourceWork): SourceScope | null =>
    scopes.find((value) => value.source === work.source) ?? null;
  const [observed, setObserved] = useState<{
    key: string;
    adapter: SourceAdapter;
    reader: RecentUpdatesView;
    state: RecentViewState;
  } | null>(null);
  const current =
    observed?.key === scopeKey && observed.adapter === adapter
      ? observed
      : null;
  const reader = current?.reader ?? null;
  const state = current?.state ?? null;
  const browsing = useRecentBrowsingMarkers(
    accounts,
    state?.sources ?? noRecentSources,
    active,
    !!adapter.recentHistory,
  );
  const data = state?.snapshot ?? null;
  const busy = state?.reading ?? false;
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
  const views = useRef(
    new Map<string, { adapter: SourceAdapter; reader: RecentUpdatesView }>(),
  );
  const currentView = useRef({ active, scopeKey });
  currentView.current = { active, scopeKey };
  useEffect(
    () =>
      subscribeSourceDetails((scope, work) => {
        if (
          !accounts.some(
            (account) =>
              account.source === scope.source &&
              account.state === "connected" &&
              account.sessionId === scope.sessionId,
          )
        )
          return;
        const entry = readers.current.get(JSON.stringify(scope));
        if (entry?.adapter === adapter) entry.reader.applyDetail(scope, work);
      }),
    [accounts, adapter],
  );
  useEffect(() => {
    setSelection([]);
    setSelectionMode(false);
    if (!scopes.length) {
      setObserved(null);
      return;
    }
    // Create inside the effect: StrictMode cleanup must not permanently dispose
    // the memoized reader reused by its second setup.
    let retained = views.current.get(scopeKey);
    if (retained && retained.adapter !== adapter) {
      retained.reader.dispose();
      retained = undefined;
    }
    const sourceReaders = scopes.map((scope) => {
      const key = JSON.stringify(scope);
      let entry = readers.current.get(key);
      if (entry && entry.adapter !== adapter) {
        entry.reader.dispose();
        entry = undefined;
      }
      const sourceReader =
        entry?.reader ?? new RecentUpdatesReader(adapter, scope);
      readers.current.set(key, { adapter, reader: sourceReader });
      return sourceReader;
    });
    const next =
      retained?.reader ?? new RecentUpdatesView(source, sourceReaders);
    views.current.set(scopeKey, { adapter, reader: next });
    let previous: SourceWork[] | null = null;
    const unsubscribe = next.subscribe((state) => {
      const items = state.displayItems;
      if (items !== previous) items.forEach(rememberContentWork);
      if (
        currentView.current.active &&
        currentView.current.scopeKey === scopeKey &&
        previous?.length &&
        items !== previous &&
        (source === "both" ||
          (items.length >= previous.length &&
            previous.every(
              (work, index) =>
                sourceWorkKey(work) === sourceWorkKey(items[index]),
            )))
      ) {
        // Capture when the response arrives, not when it was requested: the
        // reader may have moved elsewhere while waiting. The grid restores
        // this card after commit and yields to any new scroll input.
        let anchor = grid.current?.capture() ?? null;
        if (anchor) {
          const index = items.findIndex(
            (work) => sourceWorkKey(work) === anchor?.key,
          );
          if (index >= 0 && isContentHidden(items[index])) {
            const neighbor = items
              .slice(index + 1)
              .find((work) => !isContentHidden(work));
            anchor = neighbor
              ? (grid.current?.capture(sourceWorkKey(neighbor)) ?? null)
              : null;
          }
        }
        grid.current?.restore(anchor);
      }
      previous = items;
      setObserved({ key: scopeKey, adapter, reader: next, state });
    });
    return () => {
      unsubscribe();
    };
  }, [adapter, scopeKey]);
  useEffect(
    () => () => {
      for (const value of views.current.values()) value.reader.dispose();
      views.current.clear();
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
    if (!active || !reader || !scopes.length) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = subscribeAuthorCatalogChanges((change) => {
      if (
        !scopes.some(
          (scope) =>
            change.source === scope.source &&
            change.sessionId === scope.sessionId,
        )
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
  const displayItems = state?.displayItems ?? [];
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
          <p>浏览 JM 与哔咔的新近作品，翻页时保留已显示作品的位置。</p>
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
                setSource(event.target.value as RecentSourceChoice);
                clearSelection();
              }}
            >
              <option value="Pica">哔咔</option>
              <option value="JM">JM</option>
              <option value="both">JM＋哔咔</option>
            </select>
          </label>
          <button
            className="text-button"
            disabled={!scopes.length || busy}
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
      {!scopes.length ? (
        <div className="source-empty">
          <p>
            请先连接{source === "both" ? " JM 或哔咔" : sourceLabel(source)}
            账号。
          </p>
          <button onClick={onAccounts}>前往账号设置</button>
        </div>
      ) : (
        <>
          {source === "both" &&
            state?.sources.map((item) => (
              <RecentSourceProgress
                key={item.source}
                source={item.source}
                state={item.state}
                onRetry={() => void reader?.retry(item.source)}
                onAccounts={onAccounts}
              />
            ))}
          {source !== "both" && error && (
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
              <small>诊断代码：{state.observationErrorCode}</small>
            </p>
          )}
          {state?.historyError && (
            <p role="alert" className="source-notice">
              近期补漏历史暂未读取成功；当前来源结果保留，可刷新最近更新重试。
              {state.historyErrorCode && (
                <small>诊断代码：{state.historyErrorCode}</small>
              )}
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
            {source === "both" &&
              "JM 与哔咔按来源和作品编号分别计数，未跨站合并作品。"}
            {(data?.duplicates ?? 0) > 0
              ? `已合并 ${data!.duplicates} 条重复记录。`
              : ""}
            {retained.length > 0
              ? ` 另有已保存近期作品 ${retained.length} 部，未计入本次网站分页。`
              : ""}
          </p>
          <p className="source-muted" data-testid="content-preference-summary">
            按明确 BL／耽美、AI 标签
            {source !== "Pica" ? "及 JM 女性向标签" : ""}隐藏 {hiddenCount} 部。
            仅使用已有标签；标签未知的作品正常显示，不额外读取详情。
          </p>
          <BrowsingMarkerNote
            notes={browsing.notes}
            surface="recent"
            onRetry={browsing.retry}
          />
          {state?.sources.map((item) => (
            <SourceIssues
              key={item.source}
              source={item.source}
              issues={item.state?.snapshot?.issues}
              pagesComplete={item.state?.phase === "complete"}
              testId={
                source === "both"
                  ? "recent-issues-" + item.source
                  : "recent-issues"
              }
            />
          ))}
          <details className="page-scope-details">
            <summary>浏览范围与排序说明 · 仅筛选已读取作品</summary>
            <p className="source-muted" data-testid="recent-order-note">
              {source === "both"
                ? "双站首次读取及手动刷新时，按已读取作品的网站更新时间从新到旧排列，未知日期放在末尾。后续翻页和补充历史只追加新作品、更新已有信息，保留当前浏览位置；较晚读到的新日期不会插到前面。两站分页各自保留进度，一站失败不清除另一站结果。"
                : "翻页与补充历史不会提前或重排已显示作品；点击“刷新最近更新”后，按已读取的网站顺序重新排列。"}
              日期以网站提供为准，不保证每次章节更新都会排到前面。筛选仅覆盖已读取范围。
            </p>
            {data && (
              <p className="source-muted">
                最近读取：{new Date(data.updatedAt).toLocaleString()} ·{" "}
                {inventoryScopeNote}
              </p>
            )}
            {state?.sources.map(
              ({ source: coverageSource, state: sourceState }) =>
                sourceState?.retainedCoverage && (
                  <p
                    key={coverageSource}
                    className="source-muted"
                    data-testid={
                      source === "both"
                        ? "recent-retained-coverage-" + coverageSource
                        : "recent-retained-coverage"
                    }
                  >
                    {source === "both" && sourceLabel(coverageSource) + " · "}
                    已保存近期补漏范围：{
                      sourceState.retainedCoverage.pagesRead
                    }{" "}
                    页
                    {sourceState.retainedCoverage.checkedAt
                      ? ` · ${new Date(sourceState.retainedCoverage.checkedAt).toLocaleString()}`
                      : ""}
                    {sourceState.retainedCoverage.errorCode
                      ? " · 范围未完成，已有记录保留。"
                      : sourceState.retainedCoverage.reachedEnd
                        ? " · 本次入口分页已读完。"
                        : sourceState.retainedCoverage.joinedPrevious
                          ? " · 已衔接上次已确认范围。"
                          : sourceState.retainedCoverage.initialWindow
                            ? " · 已建立初始窗口，更早历史未覆盖。"
                            : " · 已保存部分范围，未证明完整。"}
                    不代表全站历史作品均已覆盖。
                  </p>
                ),
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
            renderItem={(work) => {
              const scope = scopeFor(work);
              if (!scope) return null;
              return (
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
                      <SourceCover
                        adapter={adapter}
                        scope={scope}
                        work={work}
                      />
                      <BrowsingNewBadge work={work} marked={browsing.keys} />
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
                    {source === "both" && `${sourceLabel(work.source)} · `}
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
              );
            }}
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
              {source === "both"
                ? combinedProgress(state)
                : busy
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
            {source === "both" ? (
              state?.canLoadNext && (
                <button
                  className="text-button"
                  onClick={() => void reader?.loadNext()}
                >
                  读取下一页
                </button>
              )
            ) : state?.phase === "error" ? (
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

function combinedProgress(state: RecentViewState | null): string {
  if (!state) return "尚未读取最近更新。";
  if (state.phase === "complete")
    return state.snapshot?.issues?.length
      ? "双站本次返回的分页已读完，仍有记录待核对。"
      : "双站本次返回的分页已读完。";
  const failed = state.sources.some(
    ({ state }) =>
      !state || state.phase === "error" || state.phase === "limited",
  );
  if (state.reading)
    return failed
      ? "部分来源正在读取，另有来源未完成；已读内容保留。"
      : "正在读取最近更新；两站各自保留进度。";
  if (state.canLoadNext)
    return failed
      ? "部分来源未完成，可继续浏览另一来源，或重试失败来源。"
      : "向下滚动或点击读取下一页继续浏览；仅覆盖已读取范围。";
  return "双站范围尚未完成，请查看各来源状态；已读内容保留。";
}

function RecentSourceProgress({
  source,
  state,
  onRetry,
  onAccounts,
}: {
  source: Source;
  state: RecentUpdatesState | null;
  onRetry(): void;
  onAccounts(): void;
}) {
  return (
    <p
      className="source-muted"
      data-testid={"recent-source-progress-" + source}
    >
      {sourceLabel(source)} ·{" "}
      {state
        ? `已读取 ${state.snapshot?.page ?? 0} 页 / ${state.snapshot?.items.length ?? 0} 部 · `
        : ""}
      {!state ? (
        <>
          未连接，当前合并范围不完整。{" "}
          <button className="text-button" onClick={onAccounts}>
            前往账号设置
          </button>
        </>
      ) : state.phase === "error" ? (
        <>
          {sourceErrorMessage(state.error)} 已读内容保留。{" "}
          <button className="text-button" onClick={onRetry}>
            重试{sourceLabel(source)}
          </button>
        </>
      ) : state.phase === "reading" ? (
        "正在读取…"
      ) : state.phase === "complete" ? (
        state.snapshot?.issues?.length ? (
          "分页已读完，仍有记录待核对。"
        ) : (
          "本次分页已读完。"
        )
      ) : state.phase === "limited" ? (
        "达到浏览保存上限，当前范围未读完。"
      ) : state.phase === "ready" ? (
        "当前已读取部分范围。"
      ) : (
        "等待读取。"
      )}
    </p>
  );
}
