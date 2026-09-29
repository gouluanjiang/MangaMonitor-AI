import { CoverInteraction } from "./reader-access.tsx";
import { AuthorLinks } from "./AuthorLinks.tsx";
import { FloatingSelection } from "./FloatingSelection.tsx";
import { useBrowseSession } from "./useBrowseSession.ts";
import { DownloadWorkButton } from "./DownloadWorkButton.tsx";
import { isContentHidden, rememberContentWork } from "./content-filter.ts";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import { useEffect, useMemo, useRef, useState } from "react";
import { useReaderAccess, sourceReaderRequest } from "./reader-access.tsx";
import type { ReactNode } from "react";
import type {
  AccountSummary,
  RankOptions,
  Source,
  SourceAdapter,
  SourcePage,
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

export function RankingPanel({
  active = true,
  source,
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
  active?: boolean;
  source: Source;
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
  const root = useRef<HTMLElement>(null);
  const grid = useRef<SourceGridHandle>(null);
  const [selectionMode, setSelectionMode] = useState(false);
  const scope = accountScope(
    accounts.find((account) => account.source === source),
  );
  const scopeKey = JSON.stringify(scope);
  const epoch = useRef(0),
    currentScope = useRef(scopeKey);
  currentScope.current = scopeKey;
  const [options, setOptions] = useState<RankOptions | null>(null);
  const optionsContext = useRef("");
  const resultContext = useRef("");
  const [category, setCategory] = useState<string | null>(null),
    [period, setPeriod] = useState("");
  const [result, setResult] = useState<{
    key: string;
    page: SourcePage;
    time: number;
  } | null>(null);
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [reload, setReload] = useState(0),
    [optionsRetry, setOptionsRetry] = useState(0);
  const [filter, setFilter] = useState<InventoryFilter>("all"),
    [query, setQuery] = useState("");
  const [selection, setSelection] = useState<string[]>([]);
  const key = scopeKey + JSON.stringify([category, period]);
  const optionsKey = scopeKey + ":" + optionsRetry;
  const data = result?.key === key ? result : null;
  useEffect(() => {
    if (!active) {
      setBusy(false);
      setSelection([]);
      return;
    }
    if (options && optionsContext.current === optionsKey) return;
    const request = ++epoch.current;
    setOptions(null);
    setResult(null);
    setPeriod("");
    setCategory(null);
    setError("");
    setSelection([]);
    if (!scope) {
      setBusy(false);
      return;
    }
    setBusy(true);
    void adapter
      .rankingOptions(scope)
      .then((next) => {
        if (request !== epoch.current || currentScope.current !== scopeKey)
          return;
        optionsContext.current = optionsKey;
        setOptions(next);
        setCategory(next.categories[0]?.id ?? null);
        setPeriod(
          (source === "JM"
            ? next.periods.find((option) => option.id === "manga")?.id
            : undefined) ??
            next.periods[0]?.id ??
            "",
        );
      })
      .catch((cause) => {
        if (request === epoch.current) setError(sourceErrorMessage(cause));
      })
      .finally(() => {
        if (request === epoch.current) setBusy(false);
      });
    return () => {
      epoch.current++;
    };
  }, [adapter, scopeKey, optionsRetry, active]);
  useEffect(() => {
    if (!active) {
      setBusy(false);
      return;
    }
    if (resultContext.current === key + ":" + reload && data) return;
    if (optionsContext.current !== optionsKey) return;
    if (!scope || !options || !period || (source === "JM" && !category)) return;
    const request = ++epoch.current;
    setBusy(true);
    setError("");
    setSelection([]);
    void adapter
      .query(scope, {
        kind: "ranking",
        query: period,
        folderId: category,
        page: 1,
      })
      .then((page) => {
        if (request === epoch.current && currentScope.current === scopeKey) {
          resultContext.current = key + ":" + reload;
          setResult({ key, page, time: Date.now() });
        }
      })
      .catch((cause) => {
        if (request === epoch.current) setError(sourceErrorMessage(cause));
      })
      .finally(() => {
        if (request === epoch.current) setBusy(false);
      });
    return () => {
      epoch.current++;
    };
  }, [adapter, key, options, reload, active]);
  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const searched = (data?.page.items ?? [])
    .filter((work) => !isContentHidden(work))
    .filter((work) =>
      [work.title, ...work.authors]
        .join(" ")
        .normalize("NFKC")
        .toLocaleLowerCase()
        .includes(terms),
    );
  useEffect(() => {
    data?.page.items.forEach(rememberContentWork);
  }, [data?.page.items]);
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
  const complete = !error && !busy && data?.page.hasMore === false;
  useBrowseSession({
    scope: JSON.stringify(["ranking", key, query, filter]),
    active,
    root,
    grid,
    itemKeys: visible.map(sourceWorkKey),
  });
  return (
    <section
      ref={root}
      className="source-workbench"
      data-testid="ranking-panel"
    >
      <div className="page-heading source-heading">
        <div>
          <h1>{source === "JM" ? "JM · 每周必看" : "哔咔 · 排行榜"}</h1>
          <p>浏览网站推荐与榜单，筛选漫画库中尚未入库的作品。</p>
        </div>
      </div>
      {navigation}
      {!scope ? (
        <div className="source-empty">
          <p>请先连接{sourceLabel(source)}账号。</p>
          <button onClick={onAccounts}>前往账号设置</button>
        </div>
      ) : (
        <>
          <div className="source-toolbar source-page-tools">
            <div className="source-toolbar-leading">
              {source === "JM" && (
                <label>
                  期数{" "}
                  <select
                    aria-label="每周必看期数"
                    value={category ?? ""}
                    onChange={(event) => {
                      setCategory(event.target.value);
                      setSelection([]);
                    }}
                  >
                    {options?.categories.map((option) => (
                      <option key={option.id} value={option.id}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <label>
                {source === "JM" ? "类型" : "榜单"}{" "}
                <select
                  aria-label="排行类型"
                  value={period}
                  onChange={(event) => {
                    setPeriod(event.target.value);
                    setSelection([]);
                  }}
                >
                  {options?.periods.map((option) => (
                    <option key={option.id} value={option.id}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="text-button"
                disabled={busy}
                onClick={() => {
                  if (options) setReload((value) => value + 1);
                  else setOptionsRetry((value) => value + 1);
                }}
              >
                刷新榜单
              </button>
            </div>
            <input
              type="search"
              aria-label="筛选当前榜单"
              placeholder="筛选作品或作者…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setSelection([]);
              }}
            />
          </div>
          {error && (
            <p role="alert" className="source-notice">
              {error} {data ? "已读取榜单保留。" : "请点击刷新榜单重试。"}
            </p>
          )}
          {busy && <p role="status">正在读取来源榜单…</p>}
          <div className="source-tabs" aria-label="榜单入库筛选">
            {(Object.keys(inventoryFilterLabels) as InventoryFilter[]).map(
              (value) => (
                <button
                  key={value}
                  aria-pressed={filter === value}
                  onClick={() => {
                    setFilter(value);
                    setSelection([]);
                  }}
                >
                  {inventoryFilterLabels[value]} {counts[value]}
                </button>
              ),
            )}
          </div>
          <p className="page-summary" data-testid="ranking-counts">
            已读取 {data?.page.items.length ?? 0} 条
            {data?.page.total !== null && data?.page.total !== undefined
              ? ` / 来源报告 ${data.page.total} 条`
              : ""}{" "}
            · 已入库 {counts.owned} 条 · 未入库 {counts.missing} 条 · 当前显示{" "}
            {visible.length} 条。
            {data
              ? complete
                ? data.page.issues?.length
                  ? "本次榜单分页已读完，仍有来源记录待核对。"
                  : "本次榜单已读完。"
                : "本次榜单尚未完整确认，请刷新重试。"
              : ""}
          </p>
          <SourceIssues
            source={source}
            issues={data?.page.issues}
            pagesComplete={complete}
            testId="ranking-issues"
          />
          {data && (
            <details className="page-scope-details">
              <summary>榜单范围与入库说明 · 保留网站排序</summary>
              <p className="source-muted">
                来源排序 · {new Date(data.time).toLocaleString()} ·
                {inventoryScopeNote}
              </p>
            </details>
          )}
          <FloatingSelection
            active={selectionMode}
            selectedCount={selected.length}
            onEnter={() => setSelectionMode(true)}
            onCancel={() => {
              setSelection([]);
              setSelectionMode(false);
            }}
            disabled={
              !library.rootId || selected.length > downloadSelectionLimit
            }
            onDownload={() => {
              void Promise.resolve(onDownloadMany(selected)).then((keys) => {
                if (keys)
                  setSelection((previous) =>
                    previous.filter((key) => !keys.includes(key)),
                  );
              });
            }}
          >
            <button
              disabled={!complete}
              onClick={() =>
                setSelection(
                  visible
                    .filter((work) => inventory(work).kind !== "owned")
                    .map(sourceWorkKey),
                )
              }
            >
              全选当前范围
            </button>
          </FloatingSelection>
          <VirtualSourceGrid<SourceWork>
            ref={grid}
            items={visible}
            density={density}
            itemKey={sourceWorkKey}
            testId="ranking-grid"
            renderItem={(work) => (
              <article
                className="source-card"
                data-testid={"rank-work-" + sourceWorkKey(work)}
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
                        setSelection((old) =>
                          old.includes(sourceWorkKey(work))
                            ? old.filter((key) => key !== sourceWorkKey(work))
                            : [...old, sourceWorkKey(work)],
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
                ? complete &&
                  data.page.items.length === 0 &&
                  !data.page.issues?.length
                  ? source === "JM"
                    ? "本期该类型暂无作品，可以切换期数或类型。"
                    : "来源当前榜单暂无作品。"
                  : "当前筛选没有作品。"
                : "来源尚未返回可用榜单。"}
            </p>
          )}
        </>
      )}
    </section>
  );
}
