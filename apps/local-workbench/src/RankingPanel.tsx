import { useEffect, useMemo, useRef, useState } from "react";
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
import {
  createInventoryMatcher,
  inventoryFilterLabels,
  inventoryFilterMatches,
  inventoryLabel,
} from "./inventory-model.ts";
import type { InventoryFilter } from "./inventory-model.ts";
import { sourceErrorMessage } from "./source-runtime.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";

export function RankingPanel({
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
  onDownloadMany(works: SourceWork[]): void;
  onAccounts(): void;
}) {
  const scope = accountScope(
    accounts.find((account) => account.source === source),
  );
  const scopeKey = JSON.stringify(scope);
  const epoch = useRef(0),
    currentScope = useRef(scopeKey);
  currentScope.current = scopeKey;
  const [options, setOptions] = useState<RankOptions | null>(null);
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
  const data = result?.key === key ? result : null;
  useEffect(() => {
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
        setOptions(next);
        setCategory(next.categories[0]?.id ?? null);
        setPeriod(next.periods[0]?.id ?? "");
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
  }, [adapter, scopeKey, optionsRetry]);
  useEffect(() => {
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
        if (request === epoch.current && currentScope.current === scopeKey)
          setResult({ key, page, time: Date.now() });
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
  }, [adapter, key, options, reload]);
  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const searched = (data?.page.items ?? []).filter((work) =>
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
  const complete = !error && !busy && data?.page.hasMore === false;
  return (
    <section className="source-workbench" data-testid="ranking-panel">
      <div className="page-heading">
        <h1>发现</h1>
        <p>从来源推荐中浏览作品</p>
      </div>
      {navigation}
      <h2>{source === "JM" ? "JM · 每周必看" : "哔咔 · 排行榜"}</h2>
      {!scope ? (
        <div className="source-empty">
          <p>请先连接{sourceLabel(source)}账号。</p>
          <button onClick={onAccounts}>前往账号设置</button>
        </div>
      ) : (
        <>
          <div className="source-toolbar">
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
              {error} 已读取榜单保留。
            </p>
          )}
          {busy && <p role="status">正在读取来源榜单…</p>}
          <div className="result-filters" aria-label="榜单入库筛选">
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
          <p className="source-muted" data-testid="ranking-counts">
            已读取 {data?.page.items.length ?? 0} 条
            {data?.page.total !== null && data?.page.total !== undefined
              ? ` / 来源报告 ${data.page.total} 条`
              : ""}{" "}
            · 已入库 {counts.owned} 条 · 未入库 {counts.missing} 条 · 当前显示{" "}
            {visible.length} 条。
            {data
              ? complete
                ? "本次榜单已读完。"
                : "本次榜单尚未完整确认，请刷新重试。"
              : ""}
          </p>
          {data && (
            <p className="source-muted">
              来源排序 · {new Date(data.time).toLocaleString()} ·
              入库状态按本软件对应来源的下载记录核实。
            </p>
          )}
          {visible.length > 0 && (
            <div className="source-toolbar">
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
                选择当前筛选范围
              </button>
              {selection.length > 0 && (
                <>
                  <span>已选 {selected.length} 部</span>
                  <button
                    className="button primary"
                    disabled={!library.rootId || !selected.length}
                    onClick={() => onDownloadMany(selected)}
                  >
                    准备下载所选作品
                  </button>
                  <button
                    className="text-button"
                    onClick={() => setSelection([])}
                  >
                    取消选择
                  </button>
                </>
              )}
            </div>
          )}
          <VirtualSourceGrid<SourceWork>
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
                  <button
                    className="source-cover-button"
                    aria-label={"查看《" + work.title + "》详情"}
                    onClick={() => onOpen(work)}
                  >
                    <SourceCover adapter={adapter} scope={scope} work={work} />
                  </button>
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
                </div>
                <h3>
                  <button onClick={() => onOpen(work)}>{work.title}</button>
                </h3>
                <p>{work.authors.join("、") || "作者资料未取得"}</p>
                <p className="source-card-state">
                  {inventoryLabel(inventory(work))}
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
              {data ? "当前筛选没有作品。" : "来源尚未返回可用榜单。"}
            </p>
          )}
        </>
      )}
    </section>
  );
}
