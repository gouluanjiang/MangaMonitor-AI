import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AccountSummary,
  SourceAdapter,
  SourceScope,
  SourceWork,
  Source,
} from "./source-types.ts";
import { accountScope, sourceWorkKey, sourceLabel } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { DownloadInventorySnapshot } from "./download-types.ts";
import { downloadSelectionLimit } from "./download-types.ts";
import type { WorkReference } from "./booklists.ts";
import type {
  CompletionAdapter,
  DiscoverySnapshot,
} from "./completion-types.ts";
import {
  completionError,
  createCompletionAdapter,
} from "./completion-runtime.ts";
import {
  createInventoryMatcher,
  inventoryFilterLabels,
  inventoryFilterMatches,
  inventoryLabel,
  inventoryScopeNote,
} from "./inventory-model.ts";
import type { InventoryFilter } from "./inventory-model.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import { createAuthorSearchAdapter } from "./author-search.ts";
import { jmSearchScopeNote } from "./source-search.ts";
import "./completion.css";

const nativeAdapter = createCompletionAdapter();
interface Props {
  mode?: "updates" | "search";
  accounts: AccountSummary[];
  sourceAdapter: SourceAdapter;
  adapter?: CompletionAdapter;
  library: LibrarySnapshot;
  inventorySnapshot?: DownloadInventorySnapshot;
  inventoryReady?: boolean;
  onRefreshInventory?(): Promise<void>;
  density: 5 | 7 | 9;
  onOpenWork(reference: WorkReference): void;
  onDownload(work: SourceWork): void;
  onDownloadMany(works: SourceWork[]): void;
  downloadBusy?: boolean;
  onOpenLibrary(): void;
  onOpenAccounts(): void;
}

export function CompletionPanel({
  mode = "updates",
  accounts,
  sourceAdapter,
  adapter: providedAdapter,
  library,
  inventorySnapshot,
  inventoryReady = false,
  onRefreshInventory,
  density,
  onOpenWork,
  onDownload,
  onDownloadMany,
  downloadBusy = false,
  onOpenLibrary,
  onOpenAccounts,
}: Props) {
  const searchAdapter = useMemo(
    () => createAuthorSearchAdapter(sourceAdapter),
    [sourceAdapter],
  );
  const adapter =
    providedAdapter ?? (mode === "search" ? searchAdapter : nativeAdapter);
  const [searchAuthor, setSearchAuthor] = useState("");
  const scopes = accounts
    .map(accountScope)
    .filter((s): s is SourceScope => s !== null);
  const scopeKey = JSON.stringify(scopes);
  const current = useRef({ key: scopeKey, scopes });
  current.current = { key: scopeKey, scopes };
  const epoch = useRef(0),
    operation = useRef(false),
    mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const [result, setResult] = useState<{
    key: string;
    value: DiscoverySnapshot;
  } | null>(null);
  const view = result?.key === scopeKey ? result.value : null;
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [author, setAuthor] = useState(""),
    [query, setQuery] = useState("");
  const [source, setSource] = useState<Source | "all">("all");
  const [filter, setFilter] = useState<InventoryFilter>("missing");
  const [selectionMode, setSelectionMode] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  useEffect(() => {
    setSelection([]);
  }, [scopeKey, author, source, filter, query, mode]);
  const connected = scopes.length === 2;
  const running = view?.run?.phase === "checking";
  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const load = useCallback(async () => {
    const captured = current.current,
      request = ++epoch.current;
    if (captured.scopes.length !== 2) return;
    try {
      const next = await adapter.read(captured.scopes);
      if (current.current.key === captured.key && request === epoch.current) {
        setResult({ key: captured.key, value: next });
        setError("");
      }
    } catch (cause) {
      if (current.current.key === captured.key && request === epoch.current)
        setError(completionError(cause));
    }
  }, [adapter]);
  useEffect(() => {
    setResult(null);
    setError("");
    setAuthor("");
    setQuery("");
    void load();
    return () => {
      epoch.current++;
    };
  }, [load, scopeKey]);
  useEffect(() => {
    if (!running || busy || error) return;
    const timer = setTimeout(() => void load(), 1500);
    return () => clearTimeout(timer);
  }, [load, running, busy, error, view]);
  async function perform(action: () => Promise<DiscoverySnapshot | void>) {
    if (operation.current) return;
    operation.current = true;
    setSelection([]);
    epoch.current++;
    const key = current.current.key;
    setBusy(true);
    setError("");
    try {
      const next = await action();
      if (mounted.current && current.current.key === key) {
        if (next) setResult({ key, value: next });
        else await load();
      }
    } catch (cause) {
      if (mounted.current && current.current.key === key)
        setError(completionError(cause));
    } finally {
      operation.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  function startCheck() {
    const captured = current.current;
    const selected =
      mode === "search" ? [searchAuthor.trim()] : author ? [author] : [];
    void perform(async () => {
      await onRefreshInventory?.();
      if (!mounted.current || current.current.key !== captured.key) return;
      return adapter.start(captured.scopes, selected);
    });
  }
  const authors = [
    ...new Set(view?.authors.map((range) => range.author) ?? []),
  ];
  const ranges = (view?.authors ?? []).filter(
    (range) =>
      (!author || range.author === author) &&
      (source === "all" || range.source === source),
  );
  const complete =
    !running &&
    !error &&
    ranges.length > 0 &&
    ranges.every((range) => range.state === "complete");
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const scopedRecords = (view?.records ?? []).filter(
    (record) =>
      (!author || record.matchedAuthors.includes(author)) &&
      (source === "all" || record.work.source === source),
  );
  const records = scopedRecords.filter(
    (record) =>
      !terms ||
      [record.work.title, ...record.work.authors]
        .join(" ")
        .normalize("NFKC")
        .toLocaleLowerCase()
        .includes(terms),
  );
  const counts = Object.fromEntries(
    (Object.keys(inventoryFilterLabels) as InventoryFilter[]).map((kind) => [
      kind,
      records.filter((record) =>
        inventoryFilterMatches(inventory(record.work), kind),
      ).length,
    ]),
  ) as Record<InventoryFilter, number>;
  const visible = records.filter((record) =>
    inventoryFilterMatches(inventory(record.work), filter),
  );
  const selectionKeys = new Set(selection);
  const selectable = visible.filter(
    (record) => inventory(record.work).kind !== "owned",
  );
  const selected = selectable
    .filter((record) => selectionKeys.has(sourceWorkKey(record.work)))
    .map((record) => record.work);
  const lastCheck = Math.max(
    0,
    ...ranges.map((range) => range.lastCompleteAt ?? range.lastAttemptAt ?? 0),
  );
  return (
    <section
      className={`completion-panel${selected.length ? " has-completion-selection" : ""}`}
      data-testid="completion-panel"
    >
      <header className="page-heading">
        <h1>{mode === "search" ? "作者搜索" : "作者更新"}</h1>
        <p>
          {mode === "search"
            ? "输入作者名，读取 JM 与哔咔的完整查询结果，无需先关注。"
            : "检查关注作者在 JM 和哔咔的作品，挑选需要下载的漫画。"}
        </p>
      </header>
      {!connected ? (
        <div className="source-empty">
          <p>请连接 JM 和哔咔账号后检查。</p>
          <button onClick={onOpenAccounts}>连接账号</button>
        </div>
      ) : (
        <>
          <div className="completion-controls">
            {mode === "search" ? (
              <label>
                作者名{" "}
                <input
                  aria-label="搜索作者名"
                  value={searchAuthor}
                  onChange={(event) => setSearchAuthor(event.target.value)}
                  disabled={busy || running}
                />
              </label>
            ) : (
              <label>
                检查作者{" "}
                <select
                  aria-label="检查作者"
                  value={author}
                  onChange={(event) => setAuthor(event.target.value)}
                  disabled={busy || running}
                >
                  <option value="">全部关注作者</option>
                  {authors.map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
              </label>
            )}
            <button
              className="primary-button"
              disabled={
                busy ||
                running ||
                (mode === "search" ? !searchAuthor.trim() : !authors.length)
              }
              data-testid="completion-start"
              onClick={startCheck}
            >
              {mode === "search" ? "搜索两站作品" : "检查作者更新"}
            </button>
            {running && (
              <button
                disabled={busy}
                onClick={() =>
                  void perform(async () => {
                    if (view?.run) await adapter.cancel(view.run.id);
                  })
                }
              >
                停止本次检查
              </button>
            )}
            <button
              disabled={busy}
              onClick={() =>
                void perform(async () => {
                  await onRefreshInventory?.();
                })
              }
            >
              刷新结果与入库状态
            </button>
          </div>
          {mode === "updates" && !authors.length && (
            <p className="source-empty">
              尚未关注作者。可以先搜索作者，再添加关注。
            </p>
          )}
          {!library.rootId && (
            <p className="source-notice">
              尚未选择漫画库。
              <button onClick={onOpenLibrary}>设置漫画库</button>
            </p>
          )}
          {running && (
            <p role="status" data-testid="completion-progress">
              正在检查 {view?.run?.currentAuthor ?? "关注作者"} ·{" "}
              {view?.run?.currentSource
                ? sourceLabel(view.run.currentSource)
                : "准备中"}{" "}
              · 第 {view?.run?.currentPage ?? 0} 页 · 已检查{" "}
              {view?.run?.completedScopes ?? 0} / {view?.run?.totalScopes ?? 0}{" "}
              个来源范围
            </p>
          )}
          {error && (
            <p role="alert" className="source-notice">
              {error}
            </p>
          )}
          <div className="completion-controls">
            <label>
              来源{" "}
              <select
                aria-label="更新来源"
                value={source}
                onChange={(event) =>
                  setSource(event.target.value as Source | "all")
                }
              >
                <option value="all">JM 与哔咔</option>
                <option value="JM">JM</option>
                <option value="Pica">哔咔</option>
              </select>
            </label>
            <input
              aria-label="筛选作者更新"
              placeholder="筛选已发现作品或作者…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <div className="source-tabs" aria-label="作者更新入库筛选">
            {(Object.keys(inventoryFilterLabels) as InventoryFilter[]).map(
              (value) => (
                <button
                  key={value}
                  aria-pressed={filter === value}
                  onClick={() => setFilter(value)}
                >
                  {inventoryFilterLabels[value]} {counts[value]}
                </button>
              ),
            )}
          </div>
          <p data-testid="completion-counts">
            {complete ? "当前检查范围已读完" : "检查范围尚未读完"} · 已记录{" "}
            {records.length} 条 · 已入库 {counts.owned} 条 · 未入库{" "}
            {counts.missing} 条 · 当前显示 {visible.length} 条
            {counts.unknown > 0 ? ` · 状态待核实 ${counts.unknown} 条` : ""}
          </p>
          <p className="source-muted">
            {inventoryScopeNote} JM 与哔咔分别计数，未选择下载的记录会继续保留。
            {lastCheck > 0
              ? ` 上次检查：${new Date(lastCheck).toLocaleString()}`
              : " 尚未完成检查。"}
          </p>
          {source !== "Pica" && (
            <p className="source-muted">{jmSearchScopeNote}</p>
          )}
          {complete &&
            scopedRecords.length > 0 &&
            scopedRecords.every(
              (record) => inventory(record.work).kind === "owned",
            ) && (
              <p role="status" data-testid="completion-all-owned">
                本次查询结果已全部入库。范围：
                {source === "all" ? "JM 与哔咔" : sourceLabel(source)}，
                {author || authors.join("、")}，
                {new Date(lastCheck).toLocaleString()}。
              </p>
            )}
          {!complete &&
            ranges.some((range) =>
              ["partial", "error", "cancelled"].includes(range.state),
            ) && (
              <details className="source-notice">
                <summary>查看未完成范围</summary>
                {ranges
                  .filter((range) => range.state !== "complete")
                  .map((range) => (
                    <p key={range.source + range.author}>
                      {range.author} · {sourceLabel(range.source)} · 已读取{" "}
                      {range.pagesRead} 页 ·{" "}
                      {range.errorCode ? "来源读取未完成" : "检查未完成"}
                    </p>
                  ))}
              </details>
            )}
          {visible.length === 0 && (
            <p className="source-empty">
              {records.length > 0
                ? "当前筛选没有结果。"
                : mode === "search"
                  ? "输入作者名，点击“搜索两站作品”读取结果。"
                  : "点击“检查作者更新”读取关注作者的作品。"}
            </p>
          )}
          {visible.length > 0 && (
            <div className="completion-controls" aria-label="作者作品多选">
              <button
                aria-pressed={selectionMode}
                onClick={() => {
                  setSelectionMode(!selectionMode);
                  setSelection([]);
                }}
              >
                {selectionMode ? "退出多选" : "多选"}
              </button>
              {selectionMode && (
                <>
                  <button
                    data-testid="completion-select-all"
                    disabled={!complete || !selectable.length}
                    onClick={() =>
                      setSelection(
                        selectable.map((record) => sourceWorkKey(record.work)),
                      )
                    }
                  >
                    全选当前筛选范围 · {selectable.length} 本
                  </button>
                  <span className="source-muted">
                    {complete
                      ? "包含未滚动到的作品，已入库作品不选入。"
                      : "当前检查范围未读完；可逐本勾选已读结果，读完后再全选。"}
                  </span>
                </>
              )}
            </div>
          )}
          <VirtualSourceGrid
            items={visible}
            density={density}
            itemKey={(record) => sourceWorkKey(record.work)}
            key={scopeKey + author + source + filter + query}
            renderItem={(record) => {
              const work = record.work,
                scope = scopes.find((value) => value.source === work.source)!;
              const stock = inventory(work);
              return (
                <article
                  className={`source-card${selectionKeys.has(sourceWorkKey(work)) ? " is-selected" : ""}`}
                  data-testid={"author-update-" + sourceWorkKey(work)}
                >
                  {selectionMode && (
                    <label className="completion-select">
                      <input
                        type="checkbox"
                        aria-label={"选择 " + work.title}
                        disabled={stock.kind === "owned"}
                        checked={selected.some(
                          (item) => sourceWorkKey(item) === sourceWorkKey(work),
                        )}
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
                      选择
                    </label>
                  )}
                  <button
                    className="source-card-open"
                    onClick={() =>
                      onOpenWork({ source: work.source, workId: work.workId })
                    }
                  >
                    <SourceCover
                      adapter={sourceAdapter}
                      scope={scope}
                      work={work}
                    />
                    <strong>{work.title}</strong>
                    <span>
                      {work.authors.join("、") ||
                        record.matchedAuthors.join("、")}
                    </span>
                  </button>
                  <p>
                    {sourceLabel(work.source)} · {inventoryLabel(stock)}
                  </p>
                  <button
                    className="text-button"
                    disabled={
                      stock.kind === "owned" || !library.rootId || downloadBusy
                    }
                    onClick={() => onDownload(work)}
                  >
                    {stock.kind === "owned" ? "已入库" : "下载到漫画库"}
                  </button>
                </article>
              );
            }}
          />
          {selected.length > 0 && (
            <div
              className="source-selection-bar"
              data-testid="completion-selection-bar"
            >
              <strong>已选 {selected.length} 本</strong>
              <span>
                {complete
                  ? "当前筛选范围，包含未滚动到的作品"
                  : "仅来自已读取结果，检查范围尚未完成"}
              </span>
              <button onClick={() => setSelection([])}>取消选择</button>
              <button
                className="primary-button"
                disabled={
                  downloadBusy ||
                  !library.rootId ||
                  selected.length > downloadSelectionLimit
                }
                onClick={() => onDownloadMany(selected)}
              >
                查看下载计划
              </button>
              {selected.length > downloadSelectionLimit && (
                <span>
                  一次最多选择 500 本，请缩小筛选范围；没有截取或忽略后续作品。
                </span>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}
