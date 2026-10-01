import { CoverInteraction } from "./reader-access.tsx";
import { AuthorLinks } from "./AuthorLinks.tsx";
import { FloatingSelection } from "./FloatingSelection.tsx";
import { useBrowseSession } from "./useBrowseSession.ts";
import { DownloadWorkButton } from "./DownloadWorkButton.tsx";
import {
  isContentHidden,
  rememberContentWork,
  getContentFilterRevision,
  subscribeContentFilter,
} from "./content-filter.ts";
import { useSyncExternalStore } from "react";
import { rememberAuthorCatalog } from "./author-catalog-membership.ts";
import {
  subscribeAuthorCatalogChanges,
  notifyAuthorCatalogChanged,
} from "./author-catalog-events.ts";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useReaderAccess, sourceReaderRequest } from "./reader-access.tsx";
import type {
  AccountSummary,
  SourceAdapter,
  SourceScope,
  SourceWork,
  Source,
  AuthorCreditContext,
} from "./source-types.ts";
import { accountScope, sourceWorkKey, sourceLabel } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { DownloadInventorySnapshot } from "./download-types.ts";
import { downloadSelectionLimit } from "./download-types.ts";
import type { WorkReference } from "./booklists.ts";
import type {
  CompletionAdapter,
  DiscoveryMode,
  DiscoverySnapshot,
  RecentCheckRun,
} from "./completion-types.ts";
import {
  completionError,
  completionReadFailure,
  createCompletionAdapter,
  unfinishedRangeMessage,
  authorCatalogAt,
} from "./completion-runtime.ts";
import type { CompletionReadFailure } from "./completion-runtime.ts";
import {
  createInventoryMatcher,
  inventoryFilterLabels,
  inventoryFilterMatches,
  inventoryLabel,
  inventoryScopeNote,
} from "./inventory-model.ts";
import type { InventoryFilter } from "./inventory-model.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { SourceLanguageBadge } from "./SourceLanguageBadge.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import { createAuthorSearchAdapter } from "./author-search.ts";
import { partitionAuthorRecords } from "./author-evidence.ts";
import {
  summarizeDiscoveryChanges,
  uniqueDiscoveryRecords,
} from "./discovery-summary.ts";
import { AuthorCreditNote } from "./AuthorCreditNote.tsx";
import { jmSearchScopeNote } from "./source-search.ts";
import {
  formatWorkDate,
  normalizedWorkDate,
  readSortPreference,
  sortByWorkDate,
  updatedSorts,
  writeSortPreference,
} from "./work-dates.ts";
import type { UpdatedSort } from "./work-dates.ts";
import "./completion.css";
import { SourceIssues } from "./SourceIssues.tsx";

const nativeAdapter = createCompletionAdapter();
type ReadRequest = { kind: "full" | "progress"; includeOther: boolean };
const emptyRecords: DiscoverySnapshot["records"] = [];
export interface CompletionPanelProps {
  active?: boolean;
  mode?: "updates" | "search";
  searchTabId?: string;
  authorRequest?: { name: string; key: number } | null;
  accounts: AccountSummary[];
  sourceAdapter: SourceAdapter;
  adapter?: CompletionAdapter;
  library: LibrarySnapshot;
  inventorySnapshot?: DownloadInventorySnapshot;
  inventoryReady?: boolean;
  inventoryError?: string | null;
  onRefreshInventory?(): Promise<void>;
  density: 5 | 7 | 9;
  onOpenWork(
    reference: WorkReference,
    creditContext?: AuthorCreditContext,
  ): void;
  onDownload(work: SourceWork): void;
  onDownloadMany(works: SourceWork[]): Promise<string[]> | void;
  downloadBusy?: boolean;
  onOpenLibrary(): void;
  onOpenAccounts(): void;
}

export function CompletionPanel({
  active = true,
  mode = "updates",
  searchTabId,
  authorRequest = null,
  accounts,
  sourceAdapter,
  adapter: providedAdapter,
  library,
  inventorySnapshot,
  inventoryReady = false,
  inventoryError = null,
  onRefreshInventory,
  density,
  onOpenWork,
  onDownload,
  onDownloadMany,
  downloadBusy = false,
  onOpenLibrary,
  onOpenAccounts,
}: CompletionPanelProps) {
  const renderStartedAt = performance.now();
  const [paintMetrics, setPaintMetrics] = useState<{
    revision: number;
    runId: string;
    value: NonNullable<DiscoverySnapshot["searchMetrics"]>;
  } | null>(null);
  const root = useRef<HTMLElement>(null);
  const grid = useRef<SourceGridHandle>(null);
  const handledAuthorRequest = useRef<number | null>(null);
  const contentRevision = useSyncExternalStore(
    subscribeContentFilter,
    getContentFilterRevision,
  );

  const searchAdapter = useMemo(
    () => createAuthorSearchAdapter(sourceAdapter),
    [sourceAdapter],
  );
  const adapter =
    providedAdapter ?? (mode === "search" ? searchAdapter : nativeAdapter);
  useEffect(() => {
    if (mode === "search") searchAdapter.setActive(active);
  }, [active, mode, searchAdapter]);
  const [searchAuthor, setSearchAuthor] = useState("");
  const scopes = accounts
    .map(accountScope)
    .filter((s): s is SourceScope => s !== null);
  const scopeKey = JSON.stringify(scopes);
  const current = useRef({ key: scopeKey, scopes, active, adapter, mode });
  current.current = { key: scopeKey, scopes, active, adapter, mode };
  const epoch = useRef(0),
    operation = useRef(false),
    mounted = useRef(true),
    readTask = useRef<Promise<void> | null>(null),
    readFailures = useRef(0),
    failedRead = useRef<ReadRequest>({ kind: "full", includeOther: false }),
    otherView = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const [result, setResult] = useState<{
    key: string;
    value: DiscoverySnapshot;
    catalogCheckId: string | null;
  } | null>(null);
  const view = result?.key === scopeKey ? result.value : null;
  const searchMetrics =
    paintMetrics !== null &&
    paintMetrics.runId === view?.run?.id &&
    paintMetrics?.revision === view?.revision
      ? paintMetrics.value
      : view?.searchMetrics;
  const [busy, setBusy] = useState(false),
    [actionError, setActionError] = useState("");
  const [reading, setReading] = useState(false);
  const [recentCheck, setRecentCheck] = useState<{
    key: string;
    run: RecentCheckRun;
  } | null>(null);
  const [recentError, setRecentError] = useState("");
  const [recentRefresh, setRecentRefresh] = useState(0);
  const recentRun = recentCheck?.key === scopeKey ? recentCheck.run : null;
  const recentRunning = recentRun?.phase === "checking";
  const [readFailure, setReadFailure] = useState<CompletionReadFailure | null>(
    null,
  );
  const [author, setAuthor] = useState(""),
    [query, setQuery] = useState("");
  const [source, setSource] = useState<Source | "all">("all");
  const [filter, setFilter] = useState<InventoryFilter>("missing");
  const sortPage = mode === "search" ? "author-search" : "author-updates";
  const [sort, setSort] = useState<UpdatedSort>(() =>
    readSortPreference(sortPage, updatedSorts, "updated-desc"),
  );
  useEffect(() => {
    setSort(readSortPreference(sortPage, updatedSorts, "updated-desc"));
  }, [sortPage]);
  const [showOther, setShowOther] = useState(false);
  const [newOnly, setNewOnly] = useState(false);
  const checkSummary = view?.lastCheck;
  otherView.current = showOther;
  const [selectionMode, setSelectionMode] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  useEffect(() => {
    setSelection([]);
  }, [
    scopeKey,
    author,
    source,
    filter,
    query,
    mode,
    showOther,
    newOnly,
    checkSummary?.id,
  ]);
  useEffect(() => {
    setNewOnly(false);
  }, [scopeKey, mode, checkSummary?.id]);
  const connected = scopes.length === 2;
  const authorRunning = view?.run?.phase === "checking";
  const running = authorRunning || recentRunning;
  const inventory = useMemo(
    () => createInventoryMatcher(library, inventorySnapshot, inventoryReady),
    [library, inventorySnapshot, inventoryReady],
  );
  const authorResults = useMemo(() => {
    const partition = partitionAuthorRecords(
      view?.records ?? emptyRecords,
      author,
      source,
      view?.authorPolicies,
      view?.followedAuthors ??
        (view?.authors.length
          ? [...new Set(view.authors.map((range) => range.author))]
          : undefined),
    );
    return {
      confirmed: uniqueDiscoveryRecords(partition.confirmed),
      other: uniqueDiscoveryRecords(partition.other),
    };
  }, [
    view?.records,
    view?.authorPolicies,
    view?.followedAuthors,
    view?.authors,
    author,
    source,
  ]);
  const load = useCallback(
    async (
      resetRetries = false,
      read: ReadRequest = { kind: "full", includeOther: otherView.current },
    ) => {
      if (operation.current && !resetRetries) return;
      const captured = current.current,
        request = ++epoch.current;
      const valid = () =>
        mounted.current &&
        current.current.active &&
        current.current.key === captured.key &&
        current.current.mode === captured.mode &&
        current.current.adapter === adapter &&
        request === epoch.current;
      if (!captured.active || captured.scopes.length !== 2) return;
      if (resetRetries) readFailures.current = 0;
      // An invalidated IPC cannot be cancelled. Wait for it before the newest
      // read so manual refresh, navigation and StrictMode never overlap polls.
      await readTask.current;
      if (!valid()) return;
      setReading(true);
      const task = (async () => {
        try {
          if (read.kind === "progress") {
            const progress = await adapter.progress(captured.scopes);
            if (!valid()) return;
            if (progress.run?.phase === "checking") {
              setResult((previous) => ({
                key: captured.key,
                catalogCheckId:
                  previous?.key === captured.key
                    ? previous.catalogCheckId
                    : null,
                value: {
                  ...progress,
                  records:
                    previous?.key === captured.key
                      ? previous.value.records
                      : emptyRecords,
                  includesOther:
                    previous?.key === captured.key
                      ? previous.value.includesOther
                      : false,
                },
              }));
              readFailures.current = 0;
              setReadFailure(null);
              return;
            }
            // A terminal run gets its catalog once. If that read fails, retry
            // the catalog itself, not an already-completed progress request.
            read = { kind: "full", includeOther: otherView.current };
          }
          const next = await adapter.read(captured.scopes, read.includeOther);
          if (valid()) {
            readFailures.current = 0;
            setResult({
              key: captured.key,
              value: next,
              catalogCheckId: next.lastCheck?.id ?? null,
            });
            setReadFailure(null);
            if (read.includeOther) setShowOther(true);
          }
        } catch (cause) {
          if (valid()) {
            failedRead.current = read;
            setReadFailure(
              completionReadFailure(cause, ++readFailures.current),
            );
          }
        }
      })();
      readTask.current = task;
      await task;
      if (readTask.current === task) {
        readTask.current = null;
        if (mounted.current) setReading(false);
      }
    },
    [adapter],
  );
  useEffect(() => {
    setResult(null);
    setActionError("");
    setReadFailure(null);
    readFailures.current = 0;
    setAuthor("");
    setQuery("");
    setSearchAuthor("");
    handledAuthorRequest.current = null;
    setSource("all");
    setFilter("missing");
    setShowOther(false);
    setRecentCheck(null);
    setRecentError("");
    // Invalidate the previous session's in-flight search, including logout.
    // Reading this in-memory adapter never starts a source request.
    if (mode === "search") void searchAdapter.read(current.current.scopes);
  }, [scopeKey, mode, searchAdapter]);
  useEffect(
    () => () => {
      if (mode === "search") void searchAdapter.read([]);
    },
    [mode, searchAdapter],
  );
  useEffect(() => {
    if (active) void load(true);
    return () => {
      epoch.current++;
    };
  }, [load, scopeKey, active, mode]);
  useEffect(() => {
    if (!active || !adapter.subscribe) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = adapter.subscribe(() => {
      if (timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        void load(false);
      }, 60);
    });
    return () => {
      unsubscribe();
      clearTimeout(timer);
    };
  }, [adapter, active, scopeKey, load]);
  useEffect(() => {
    if (!active || mode !== "updates") return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = subscribeAuthorCatalogChanges((change) => {
      if (
        !current.current.scopes.some(
          (scope) =>
            scope.source === change.source &&
            scope.sessionId === change.sessionId,
        )
      )
        return;
      clearTimeout(timer);
      timer = setTimeout(() => void load(false), 250);
    });
    return () => {
      unsubscribe();
      clearTimeout(timer);
    };
  }, [active, mode, scopeKey, load]);
  useEffect(() => {
    if (!active || !connected || busy || reading) return;
    const delay = readFailure
      ? readFailure.retryAfterMs
      : authorRunning && !adapter.subscribe
        ? 1500
        : null;
    if (delay === null) return;
    const request: ReadRequest = readFailure
      ? failedRead.current
      : { kind: "progress", includeOther: false };
    const timer = setTimeout(() => void load(false, request), delay);
    return () => clearTimeout(timer);
  }, [
    load,
    authorRunning,
    busy,
    reading,
    readFailure,
    view,
    active,
    connected,
  ]);
  useEffect(() => {
    if (
      !active ||
      !connected ||
      mode !== "updates" ||
      !adapter.recentCheckProgress
    )
      return;
    let alive = true,
      timer: ReturnType<typeof setTimeout> | undefined;
    const captured = current.current;
    const poll = async () => {
      try {
        const next = await adapter.recentCheckProgress!();
        if (!alive || current.current.key !== captured.key) return;
        setRecentError("");
        if (next) {
          setRecentCheck({ key: captured.key, run: next });
          if (next.phase === "checking")
            timer = setTimeout(() => void poll(), 1500);
          else if (recentRunning)
            for (const scope of captured.scopes)
              notifyAuthorCatalogChanged(scope);
        }
      } catch {
        if (alive)
          setRecentError(
            "近期补漏进度暂未读取，请刷新显示后重试；不代表后台检查已经停止。",
          );
      }
    };
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [
    adapter,
    active,
    connected,
    mode,
    scopeKey,
    recentRunning,
    recentRefresh,
  ]);
  async function perform(
    action: (current: () => boolean) => Promise<DiscoverySnapshot | void>,
  ) {
    if (operation.current) return;
    operation.current = true;
    setSelection([]);
    const request = ++epoch.current,
      captured = current.current;
    const valid = () =>
      mounted.current &&
      current.current.active &&
      current.current.key === captured.key &&
      current.current.mode === captured.mode &&
      current.current.adapter === captured.adapter &&
      request === epoch.current;
    setBusy(true);
    setActionError("");
    try {
      const next = await action(valid);
      if (valid()) {
        if (next) {
          readFailures.current = 0;
          setResult({
            key: captured.key,
            value: next,
            catalogCheckId: next.lastCheck?.id ?? null,
          });
          setReadFailure(null);
        } else await load(true);
      }
    } catch (cause) {
      if (valid()) {
        setActionError(completionError(cause));
        // A failed start, cancellation or inventory refresh does not establish
        // that the backend run stopped. Read its state independently.
        await load(true);
      }
    } finally {
      operation.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  function startCheck(
    checkMode: DiscoveryMode = "incremental",
    unfinishedOnly = false,
    requestedName?: string,
  ) {
    setShowOther(false);
    setNewOnly(false);
    if (mode === "updates") setFilter("missing");
    const captured = current.current;
    const selected =
      mode === "search"
        ? [(requestedName ?? searchAuthor).trim()]
        : author
          ? [author]
          : [];
    void perform(async (isCurrent) => {
      if (mode === "search") {
        // Ownership refresh is independent of displaying saved/source results.
        void onRefreshInventory?.().catch(() => {});
      } else await onRefreshInventory?.();
      if (!isCurrent()) return;
      if (unfinishedOnly)
        return adapter.startUnfinished(captured.scopes, selected);
      const next = await adapter.start(
        captured.scopes,
        selected,
        mode === "search" ? "full" : checkMode,
      );
      if (mode === "updates" && adapter.startRecentCheck) {
        try {
          const recent = await adapter.startRecentCheck(captured.scopes);
          if (isCurrent()) {
            setRecentError("");
            setRecentCheck({ key: captured.key, run: recent });
          }
        } catch {
          if (isCurrent())
            setRecentError(
              "作者查询已开始，但近期交叉补漏未能启动；本次不包含近期连续范围，请重试检查。",
            );
        }
      }
      return next;
    });
  }
  useEffect(() => {
    if (
      mode !== "search" ||
      !active ||
      !connected ||
      busy ||
      running ||
      !authorRequest ||
      handledAuthorRequest.current === authorRequest.key
    )
      return;
    handledAuthorRequest.current = authorRequest.key;
    setSearchAuthor(authorRequest.name);
    setSource("all");
    setAuthor("");
    setQuery("");
    setFilter("all");
    startCheck("full", false, authorRequest.name);
  }, [authorRequest, mode, active, connected, busy, running]);
  useEffect(() => {
    if (!view) return;
    view.records.forEach((record) => rememberContentWork(record.work));
    if (mode === "updates") rememberAuthorCatalog(view);
  }, [view?.records, view?.authorPolicies, scopeKey, mode]);
  const authors = view?.followedAuthors ?? [
    ...new Set(view?.authors.map((range) => range.author) ?? []),
  ];
  const ranges = (view?.authors ?? []).filter(
    (range) =>
      (!author || range.author === author) &&
      (source === "all" || range.source === source),
  );
  // The source display filter does not change which sources a check covers.
  const pendingScopes = (view?.authors ?? []).filter(
    (range) =>
      (!author || range.author === author) && range.state !== "complete",
  ).length;
  const complete =
    !running &&
    !actionError &&
    !readFailure &&
    !view?.observationErrorCode &&
    ranges.length > 0 &&
    ranges.every((range) => range.state === "complete");
  const includesIncremental =
    mode === "updates" &&
    ranges.some((range) => range.lastCheckMode === "incremental");
  const issueCount = ranges.reduce(
    (sum, range) => sum + (range.issueCount ?? 0),
    0,
  );
  const pagesComplete =
    !running &&
    !actionError &&
    !readFailure &&
    ranges.length > 0 &&
    ranges.every(
      (range) =>
        range.pagesComplete ??
        (range.state === "complete" && range.lastCheckMode !== "incremental"),
    );
  const fullRangeChecked =
    complete && !includesIncremental && !view?.historicalReadError;
  const catalogScopes = ranges.filter(
    (range) => authorCatalogAt(range, view?.authorPolicies) !== null,
  ).length;
  const terms = query.normalize("NFKC").toLocaleLowerCase().trim();
  const scopedRecords = useMemo(
    () =>
      (showOther ? authorResults.other : authorResults.confirmed).filter(
        (record) => !isContentHidden(record.work),
      ),
    [authorResults, showOther, contentRevision],
  );
  const matchingRecords = useMemo(
    () =>
      scopedRecords.filter(
        (record) =>
          !terms ||
          [record.work.title, ...record.work.authors]
            .join(" ")
            .normalize("NFKC")
            .toLocaleLowerCase()
            .includes(terms),
      ),
    [scopedRecords, terms],
  );
  const summaryReady =
    mode === "updates" &&
    !showOther &&
    !!checkSummary &&
    checkSummary.phase !== "checking" &&
    !running &&
    !readFailure &&
    result?.catalogCheckId === checkSummary.id;
  const changeCounts = useMemo(
    () =>
      summaryReady && checkSummary
        ? summarizeDiscoveryChanges(matchingRecords, checkSummary.id, inventory)
        : null,
    [summaryReady, checkSummary?.id, matchingRecords, inventory],
  );
  const onlyNewVisible = newOnly && summaryReady;
  const records = useMemo(
    () =>
      onlyNewVisible
        ? matchingRecords.filter(
            (record) => record.firstDiscoveredRunId === checkSummary?.id,
          )
        : matchingRecords,
    [matchingRecords, onlyNewVisible, checkSummary?.id],
  );
  const { counts, visible, selectable } = useMemo(() => {
    const counts: Record<InventoryFilter, number> = {
      all: records.length,
      owned: 0,
      missing: 0,
      unknown: 0,
    };
    const visible: typeof records = [],
      selectable: typeof records = [];
    // Progress-only replies keep this catalog reference: large membership,
    // ownership and title-filter passes run only when their inputs change.
    for (const record of records) {
      const stock = inventory(record.work);
      counts[stock.kind === "unconfigured" ? "unknown" : stock.kind]++;
      if (!inventoryFilterMatches(stock, filter)) continue;
      visible.push(record);
      if (!showOther && stock.kind !== "owned") selectable.push(record);
    }
    return { counts, visible, selectable };
  }, [records, inventory, filter, showOther]);
  const sortedVisible = useMemo(
    () =>
      sortByWorkDate(visible, (record) => record.work.sourceUpdatedAt, sort),
    [visible, sort],
  );
  const datedCount = useMemo(
    () =>
      visible.filter(
        (record) => normalizedWorkDate(record.work.sourceUpdatedAt) !== null,
      ).length,
    [visible],
  );
  const undatedCount = visible.length - datedCount;
  const allScopedOwned = useMemo(
    () =>
      scopedRecords.length > 0 &&
      scopedRecords.every((record) => inventory(record.work).kind === "owned"),
    [scopedRecords, inventory],
  );
  const othersLoaded = view?.includesOther !== false;
  const otherCount = othersLoaded
    ? authorResults.other.length
    : !author && source === "all"
      ? (view?.otherRecordCount ?? 0)
      : null;
  const selectionKeys = new Set(selection);
  const selected = selection.length
    ? selectable
        .filter((record) => selectionKeys.has(sourceWorkKey(record.work)))
        .map((record) => record.work)
    : [];
  const lastCheck = Math.max(
    0,
    ...ranges.map(
      (range) =>
        range.lastCheckedAt ?? range.lastCompleteAt ?? range.lastAttemptAt ?? 0,
    ),
  );
  const lastFullCheck = Math.max(
    0,
    ...ranges.map((range) => authorCatalogAt(range, view?.authorPolicies) ?? 0),
  );
  useBrowseSession({
    scope: JSON.stringify([
      mode,
      searchTabId ?? null,
      scopeKey,
      author,
      source,
      filter,
      query,
      showOther,
      sort,
      onlyNewVisible,
    ]),
    active,
    root,
    grid,
    itemKeys: sortedVisible.map((record) => sourceWorkKey(record.work)),
  });
  useLayoutEffect(() => {
    if (!active || mode !== "search" || !view) return;
    const frame = requestAnimationFrame(() => {
      if (!view.run) return;
      const value = searchAdapter.rendered(
        view.run.id,
        view.revision,
        performance.now() - renderStartedAt,
        sortedVisible.length > 0,
      );
      if (value)
        setPaintMetrics({ runId: view.run.id, revision: view.revision, value });
    });
    return () => cancelAnimationFrame(frame);
  }, [active, mode, view?.revision, searchAdapter]);
  // Preserve state and memoized catalog while another page is visible.
  if (!active) return null;
  return (
    <section
      ref={root}
      className={`completion-panel${selected.length ? " has-completion-selection" : ""}`}
      data-testid="completion-panel"
    >
      <header className="page-heading">
        <h1>{mode === "search" ? "作者搜索" : "作者更新"}</h1>
        <p>
          {mode === "search"
            ? "输入作者名，读取 JM 与哔咔的完整查询结果，无需先关注。"
            : "检查关注作者的新作品，保留之前未下载的漫画，默认只显示未入库内容。"}
        </p>
      </header>
      {mode === "search" && view && (
        <>
          <p role="status" data-testid="author-search-cache-status">
            {running
              ? "已保存结果可先浏览，正在分批更新；当前结果尚未读取完整。"
              : "本标签保留上次结果；重新打开不会自动重查，可手动重新搜索。"}
          </p>
          {searchMetrics && (
            <details
              className="author-search-timings"
              data-testid="author-search-timings"
            >
              <summary>搜索耗时</summary>
              <p>
                首次结果可用：
                {searchMetrics.firstRecordsMs === null
                  ? "尚无结果"
                  : `${Math.round(searchMetrics.firstRecordsMs)} ms`}{" "}
                · 首次可见：
                {searchMetrics.firstVisibleMs === null
                  ? "尚未记录"
                  : `${Math.round(searchMetrics.firstVisibleMs)} ms`}{" "}
                · 全部结束：
                {searchMetrics.completedMs === null
                  ? "尚未结束"
                  : `${Math.round(searchMetrics.completedMs)} ms`}
              </p>
              <p>
                本地目录读取 {Math.round(searchMetrics.localCatalogMs)} ms ·
                查询规则 {Math.round(searchMetrics.policyMs)} ms · 请求排队{" "}
                {Math.round(
                  searchMetrics.queueMs + searchMetrics.nativeQueueMs,
                )}{" "}
                ms · 来源请求 {Math.round(searchMetrics.sourceOperationMs)} ms ·
                本地登记 {Math.round(searchMetrics.localCommitMs)} ms · 界面更新{" "}
                {Math.round(searchMetrics.renderMs)} ms
              </p>
              <p className="source-muted">
                分项可能并行，不能相加当作总耗时。来源请求包含网络、协议解析及来源重试；传递等其余耗时{" "}
                {Math.round(searchMetrics.transportOtherMs)} ms。
                缺少原生分段计时的请求{" "}
                {searchMetrics.requestsWithoutNativeTiming} 次；失败请求{" "}
                {searchMetrics.failedRequests}{" "}
                次。结束耗时不代表结果完整，以范围状态为准。
              </p>
            </details>
          )}
        </>
      )}
      {!connected ? (
        <div className="source-empty">
          <p>请连接 JM 和哔咔账号后检查。</p>
          <button onClick={onOpenAccounts}>连接账号</button>
        </div>
      ) : (
        <>
          <div className="completion-controls source-page-tools">
            {mode === "search" ? (
              searchTabId ? (
                <strong>{authorRequest?.name}</strong>
              ) : (
                <label>
                  作者名{" "}
                  <input
                    aria-label="搜索作者名"
                    value={searchAuthor}
                    onChange={(event) => setSearchAuthor(event.target.value)}
                    disabled={busy || running}
                  />
                </label>
              )
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
              data-testid={
                searchTabId ? "author-tab-refresh" : "completion-start"
              }
              onClick={() => startCheck()}
            >
              {mode === "search"
                ? searchTabId
                  ? "重新搜索当前作者"
                  : "搜索两站作品"
                : author
                  ? "检查该作者新增作品"
                  : "一键检查全部关注作者"}
            </button>
            {mode === "updates" && (
              <button
                disabled={busy || running || pendingScopes === 0}
                data-testid="completion-unfinished-check"
                onClick={() => startCheck("incremental", true)}
              >
                仅补查未完成{pendingScopes > 0 ? `（${pendingScopes}）` : ""}
              </button>
            )}
            {mode === "updates" && (
              <button
                disabled={busy || running || !authors.length}
                data-testid="completion-full-check"
                onClick={() => startCheck("full")}
              >
                完整复核
              </button>
            )}
            {running && (
              <button
                disabled={busy}
                onClick={() =>
                  void perform(async (isCurrent) => {
                    const cancels: Promise<unknown>[] = [];
                    if (authorRunning && view?.run)
                      cancels.push(adapter.cancel(view.run.id));
                    if (recentRunning && recentRun && adapter.cancelRecentCheck)
                      cancels.push(adapter.cancelRecentCheck(recentRun.id));
                    const outcomes = await Promise.allSettled(cancels);
                    const failure = outcomes.find(
                      (result) => result.status === "rejected",
                    );
                    if (adapter.recentCheckProgress) {
                      try {
                        const recent = await adapter.recentCheckProgress();
                        if (isCurrent() && recent)
                          setRecentCheck({ key: scopeKey, run: recent });
                      } catch (cause) {
                        if (!failure) throw cause;
                      }
                    }
                    // Progress success must not erase a failed stop action or
                    // imply either backend run has already stopped.
                    if (failure?.status === "rejected") throw failure.reason;
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
                  setRecentRefresh((value) => value + 1);
                  await onRefreshInventory?.();
                })
              }
            >
              刷新显示与入库状态
            </button>
          </div>
          {mode === "updates" && view && !authors.length && (
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
          {authorRunning && (
            <p
              className="page-summary"
              role="status"
              data-testid="completion-progress"
            >
              {readFailure
                ? "上次读取的检查进度（当前进度待刷新）："
                : "正在检查 "}
              {view?.run?.currentAuthor ?? "关注作者"} ·{" "}
              {view?.run?.currentSource
                ? sourceLabel(view.run.currentSource)
                : "准备中"}{" "}
              {view?.run?.currentQueryCount && view.run.currentQueryCount > 1
                ? ` · 检索词 ${view.run.currentQueryIndex} / ${view.run.currentQueryCount}`
                : ""}
              · 第 {view?.run?.currentPage ?? 0} 页 · 已检查{" "}
              {view?.run?.completedScopes ?? 0} / {view?.run?.totalScopes ?? 0}{" "}
              个来源范围
              {mode === "updates" && view?.run?.currentStrategy
                ? view.run.currentStrategy === "incremental"
                  ? " · 本范围增量检查"
                  : " · 本范围读取完整目录"
                : ""}
            </p>
          )}
          {running && (
            <p
              className="source-muted"
              data-testid="completion-saved-results-note"
            >
              检查进行中，列表为最近读取结果；结束后自动刷新。“刷新显示与入库状态”仅读取本机目录，不重新检查网站。
            </p>
          )}
          {mode === "updates" && (recentRun || recentError) && (
            <div className="source-muted" data-testid="completion-recent-check">
              {recentRun && (
                <p>
                  近期交叉补漏：
                  {recentRun.phase === "checking"
                    ? "正在读取"
                    : recentRun.phase === "complete"
                      ? "本次范围已保存"
                      : recentRun.phase === "cancelled"
                        ? "已停止，范围未完成"
                        : "范围未完成"}
                  {recentRun.currentSource
                    ? ` · ${sourceLabel(recentRun.currentSource)} 第 ${recentRun.currentPage} 页`
                    : ""}
                  {` · 已读取 ${recentRun.pagesRead} 页 / ${recentRun.recordsRead} 条`}
                </p>
              )}
              {recentRun?.results.map((result) => (
                <p key={result.source}>
                  {sourceLabel(result.source)}：{result.pagesRead} 页，
                  {result.errorCode
                    ? "读取未完成"
                    : result.reachedEnd
                      ? "来源入口分页已读完"
                      : result.joinedPrevious
                        ? "已衔接上次范围"
                        : result.initialWindow
                          ? "初始窗口已保存，更早历史未覆盖"
                          : "已保存部分范围"}
                  。
                </p>
              ))}
              {recentError && <p role="alert">{recentError}</p>}
            </div>
          )}
          {actionError && (
            <p role="alert" className="source-notice">
              {actionError}
            </p>
          )}
          {view?.observationErrorCode && (
            <p className="source-notice" role="alert">
              跨入口补录未完成，已保存结果保留；新读取作品尚未全部纳入作者目录，请刷新显示后核对。
            </p>
          )}
          {mode === "search" &&
            ((view?.historicalSupplementCount ?? 0) > 0 ||
              view?.historicalReadError) && (
              <p
                className="source-muted"
                data-testid="completion-search-history"
              >
                已知作者目录补充 {view?.historicalSupplementCount ?? 0}{" "}
                条；不计入本次来源分页和读取进度。
                {view?.historicalSupplementAt
                  ? ` 目录最近核验：${new Date(view.historicalSupplementAt).toLocaleString()}。`
                  : ""}
                {view?.historicalReadError
                  ? " 已保存目录暂未读取成功，当前统计未覆盖这部分，请重试搜索。"
                  : ""}
              </p>
            )}
          {view?.run?.storageWarningCode && (
            <p
              className="source-notice"
              data-testid="completion-storage-warning"
            >
              目录整理暂未完成，已读取结果已保存，下次检查时会再尝试。
            </p>
          )}
          {readFailure && (
            <p
              role="alert"
              className="source-notice"
              data-testid="completion-read-error"
            >
              检查进度暂未刷新，已显示结果保留；这不表示后台检查已停止。
              错误代码：{readFailure.code}。
              {readFailure.retryAfterMs !== null
                ? ` ${readFailure.retryAfterMs / 1000} 秒后重试读取（${readFailure.failures} / 3）。`
                : " 进度刷新已暂停，请点击“刷新显示与入库状态”重试。"}
            </p>
          )}
          {inventoryError && (
            <p
              role="alert"
              className="source-notice"
              data-testid="completion-inventory-error"
            >
              入库状态暂未核对完成，当前不能判断已入库或未入库。请点击“刷新显示与入库状态”重试。
            </p>
          )}
          <div className="completion-controls source-page-tools">
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
              type="search"
              aria-label="筛选作者更新"
              placeholder="筛选已发现作品或作者…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <label>
              排序{" "}
              <select
                aria-label="作者作品排序"
                data-testid="completion-sort"
                value={sort}
                onChange={(event) => {
                  const value = event.target.value as UpdatedSort;
                  setSort(value);
                  writeSortPreference(sortPage, value);
                }}
              >
                <option value="updated-desc">更新时间：从新到旧</option>
                <option value="updated-asc">更新时间：从旧到新</option>
                <option value="source">目录顺序</option>
              </select>
            </label>
          </div>
          {mode === "updates" && !showOther && (
            <section
              className="completion-change-summary"
              aria-label="本次检查变化"
              data-testid="completion-change-summary"
            >
              <div className="completion-change-heading">
                <strong>本次检查变化</strong>
                <button
                  data-testid="completion-new-only"
                  aria-pressed={onlyNewVisible}
                  disabled={!summaryReady}
                  onClick={() => {
                    setNewOnly(!onlyNewVisible);
                    setSelection([]);
                  }}
                >
                  仅看本次新发现
                </button>
              </div>
              {!checkSummary ? (
                <p className="source-muted">
                  下一次检查后生成变化摘要，已有目录按历史记录保留。
                </p>
              ) : (
                <>
                  <p
                    className="source-muted"
                    data-testid="completion-change-range"
                  >
                    {readFailure
                      ? "上次读取的检查记录（当前状态待刷新）"
                      : {
                          checking: "本次检查进行中",
                          complete: "本次检查已完成",
                          partial: "本次检查部分完成",
                          cancelled: "本次检查已停止",
                          error: "本次检查未完成",
                          interrupted: "上次检查被中断",
                        }[checkSummary.phase]}
                    {" · "}
                    {checkSummary.allFollowed ? "全部关注作者" : "所选作者"}
                    {checkSummary.onlyUnfinished
                      ? " · 仅补查未完成"
                      : checkSummary.mode === "full"
                        ? " · 完整复核"
                        : " · 增量检查"}
                    {" · 实际 "}
                    {checkSummary.authorCount} 位作者 /{" "}
                    {checkSummary.totalScopes} 个来源范围
                    {" · 已尝试 "}
                    {checkSummary.attemptedScopes} 个，完成{" "}
                    {checkSummary.completeScopes} 个{" · "}
                    {new Date(
                      checkSummary.finishedAt ?? checkSummary.startedAt,
                    ).toLocaleString()}
                  </p>
                  {running || checkSummary.phase === "checking" ? (
                    <p data-testid="completion-change-pending">
                      正在检查，结束后汇总本次新发现。列表仍保留最近读取结果。
                    </p>
                  ) : changeCounts ? (
                    <>
                      <p data-testid="completion-change-counts">
                        本次首次发现 {changeCounts.newTotal} 条 · 未入库{" "}
                        {changeCounts.newMissing} 条 · 已入库{" "}
                        {changeCounts.newOwned} 条 · 状态待核实{" "}
                        {changeCounts.newUnknown} 条；历史保留未入库{" "}
                        {changeCounts.historicalMissing} 条
                      </p>
                      <p className="source-muted">
                        统计按当前作者、来源和关键词范围；JM
                        与哔咔分别计数。历史未入库作品继续保留。
                        {checkSummary.firstCatalog
                          ? " 本批包含首次建立的目录，首次收录不代表网站新发布。"
                          : " 首次发现不代表网站新发布。"}
                        {checkSummary.phase !== "complete"
                          ? " 仅统计本批已读取范围，未完成范围仍需补查。"
                          : ""}
                      </p>
                    </>
                  ) : (
                    <p data-testid="completion-change-pending">
                      本次结果尚未读取完成，刷新结果后汇总；当前不显示最终新增数量。
                    </p>
                  )}
                </>
              )}
            </section>
          )}
          {(showOther || otherCount === null || otherCount > 0) && (
            <div
              className="source-notice"
              data-testid="completion-other-results"
            >
              <span>
                作者作品 {authorResults.confirmed.length} 条 ·{" "}
                {otherCount === null
                  ? "其他关键词结果按需读取"
                  : `其他关键词结果 ${otherCount} 条`}
                。
                其他结果的作者字段未对应上，保留供查看，不计入作者统计或批量下载。
              </span>{" "}
              <button
                aria-pressed={showOther}
                disabled={reading}
                onClick={() => {
                  setNewOnly(false);
                  setSelectionMode(false);
                  setSelection([]);
                  setFilter("all");
                  if (!showOther && !othersLoaded)
                    void load(true, { kind: "full", includeOther: true });
                  else setShowOther(!showOther);
                }}
              >
                {showOther ? "返回作者作品" : "查看其他关键词结果"}
              </button>
            </div>
          )}
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
          <p className="page-summary" data-testid="completion-counts">
            {onlyNewVisible ? "仅看本次新发现 · " : ""}
            {showOther ? "其他关键词结果（未确认作者归属） · " : ""}
            {readFailure
              ? "显示上次读取结果，当前进度待刷新"
              : complete
                ? includesIncremental
                  ? "本轮检查已完成（含增量），历史目录已保留"
                  : "当前检查范围已读完"
                : pagesComplete && issueCount
                  ? "分页已读完，来源记录仍待核对"
                  : "检查范围尚未读完"}{" "}
            · 已记录 {records.length} 条 · 已入库 {counts.owned} 条 · 未入库{" "}
            {counts.missing} 条 · 当前显示 {visible.length} 条
            {counts.unknown > 0 ? ` · 状态待核实 ${counts.unknown} 条` : ""}
          </p>
          {ranges
            .filter((range) => (range.issueCount ?? 0) > 0)
            .map((range) => (
              <div key={range.source + range.author}>
                <span className="source-muted">
                  {range.author} · {sourceLabel(range.source)}
                </span>
                <SourceIssues
                  source={range.source}
                  issues={range.issueSamples}
                  count={range.issueCount}
                  pagesComplete={range.pagesComplete}
                  testId="completion-source-issues"
                />
              </div>
            ))}
          {visible.length > 0 && (
            <p className="source-muted" data-testid="completion-date-coverage">
              当前显示作品：有更新时间 {datedCount} 条 · 更新时间未知{" "}
              {undatedCount} 条。
            </p>
          )}
          {sort !== "source" && (
            <p
              className="source-muted"
              data-testid="completion-date-sort-scope"
            >
              {visible.length === 0
                ? "当前没有可排序结果。"
                : datedCount === 0
                  ? "当前结果尚无可用更新时间，暂按目录顺序显示；切换时间正倒序不会改变顺序。"
                  : undatedCount > 0
                    ? `仅 ${datedCount} 条按网站更新时间排序，其余 ${undatedCount} 条日期未知，排列在最后。`
                    : "按当前保存结果的网站更新时间排序。"}
              {!complete &&
                (pagesComplete && issueCount > 0
                  ? "异常记录仍待核对，更新时间排序仅覆盖可展示作品。"
                  : "检查范围尚未读完，更新时间排序仅覆盖已读取结果。")}
            </p>
          )}
          <p className="completion-scope-summary source-muted">
            {mode === "updates"
              ? `完整目录 ${catalogScopes} / ${ranges.length} 个来源范围 · `
              : "本次 JM 与哔咔分别计数 · "}
            {lastCheck > 0
              ? `所示范围中最近一次检查 ${new Date(lastCheck).toLocaleString()}（不代表全部范围均在此时检查）`
              : "尚未完成检查"}
          </p>
          <details
            className="page-scope-details"
            data-testid="completion-scope-details"
          >
            <summary>
              检查范围与统计说明
              {source !== "Pica" ? " · JM 不含 English Manga" : ""}
            </summary>
            {mode === "updates" && (
              <p className="source-muted" data-testid="completion-check-mode">
                首次检查会读取完整目录；之后优先检查新增作品并复用历史目录。
                “完整复核”会重新读取所选作者在两站的所有分页，用于核对旧作补录等变化。
                “仅补查未完成”只读取未完成的作者与来源，已完成来源保持原样。
                主按钮始终检查当前选择的全部作者，不沿用“仅补查未完成”的范围。
                “刷新显示与入库状态”只读取本机记录，不发送作者查询。
              </p>
            )}
            {mode === "updates" && undatedCount > 0 && (
              <p
                className="source-muted"
                data-testid="completion-date-refresh-help"
              >
                旧目录可能未保存日期。可在上方选择一位作者，再点击“完整复核”重新读取该作者在两站的所有分页；来源未提供的日期仍显示未知。“刷新显示与入库状态”仅读取本机记录，不会补查网站日期。
              </p>
            )}
            <p className="source-muted">
              {inventoryScopeNote} JM
              与哔咔分别计数，未选择下载的记录会继续保留。
              {lastCheck > 0
                ? ` 上次检查：${new Date(lastCheck).toLocaleString()}`
                : " 尚未完成检查。"}
            </p>
            {mode === "updates" && (
              <p
                className="source-muted"
                data-testid="completion-catalog-scope"
              >
                已建立完整目录 {catalogScopes} / {ranges.length} 个来源范围。
                {lastFullCheck > 0
                  ? ` 最近完整读取：${new Date(lastFullCheck).toLocaleString()}。`
                  : " 尚未完成首次目录读取。"}
                {includesIncremental
                  ? " 增量检查没有重新读取所有历史分页，统计包含已保存的旧作品。"
                  : ""}
              </p>
            )}
            <p className="source-muted" data-testid="completion-query-scope">
              {mode === "search"
                ? "读取来源作者查询，并合并已知的署名明确作品；分页进度只统计本次查询。"
                : "已取得作品按明确署名归属，发现入口不限制作者归属；历史记录会保留。"}
              作者作品包含明确列出的合著者和“社团（作者）”；名称不同或信息缺失的记录保留在其他关键词结果中。
              查询分页读完不代表网站未返回的历史作品不存在。
            </p>
            {(author || mode === "search") &&
              ranges.map((range) => (
                <p
                  className="source-muted"
                  key={range.source + range.author}
                  data-testid="completion-author-check-time"
                >
                  {range.author} · {sourceLabel(range.source)} · 最后实际检查：
                  {range.lastCheckedAt
                    ? new Date(range.lastCheckedAt).toLocaleString()
                    : "尚无成功检查"}
                  {range.lastCheckMode === "incremental"
                    ? "（增量，未重读全部历史页）"
                    : range.lastCheckMode === "full"
                      ? "（完整查询）"
                      : ""}
                </p>
              ))}
            {source !== "Pica" && (
              <p className="source-muted">{jmSearchScopeNote}</p>
            )}
          </details>
          {fullRangeChecked &&
            issueCount === 0 &&
            !showOther &&
            otherCount === 0 &&
            allScopedOwned && (
              <p role="status" data-testid="completion-all-owned">
                本次作者作品已全部入库。范围：
                {source === "all" ? "JM 与哔咔" : sourceLabel(source)}，
                {author ||
                  (mode === "updates"
                    ? `${authors.length} 个关注名称`
                    : authors.join("、"))}
                ，{new Date(lastCheck).toLocaleString()}。
              </p>
            )}
          {ranges.some((range) => range.state !== "complete") && (
            <details
              className="source-notice"
              data-testid="completion-unfinished-ranges"
            >
              <summary>查看未完成范围</summary>
              {ranges
                .filter((range) => range.state !== "complete")
                .map((range) => (
                  <p key={range.source + range.author}>
                    {range.author} · {sourceLabel(range.source)} · 已读取{" "}
                    {range.pagesRead} 页 · {unfinishedRangeMessage(range)}
                  </p>
                ))}
            </details>
          )}
          {visible.length === 0 && (
            <p className="source-empty">
              {readFailure && !view
                ? "尚未读取到检查结果，请刷新重试。"
                : onlyNewVisible
                  ? "本批已读取范围内，没有符合当前筛选的首次发现作品。历史未入库作品仍保留，可关闭“仅看本次新发现”查看。"
                  : scopedRecords.length > 0
                    ? "当前筛选没有结果。"
                    : showOther
                      ? "当前范围没有其他关键词结果。"
                      : !showOther && (otherCount === null || otherCount > 0)
                        ? "尚未确认该作者的作品，可查看其他关键词结果。"
                        : fullRangeChecked
                          ? "本次完整查询没有返回作品。请核对作者名称或切换来源查看。"
                          : complete
                            ? "当前保存目录没有作者作品，可通过完整复核再次检查。"
                            : mode === "search"
                              ? "输入作者名，点击“搜索两站作品”读取结果。"
                              : "点击“一键检查全部关注作者”读取关注作者的作品。"}
            </p>
          )}
          <VirtualSourceGrid
            ref={grid}
            items={sortedVisible}
            density={density}
            itemKey={(record) => sourceWorkKey(record.work)}
            key={
              scopeKey +
              author +
              source +
              filter +
              query +
              showOther +
              sort +
              onlyNewVisible
            }
            renderItem={(record) => {
              const work = record.work,
                scope = scopes.find((value) => value.source === work.source)!;
              const stock = inventory(work);
              return (
                <article
                  className={`source-card${selectionKeys.has(sourceWorkKey(work)) ? " is-selected" : ""}`}
                  data-testid={"author-update-" + sourceWorkKey(work)}
                >
                  {selectionMode && !showOther && (
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
                  <CoverInteraction
                    className="source-card-open"
                    title={work.title}
                    request={sourceReaderRequest(scope, work)}
                    onDetails={() =>
                      onOpenWork(
                        { source: work.source, workId: work.workId },
                        {
                          scope,
                          policies: (view?.authorPolicies ?? []).filter(
                            (policy) => policy.source === work.source,
                          ),
                        },
                      )
                    }
                    selectionMode={selectionMode && !showOther}
                    selected={selectionKeys.has(sourceWorkKey(work))}
                    onToggleSelection={() => {
                      if (stock.kind !== "owned")
                        setSelection((old) =>
                          old.includes(sourceWorkKey(work))
                            ? old.filter((key) => key !== sourceWorkKey(work))
                            : [...old, sourceWorkKey(work)],
                        );
                    }}
                  >
                    <div className="source-language-cover">
                      <SourceCover
                        adapter={sourceAdapter}
                        scope={scope}
                        work={work}
                      />
                      <SourceLanguageBadge
                        tags={work.tags}
                        work={work}
                        scope={scope}
                      />
                    </div>
                  </CoverInteraction>
                  <h3>
                    <button
                      className="text-button"
                      onClick={() =>
                        onOpenWork(
                          { source: work.source, workId: work.workId },
                          {
                            scope,
                            policies: (view?.authorPolicies ?? []).filter(
                              (policy) => policy.source === work.source,
                            ),
                          },
                        )
                      }
                    >
                      {work.title}
                    </button>
                  </h3>
                  <AuthorLinks
                    authors={work.authors}
                    fallback="作者信息未提供"
                  />
                  <AuthorCreditNote work={work} />
                  <p>
                    {sourceLabel(work.source)} · {inventoryLabel(stock)}
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
                  {showOther ? (
                    <p className="source-muted">
                      {work.authorCreditReview
                        ? "已核对为其他作者作品。"
                        : "作者归属未确认，可打开详情核对。"}
                    </p>
                  ) : (
                    <DownloadWorkButton
                      work={work}
                      owned={stock.kind === "owned"}
                      ready={!!library.rootId}
                      onClick={() => onDownload(work)}
                    />
                  )}
                </article>
              );
            }}
          />
          {!showOther && (
            <FloatingSelection
              visible={active}
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
                    setSelection((old) =>
                      old.filter((key) => !keys.includes(key)),
                    );
                });
              }}
            >
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
            </FloatingSelection>
          )}
        </>
      )}
    </section>
  );
}
