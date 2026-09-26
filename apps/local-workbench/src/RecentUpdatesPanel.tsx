import { useEffect, useMemo, useRef, useState } from "react";
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
import { SourceIssues } from "./SourceIssues.tsx";
import { formatWorkDate } from "./work-dates.ts";
import { RecentUpdatesReader } from "./recent-updates.ts";
import type { RecentUpdatesState } from "./recent-updates.ts";

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
  onDownloadMany(works: SourceWork[]): void;
  onAccounts(): void;
}) {
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
  const [filter, setFilter] = useState<InventoryFilter>("all");
  const [query, setQuery] = useState("");
  const [selectionMode, setSelectionMode] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setSelection([]);
    setSelectionMode(false);
    if (!scope) {
      setObserved(null);
      return;
    }
    // Create inside the effect: StrictMode cleanup must not permanently dispose
    // the memoized reader reused by its second setup.
    const next = new RecentUpdatesReader(adapter, scope);
    const unsubscribe = next.subscribe((state) =>
      setObserved({ key: scopeKey, adapter, reader: next, state }),
    );
    return () => {
      unsubscribe();
      next.dispose();
    };
  }, [adapter, scopeKey]);
  useEffect(() => {
    if (active) void reader?.start();
    else setSelection([]);
  }, [reader, active]);

  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const searched = (data?.items ?? []).filter((work) =>
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

  useEffect(() => {
    const target = sentinel.current;
    const main = target?.closest("main");
    if (
      !active ||
      !reader ||
      !target ||
      !main ||
      state?.phase !== "ready" ||
      data?.hasMore !== true ||
      terms ||
      filter !== "all"
    )
      return;
    let lastScroll = main.scrollTop;
    let intentUntil = 0;
    let frame = 0;
    const intent = () => {
      intentUntil = Date.now() + 1500;
    };
    const keyIntent = (event: KeyboardEvent) => {
      if (
        ["ArrowDown", "PageDown", "End", " "].includes(event.key) &&
        !(event.target instanceof HTMLInputElement) &&
        !(event.target instanceof HTMLSelectElement)
      )
        intent();
    };
    const scroll = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const previous = lastScroll;
        lastScroll = main.scrollTop;
        if (
          lastScroll <= previous ||
          Date.now() > intentUntil ||
          target.getBoundingClientRect().top >
            main.getBoundingClientRect().bottom + 200
        )
          return;
        // A completed request never supplies another scroll credit. Even a short
        // or filtered page therefore cannot initiate an unbounded site scan.
        intentUntil = 0;
        void reader.loadNext();
      });
    };
    main.addEventListener("wheel", intent, { passive: true });
    main.addEventListener("touchstart", intent, { passive: true });
    main.addEventListener("pointerdown", intent, { passive: true });
    main.addEventListener("keydown", keyIntent);
    main.addEventListener("scroll", scroll, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      main.removeEventListener("wheel", intent);
      main.removeEventListener("touchstart", intent);
      main.removeEventListener("pointerdown", intent);
      main.removeEventListener("keydown", keyIntent);
      main.removeEventListener("scroll", scroll);
    };
  }, [active, reader, state?.phase, data?.page, data?.hasMore, terms, filter]);
  const clearSelection = () => {
    setSelection([]);
    setSelectionMode(false);
  };
  const error = state?.error ? sourceErrorMessage(state.error) : "";
  return (
    <section
      className={
        "source-workbench" + (selected.length ? " has-source-selection" : "")
      }
      data-testid="recent-panel"
      hidden={!active}
    >
      <div className="page-heading source-heading">
        <div>
          <h1>发现</h1>
          <p>从来源推荐中浏览作品</p>
        </div>
      </div>
      {navigation}
      <h2>{sourceLabel(source)} · 最近更新</h2>
      <div className="source-toolbar">
        <div className="source-toolbar-leading">
          <label>
            来源{" "}
            <select
              aria-label="最近更新来源"
              value={source}
              onChange={(event) => {
                setSource(event.target.value as Source);
                clearSelection();
                setQuery("");
                setFilter("all");
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
        <div className="source-search source-search-host">
          <input
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
      <p className="source-muted" data-testid="recent-order-note">
        按来源最新顺序浏览，日期以网站提供为准；不保证每次章节更新都会排到前面。筛选仅覆盖已读取范围。
      </p>
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
          <p className="source-muted" data-testid="recent-counts">
            已读取 {data?.items.length ?? 0} 部
            {data?.total !== null && data?.total !== undefined
              ? ` / 来源报告 ${data.total} 条`
              : ""}{" "}
            · 已入库 {counts.owned} 部 · 未入库 {counts.missing} 部 · 当前显示{" "}
            {visible.length} 部。统计仅覆盖已读取的 {data?.page ?? 0} 页。
            {(data?.duplicates ?? 0) > 0
              ? `已合并 ${data!.duplicates} 条重复记录。`
              : ""}
          </p>
          <SourceIssues
            source={source}
            issues={data?.issues}
            pagesComplete={state?.phase === "complete"}
            testId="recent-issues"
          />
          {data && (
            <p className="source-muted">
              最近读取：{new Date(data.updatedAt).toLocaleString()} ·{" "}
              {inventoryScopeNote}
            </p>
          )}
          {visible.length > 0 && (
            <div className="source-toolbar">
              <div className="source-toolbar-leading">
                <button
                  className="text-button"
                  aria-pressed={selectionMode}
                  onClick={() => {
                    setSelectionMode(!selectionMode);
                    setSelection([]);
                  }}
                >
                  {selectionMode ? "退出多选" : "多选"}
                </button>
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
                    全选已读取筛选范围
                  </button>
                )}
              </div>
            </div>
          )}
          <VirtualSourceGrid<SourceWork>
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
                  <button
                    className="source-cover-button source-language-cover"
                    aria-label={"查看《" + work.title + "》详情"}
                    onClick={() => onOpen(work)}
                  >
                    <SourceCover adapter={adapter} scope={scope} work={work} />
                    <SourceLanguageBadge
                      tags={work.tags}
                      work={work}
                      scope={scope}
                    />
                  </button>
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
                <p>{work.authors.join("、") || "作者资料未取得"}</p>
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
                <button
                  className="text-button"
                  disabled={inventory(work).kind === "owned" || !library.rootId}
                  onClick={() => onDownload(work)}
                >
                  下载到漫画库
                </button>
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
                        ? "仅筛选已读取范围；点击读取下一页可继续，清空筛选后恢复滚动读取。"
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
          {selected.length > 0 && (
            <div
              className="source-selection-bar"
              data-testid="recent-selection-bar"
            >
              <strong>已选 {selected.length} 部</strong>
              <span>仅限已读取的当前筛选范围</span>
              <button
                className="button primary"
                disabled={
                  !library.rootId || selected.length > downloadSelectionLimit
                }
                onClick={() => onDownloadMany(selected)}
              >
                准备下载所选作品
              </button>
              {selected.length > downloadSelectionLimit && (
                <span>一次最多选择 500 本，请缩小范围。</span>
              )}
              <button className="text-button" onClick={clearSelection}>
                取消选择
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}
