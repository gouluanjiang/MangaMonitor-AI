import { createPortal } from "react-dom";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { LibrarySnapshot } from "./library-types.ts";
import type { PhoneLibrarySnapshot } from "./phone-library-types.ts";
import { emptyPhoneLibrary } from "./phone-library-types.ts";
import {
  createInventoryMatcher,
  inventoryLabel,
} from "./phone-library-model.ts";
import type { WorkReference } from "./booklists.ts";
import type {
  AccountSummary,
  FollowingSnapshot,
  FollowMutation,
  Source,
  SourceAdapter,
  SourcePage,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import {
  accountScope,
  mergeSourceWorks,
  sourceLabel,
  sources,
  sourceWorkKey,
  toWorkReference,
} from "./source-types.ts";
import { SourceError, sourceErrorMessage } from "./source-runtime.ts";
import { CollectionReader } from "./source-collection.ts";
import type { CollectionState } from "./source-collection.ts";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import { getCoverCache, coverErrorMessage } from "./source-cover-cache.ts";
import type { CoverLease, CoverResult } from "./source-cover-cache.ts";
import "./source-workbench.css";

export interface SourceWorkbenchProps {
  adapter: SourceAdapter;
  onDownload?(work: SourceWork): void;
  downloadReady?: boolean;
  downloadBusy?: boolean;
  librarySnapshot?: LibrarySnapshot;
  phoneSnapshot?: PhoneLibrarySnapshot;
  phoneBusy?: boolean;
  phoneReady?: boolean;
  phoneError?: string;
  onMarkPhone?(work: SourceWork): Promise<boolean>;
  onUnmarkPhone?(entryId: string): Promise<boolean>;
  onOpenLibrary?(work: SourceWork): void;
  accounts: AccountSummary[];
  onAccountsChange(updates: AccountSummary[]): void;
  onOpenAccounts(source: Source): void;
  onAddToBooklists(refs: WorkReference[]): Promise<boolean>;
  onWorksChanged(scope: SourceScope, works: SourceWork[]): void;
  view: "favorites" | "search" | "following";
  active: boolean;
  density: 5 | 7 | 9;
  onDensityChange(density: 5 | 7 | 9): void | Promise<unknown>;
  requestedSource?: Source;
  requestedWork?: WorkReference;
  requestKey?: number;
  loadingAccounts?: boolean;
  searchHost?: HTMLElement | null;
}
interface SourceCoverProps {
  adapter: SourceAdapter;
  scope: SourceScope;
  work: SourceWork;
  retryVersion?: number;
  resolveMissing?: boolean;
}
function SourceCover({
  adapter,
  scope,
  work,
  retryVersion = 0,
  resolveMissing = false,
}: SourceCoverProps) {
  const container = useRef<HTMLDivElement>(null);
  const cache = getCoverCache(adapter);
  const identity = JSON.stringify([scope.source, scope.sessionId, work.workId]);
  const [state, setState] = useState<{
    identity: string;
    result: CoverResult | undefined;
    shown: boolean;
    loading: boolean;
  }>(() => ({
    identity,
    result: cache.peek(scope, work.workId),
    shown: true,
    loading: false,
  }));
  const current =
    state.identity === identity
      ? state
      : {
          identity,
          result: cache.peek(scope, work.workId),
          shown: true,
          loading: false,
        };
  useEffect(() => {
    let disposed = false;
    let visible: boolean | null = null;
    let request: CoverLease | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    setState({
      identity,
      result: cache.peek(scope, work.workId),
      shown: true,
      loading: false,
    });
    const load = () => {
      if (disposed || !visible) return;
      if (request) return;
      const cached = cache.peek(scope, work.workId);
      if (
        !cached &&
        !work.coverAvailable &&
        !resolveMissing &&
        retryVersion === 0
      ) {
        setState({ identity, result: undefined, shown: true, loading: false });
        return;
      }
      setState({ identity, result: cached, shown: true, loading: !cached });
      const job = cache.acquire(scope, work.workId, () =>
        adapter.cover(scope, work.workId),
      );
      request = job;
      void job.promise
        .then((value) => {
          if (!disposed && visible && request === job) {
            setState({ identity, result: value, shown: true, loading: false });
            // Queue pressure is temporary, not a permanently failed cover.
            if (value.status === "deferred" && value.reason === "busy")
              retryTimer = setTimeout(load, 500);
          }
        })
        .finally(() => {
          // Keep successful URLs pinned until the image leaves the viewport.
          if (
            !disposed &&
            visible &&
            request === job &&
            cache.peek(scope, work.workId)?.status === "ready"
          )
            return;
          if (request === job) request = null;
          job.release();
        });
    };
    const updateVisibility = (next: boolean) => {
      if (visible === next) return;
      visible = next;
      if (visible) load();
      else {
        request?.release();
        request = null;
        clearTimeout(retryTimer);
        setState((previous) => ({ ...previous, shown: false, loading: false }));
      }
    };
    const observer =
      "IntersectionObserver" in window
        ? new IntersectionObserver(
            (entries) => {
              updateVisibility(entries.some((entry) => entry.isIntersecting));
            },
            { rootMargin: "160px" },
          )
        : null;
    if (container.current) observer?.observe(container.current);
    // Without visibility observation, avoid fetching every mounted cover.
    if (!observer)
      setState({
        identity,
        result: { status: "error", code: "SOURCE_UNAVAILABLE" },
        shown: false,
        loading: false,
      });
    return () => {
      disposed = true;
      request?.release();
      clearTimeout(retryTimer);
      visible = false;
      observer?.disconnect();
    };
  }, [
    adapter,
    scope.source,
    scope.sessionId,
    work.workId,
    work.coverAvailable,
    retryVersion,
    resolveMissing,
  ]);
  return (
    <div
      ref={container}
      className="source-cover"
      data-testid={"source-cover-" + sourceWorkKey(work)}
    >
      {current.shown && current.result?.status === "ready" ? (
        <img
          loading="lazy"
          src={current.result.url}
          alt=""
          onError={() => {
            const result = current.result;
            if (result?.status !== "ready") return;
            cache.decodeFailed(scope, work.workId, result.url);
            setState({
              identity,
              result: cache.peek(scope, work.workId),
              shown: true,
              loading: false,
            });
          }}
        />
      ) : (
        <span
          style={{ overflowWrap: "anywhere" }}
          data-error-code={
            current.result?.status === "error" ? current.result.code : undefined
          }
        >
          {current.result?.status === "error"
            ? coverErrorMessage(current.result.code) +
              "（" +
              current.result.code +
              "）"
            : current.result?.status === "deferred" &&
                current.result.reason === "busy"
              ? "封面正在等待空闲请求…"
              : current.loading
                ? "正在读取封面…"
                : "封面未读取"}
        </span>
      )}
    </div>
  );
}
type Anchor = { key: string; offset: number; scroll?: number };
const scopeKey = (scope: SourceScope | null) =>
  scope ? scope.source + ":" + scope.sessionId : "";
export function SourceWorkbench({
  adapter,
  onDownload,
  downloadReady = false,
  downloadBusy = false,
  librarySnapshot,
  phoneSnapshot,
  phoneBusy = false,
  phoneReady = true,
  phoneError = "",
  onMarkPhone,
  onUnmarkPhone,
  onOpenLibrary,
  accounts,
  onAccountsChange,
  onOpenAccounts,
  onAddToBooklists,
  onWorksChanged,
  view,
  active,
  density,
  onDensityChange,
  requestedSource,
  requestedWork,
  requestKey,
  loadingAccounts = false,
  searchHost,
}: SourceWorkbenchProps) {
  const inventory = useMemo(
    () =>
      createInventoryMatcher(
        librarySnapshot,
        phoneSnapshot ?? emptyPhoneLibrary(),
        phoneReady,
      ),
    [librarySnapshot, phoneSnapshot, phoneReady],
  );
  const [phoneNotice, setPhoneNotice] = useState("");
  useEffect(() => {
    getCoverCache(adapter).retainScopes(
      accounts.flatMap((account) => {
        const scope = accountScope(account);
        return scope ? [scope] : [];
      }),
    );
  }, [adapter, accounts]);
  const [source, setSource] = useState<Source>(requestedSource ?? "JM");
  const [query, setQuery] = useState("");
  const [queryMode, setQueryMode] = useState<"search" | "detail">("search");
  const [folder, setFolder] = useState<string | null>(null);
  const [sort, setSort] = useState("source");
  const currentSort = useRef(sort);
  currentSort.current = sort;
  const [coverEpoch, setCoverEpoch] = useState(0);
  const [autoPaused, setAutoPaused] = useState(false);
  const gridRef = useRef<SourceGridHandle>(null);
  const sentinel = useRef<HTMLDivElement>(null);
  const collector = useRef<CollectionReader | null>(null);
  const collectionReadAll = useRef(false);
  const collectionNeedsVerification = useRef(false);
  const selectedMetadata = useRef(new Map<string, SourceWork>());
  const [collectionState, setCollectionState] = useState<CollectionState>({
    snapshot: null,
    displaySnapshot: null,
    completeSnapshot: null,
    phase: "idle",
    freshness: "none",
    error: null,
    cacheWarning: "",
  });
  const [items, setItems] = useState<SourceWork[]>([]);
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const [rangeTruncated, setRangeTruncated] = useState(false);
  const [pageInfo, setPageInfo] = useState<SourcePage | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selectionMode, setSelectionMode] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const [organizing, setOrganizing] = useState(false);
  const [detailRef, setDetailRef] = useState<WorkReference | null>(null);
  const [detail, setDetail] = useState<SourceWork | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [following, setFollowing] = useState<FollowingSnapshot | null>(null);
  const [followingTab, setFollowingTab] = useState<"authors" | "works">(
    "authors",
  );
  const [followingBusy, setFollowingBusy] = useState(false);
  const [followingError, setFollowingError] = useState("");
  const [pendingFollow, setPendingFollow] = useState<Omit<
    FollowMutation,
    "expectedRevision"
  > | null>(null);
  const [favoriteBusy, setFavoriteBusy] = useState(false);
  const [authorSearch, setAuthorSearch] = useState(false);
  const host = useRef<HTMLElement>(null);
  const listRequest = useRef(0);
  const detailRequest = useRef(0);
  const followingRequest = useRef(0);
  const favoriteLock = useRef(false);
  const followingLock = useRef(false);
  const organizeLock = useRef(false);
  const savedAnchor = useRef<Anchor | null>(null);
  const pendingAnchor = useRef<Anchor | null>(null);
  const autoContext = useRef("");
  const handledWorkRequest = useRef("");
  const lastRead = useRef({
    kind: "favorites" as "favorites" | "search",
    query: "",
    folderId: null as string | null,
    page: 1,
    append: false,
  });
  const lastListQuery = useRef<{
    kind: "favorites" | "search";
    query: string;
    folderId: string | null;
  }>({
    kind: "favorites",
    query: "",
    folderId: null,
  });
  const account = accounts.find((item) => item.source === source);
  const scope = accountScope(account);
  const currentScope = useRef(scope);
  currentScope.current = scope;
  const notifyWorks = useRef(onWorksChanged);
  notifyWorks.current = onWorksChanged;
  const accountUpdate = useRef(onAccountsChange);
  accountUpdate.current = onAccountsChange;
  const scopeId = scopeKey(scope);
  const searching = view === "search" || authorSearch;
  const stillCurrent = (expected: SourceScope) =>
    scopeKey(currentScope.current) === scopeKey(expected);
  function main() {
    return host.current?.closest("main") ?? null;
  }
  function capture(preferred?: string): Anchor | null {
    return gridRef.current?.capture(preferred) ?? null;
  }
  function restore(anchor: Anchor | null) {
    gridRef.current?.restore(anchor);
  }
  useLayoutEffect(() => {
    if (pendingAnchor.current && !detailRef) {
      restore(pendingAnchor.current);
      pendingAnchor.current = null;
    }
  }, [density, detailRef, sort, items]);
  function clearSelection() {
    if (selection.length) setNotice("范围已改变，临时选择已清空。");
    setSelection([]);
    selectedMetadata.current.clear();
  }
  function changeSource(next: Source) {
    if (next === source) return;
    collectionReadAll.current = false;
    listRequest.current += 1;
    detailRequest.current += 1;
    followingRequest.current += 1;
    setSort("source");
    setSource(next);
    setQuery("");
    setFolder(null);
    setAuthorSearch(false);
    setSelection([]);
    setSelectionMode(false);
    setNotice(selection.length ? "来源已改变，临时选择已清空。" : "");
  }
  useEffect(() => {
    currentScope.current = scope;
    return () => {
      listRequest.current += 1;
      detailRequest.current += 1;
      followingRequest.current += 1;
      currentScope.current = null;
    };
  }, []);
  useEffect(() => {
    const target = requestedWork?.source ?? requestedSource;
    if (target) changeSource(target);
  }, [requestedSource, requestedWork?.source, requestKey]);
  useEffect(() => {
    listRequest.current += 1;
    detailRequest.current += 1;
    followingRequest.current += 1;
    collectionReadAll.current = false;
    favoriteLock.current = false;
    followingLock.current = false;
    selectedMetadata.current.clear();
    setItems([]);
    setRangeTruncated(false);
    setPageInfo(null);
    setDetailRef(null);
    setDetail(null);
    setFollowing(null);
    setFollowingError("");
    setPendingFollow(null);
    setSelection([]);
    setSelectionMode(false);
    setLoading(false);
    setDetailLoading(false);
    setFavoriteBusy(false);
    setFollowingBusy(false);
    setError("");
    setDetailError("");
    setAuthorSearch(false);
    setQuery("");
    setFolder(null);
    autoContext.current = "";
  }, [scopeId, view]);
  useEffect(() => {
    if (!active || !scope || loadingAccounts) return;
    const context = scopeId + "|" + view + "|" + (folder ?? "");
    if (context === autoContext.current) return;
    autoContext.current = context;
    void readFollowing();
  }, [active, scopeId, view, folder, loadingAccounts]);
  useEffect(() => {
    if (!active || !scope || !requestedWork || requestedWork.source !== source)
      return;
    const key =
      String(requestKey ?? 0) +
      "|" +
      scopeId +
      "|" +
      sourceWorkKey(requestedWork);
    if (handledWorkRequest.current === key) return;
    handledWorkRequest.current = key;
    void openDetail(requestedWork);
  }, [
    active,
    scopeId,
    requestKey,
    requestedWork?.source,
    requestedWork?.workId,
  ]);
  useEffect(() => {
    if (!scope || view !== "favorites" || authorSearch) return;
    const reader = new CollectionReader(adapter, scope, folder, false);
    collector.current = reader;
    collectionReadAll.current = false;
    let lastDisplay: CollectionState["displaySnapshot"] = null;
    const published = new Map<string, SourceWork>();
    const unsubscribe = reader.subscribe((state) => {
      if (collector.current !== reader || !stillCurrent(scope)) return;
      setCollectionState(state);
      const displayed = state.displaySnapshot;
      if (displayed && displayed !== lastDisplay) {
        const completedReverse =
          currentSort.current === "source-reverse" &&
          displayed.complete &&
          !lastDisplay?.complete;
        if (completedReverse) {
          pendingAnchor.current = null;
          savedAnchor.current = null;
          main()?.scrollTo(0, 0);
        } else if (lastDisplay) pendingAnchor.current = capture();
        lastDisplay = displayed;
        const displayedItems =
          source === "Pica"
            ? mergeSourceWorks([], displayed.items)
            : displayed.items;
        setItems(displayedItems);
        setPageInfo(displayed);
        setRangeTruncated(false);
        const updated = displayedItems.filter(
          (work) => published.get(work.workId) !== work,
        );
        for (const work of updated) published.set(work.workId, work);
        const currentIds = new Set(displayedItems.map((work) => work.workId));
        for (const key of published.keys())
          if (!currentIds.has(key)) published.delete(key);
        if (updated.length) notifyWorks.current(scope, updated);
      }
      if (state.error) void reconcileSessionFailure(state.error, scope);
    });
    setItems([]);
    setPageInfo(null);
    setAutoPaused(false);
    if (active && !loadingAccounts) resumeCollection();
    return () => {
      reader.pause();
      unsubscribe();
      reader.dispose();
      if (collector.current === reader) collector.current = null;
    };
  }, [adapter, scopeId, view, folder, authorSearch]);
  useEffect(() => {
    if (!collector.current || view !== "favorites" || authorSearch) return;
    if (!active || loadingAccounts) {
      collectionNeedsVerification.current = true;
      collector.current.pause();
    } else if (autoPaused) collector.current.pause();
    else resumeCollection();
  }, [
    active,
    loadingAccounts,
    autoPaused,
    scopeId,
    view,
    folder,
    authorSearch,
  ]);
  function resumeCollection(retry = false) {
    const reader = collector.current;
    if (!reader || (reader.state.phase === "error" && !retry)) return;
    const verify = collectionNeedsVerification.current;
    if (verify) {
      collectionNeedsVerification.current = false;
      void reader.revalidate();
    }
    // Keep explicit full-reading intent through pauses and errors. Ordinary
    // entry and an ordinary first-page retry retain viewport-driven reading.
    if (collectionReadAll.current) void reader.readAll();
    else if (retry) void reader.retry();
    else if (!verify) void reader.resume();
  }
  useEffect(() => {
    if (
      !active ||
      autoPaused ||
      view !== "favorites" ||
      authorSearch ||
      detailRef ||
      query.trim() ||
      collectionState.phase !== "ready" ||
      !sentinel.current
    )
      return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting))
          void collector.current?.loadNext();
      },
      { root: main(), rootMargin: "200px" },
    );
    observer.observe(sentinel.current);
    return () => observer.disconnect();
  }, [
    active,
    autoPaused,
    view,
    authorSearch,
    detailRef,
    query,
    collectionState.phase,
    collectionState.snapshot?.page,
    density,
  ]);
  function changeSort(value: string) {
    const picaTimeSwitch =
      source === "Pica" &&
      view === "favorites" &&
      value !== sort &&
      ["source", "source-reverse"].includes(value) &&
      ["source", "source-reverse"].includes(sort);
    const directionChanged =
      view === "favorites" &&
      (value === "source-reverse") !== (sort === "source-reverse");
    pendingAnchor.current = directionChanged ? null : capture();
    if (directionChanged) {
      if (source === "Pica") {
        clearSelection();
      }
      savedAnchor.current = null;
      main()?.scrollTo(0, 0);
    }
    collectionReadAll.current =
      picaTimeSwitch || (view === "favorites" && value === "source-reverse");
    if (!collectionReadAll.current) collector.current?.stopReadAll();
    setSort(value);
    if (collectionReadAll.current) {
      setAutoPaused(false);
      resumeCollection();
    }
  }
  function retryCovers() {
    if (scope) getCoverCache(adapter).retryFailures(scope);
    setCoverEpoch((value) => value + 1);
  }
  function refreshCollection() {
    if (source === "Pica") {
      collectionReadAll.current = false;
      collector.current?.stopReadAll();
    }
    retryCovers();
    clearSelection();
    setAutoPaused(false);
    collectionNeedsVerification.current = false;
    void collector.current?.refresh();
  }
  async function reconcileSessionFailure(
    cause: unknown,
    captured: SourceScope,
  ) {
    if (
      !(cause instanceof SourceError) ||
      !["AUTH_REQUIRED", "SESSION_EXPIRED", "CREDENTIAL_CHANGED"].includes(
        cause.code,
      ) ||
      !stillCurrent(captured)
    )
      return;
    const previous = accounts.find(
      (item) =>
        item.source === captured.source &&
        item.sessionId === captured.sessionId,
    );
    try {
      const updates = await adapter.accounts(false);
      if (!stillCurrent(captured)) return;
      const update = updates.find((item) => item.source === captured.source);
      if (update) accountUpdate.current([update]);
    } catch {
      if (previous && stillCurrent(captured))
        accountUpdate.current([
          {
            ...previous,
            sessionId: null,
            state:
              cause.code === "CREDENTIAL_CHANGED" ? "unavailable" : "expired",
            errorCode: cause.code,
          },
        ]);
    }
  }

  async function readList(
    kind: "favorites" | "search",
    value: string,
    folderId: string | null,
    page = 1,
    append = false,
  ) {
    const captured = currentScope.current;
    if (!captured) return;
    const request = ++listRequest.current;
    const sameQuery =
      lastListQuery.current.kind === kind &&
      lastListQuery.current.query === value &&
      lastListQuery.current.folderId === folderId;
    lastListQuery.current = { kind, query: value, folderId };
    lastRead.current = { kind, query: value, folderId, page, append };
    setLoading(true);
    setError("");
    if (!append && !sameQuery) {
      setItems([]);
      setPageInfo(null);
    }
    try {
      const result = await adapter.query(captured, {
        kind,
        query: value,
        folderId,
        page,
      });
      if (!stillCurrent(captured) || request !== listRequest.current) return;
      const merged = mergeSourceWorks(
        append ? itemsRef.current : [],
        result.items,
      );
      setRangeTruncated(merged.length > 1000);
      setItems(merged.slice(0, 1000));
      setPageInfo((previous) => ({
        ...result,
        items: [],
        folders:
          append && !result.folders.length && previous
            ? previous.folders
            : result.folders,
      }));
      notifyWorks.current(captured, result.items.slice(0, 1000));
      if (!append) main()?.scrollTo(0, 0);
    } catch (cause) {
      if (stillCurrent(captured) && request === listRequest.current) {
        setError(sourceErrorMessage(cause));
        void reconcileSessionFailure(cause, captured);
      }
    } finally {
      if (stillCurrent(captured) && request === listRequest.current)
        setLoading(false);
    }
  }
  async function readFollowing() {
    const captured = currentScope.current;
    if (!captured || followingLock.current) return;
    const request = ++followingRequest.current;
    followingLock.current = true;
    setFollowingBusy(true);
    setFollowingError("");
    try {
      const result = await adapter.following(captured);
      if (stillCurrent(captured) && request === followingRequest.current)
        setFollowing(result);
    } catch (cause) {
      if (stillCurrent(captured) && request === followingRequest.current) {
        setFollowingError(sourceErrorMessage(cause));
        void reconcileSessionFailure(cause, captured);
      }
    } finally {
      if (stillCurrent(captured) && request === followingRequest.current) {
        followingLock.current = false;
        setFollowingBusy(false);
      }
    }
  }
  async function refreshAccounts() {
    setError("");
    try {
      accountUpdate.current(await adapter.accounts(true));
    } catch (cause) {
      setError(sourceErrorMessage(cause));
    }
  }
  function changeQuery(value: string) {
    setQuery(value);
    clearSelection();
    if (searching) {
      listRequest.current += 1;
      setLoading(false);
      setItems([]);
      setPageInfo(null);
      setError("");
    }
  }
  function submitSearch() {
    if (!scope || !query.trim() || loading) return;
    clearSelection();
    if (queryMode === "detail")
      void openDetail({ source, workId: query.trim() });
    else void readList("search", query.trim(), null);
  }
  async function openDetail(reference: WorkReference) {
    const captured = currentScope.current;
    if (!captured || reference.source !== captured.source) return;
    if (!detailRef) savedAnchor.current = capture(sourceWorkKey(reference));
    const request = ++detailRequest.current;
    setDetailRef(reference);
    setDetail(null);
    setDetailLoading(true);
    setDetailError("");
    setExpanded(false);
    main()?.scrollTo(0, 0);
    try {
      const result = await adapter.query(captured, {
        kind: "detail",
        query: reference.workId,
        folderId: null,
        page: 1,
      });
      if (!stillCurrent(captured) || request !== detailRequest.current) return;
      const found = result.items[0];
      if (!found) {
        setDetailError("没有取得这部作品的详情，请检查来源编号或链接。");
        return;
      }
      setDetail(found);
      setDetailRef(toWorkReference(found));
      notifyWorks.current(captured, [found]);
    } catch (cause) {
      if (stillCurrent(captured) && request === detailRequest.current) {
        setDetailError(sourceErrorMessage(cause));
        void reconcileSessionFailure(cause, captured);
      }
    } finally {
      if (stillCurrent(captured) && request === detailRequest.current)
        setDetailLoading(false);
    }
  }
  function back() {
    detailRequest.current += 1;
    setDetailRef(null);
    setDetail(null);
    pendingAnchor.current = savedAnchor.current;
  }
  function updateWork(work: SourceWork) {
    setDetail(work);
    setItems((previous) =>
      previous.map((item) =>
        sourceWorkKey(item) === sourceWorkKey(work) ? work : item,
      ),
    );
    if (currentScope.current) notifyWorks.current(currentScope.current, [work]);
  }
  async function changeFavorite() {
    const captured = currentScope.current;
    const work = detail;
    if (!captured || !work || work.favorite === null || favoriteLock.current)
      return;
    favoriteLock.current = true;
    setFavoriteBusy(true);
    setDetailError("");
    try {
      const result = await adapter.favorite(
        captured,
        work.workId,
        !work.favorite,
      );
      if (!stillCurrent(captured) || detailRequest.current !== detailEpoch)
        return;
      updateWork({ ...work, favorite: result.favorite });
      setNotice(
        result.favorite
          ? "已读回确认网站收藏。"
          : "已读回确认取消网站收藏。本地书单保留。",
      );
    } catch (cause) {
      if (stillCurrent(captured) && detailRequest.current === detailEpoch) {
        updateWork({ ...work, favorite: null });
        setDetailError(sourceErrorMessage(cause));
        void reconcileSessionFailure(cause, captured);
      }
    } finally {
      if (stillCurrent(captured)) {
        favoriteLock.current = false;
        setFavoriteBusy(false);
      }
    }
  }
  const detailEpoch = detailRequest.current;
  async function changeFollow(
    mutation: Omit<FollowMutation, "expectedRevision">,
  ) {
    const captured = currentScope.current;
    if (!captured || !following || followingLock.current) return;
    const request = ++followingRequest.current;
    followingLock.current = true;
    setFollowingBusy(true);
    setFollowingError("");
    setPendingFollow(mutation);
    try {
      const result = await adapter.follow(captured, {
        ...mutation,
        expectedRevision: following.revision,
      });
      if (!stillCurrent(captured) || request !== followingRequest.current)
        return;
      setFollowing(result);
      setPendingFollow(null);
      setNotice(
        mutation.desired
          ? "已加入本机关注。"
          : "已取消本机关注，书单、网站收藏和文件保留。",
      );
    } catch (cause) {
      if (stillCurrent(captured) && request === followingRequest.current) {
        setFollowingError(sourceErrorMessage(cause));
        void reconcileSessionFailure(cause, captured);
      }
    } finally {
      if (stillCurrent(captured) && request === followingRequest.current) {
        followingLock.current = false;
        setFollowingBusy(false);
      }
    }
  }
  async function organize(works: SourceWork[]) {
    if (!works.length || organizeLock.current) return;
    const captured = currentScope.current;
    organizeLock.current = true;
    setOrganizing(true);
    try {
      const saved = await onAddToBooklists(works.map(toWorkReference));
      if (captured && stillCurrent(captured))
        setNotice(saved ? "书单已保存。" : "书单操作未完成，选择已保留。");
    } catch {
      setNotice("书单未能保存，选择已保留。");
    } finally {
      organizeLock.current = false;
      setOrganizing(false);
    }
  }
  const followedWorks: SourceWork[] = (following?.works ?? []).map((work) => ({
    source,
    workId: work.workId,
    title: work.title,
    authors: [],
    description: null,
    tags: [],
    favorite: null,
    chapterCount: null,
    pageCount: null,
    coverAvailable: false,
  }));
  const browsingWorks =
    view === "following" && !authorSearch ? followedWorks : items;
  const filtered = browsingWorks.filter(
    (work) =>
      searching ||
      (work.title + " " + work.authors.join(" "))
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  const completeIndex = collectionState.snapshot?.complete ?? false;
  const collectionRecords = collectionState.snapshot?.items.length ?? 0;
  const collectionWorks = new Set(
    collectionState.snapshot?.items.map(sourceWorkKey) ?? [],
  ).size;
  const collectionDuplicates = collectionRecords - collectionWorks;
  const reversePreparing =
    view === "favorites" && sort === "source-reverse" && !completeIndex;
  const visible =
    sort === "title" || sort === "title-desc"
      ? [...filtered].sort(
          (a, b) =>
            (a.title.localeCompare(b.title, "zh-CN") ||
              sourceWorkKey(a).localeCompare(sourceWorkKey(b))) *
            (sort === "title-desc" ? -1 : 1),
        )
      : sort === "source-reverse" && !reversePreparing
        ? [...filtered].reverse()
        : filtered;
  const selectionKeys = new Set(selection);
  for (const key of selectedMetadata.current.keys())
    if (!selectionKeys.has(key)) selectedMetadata.current.delete(key);
  for (const work of browsingWorks)
    if (selectionKeys.has(sourceWorkKey(work)))
      selectedMetadata.current.set(sourceWorkKey(work), work);
  const selectedWorks = selection
    .map((key) => selectedMetadata.current.get(key))
    .filter((work): work is SourceWork => Boolean(work));
  const connected = Boolean(scope) && adapter.available;
  const totalKnown = pageInfo?.total !== null && pageInfo?.total !== undefined;
  const complete =
    view === "favorites"
      ? Boolean(collectionState.snapshot?.complete)
      : Boolean(
          !rangeTruncated &&
          pageInfo &&
          pageInfo.hasMore === false &&
          (!totalKnown || items.length >= pageInfo.total!),
        );
  const searchControl = (
    <form
      className="source-search"
      data-testid="source-search-control"
      onSubmit={(event) => {
        event.preventDefault();
        if (searching) submitSearch();
      }}
    >
      <span>
        {sourceLabel(source)}
        {searching ? " 来源" : view === "following" ? " 关注" : " 收藏"}
      </span>
      <input
        value={query}
        onChange={(event) => changeQuery(event.target.value)}
        data-testid="source-search-input"
        aria-label={
          searching
            ? "搜索当前来源作品或输入单个编号链接"
            : view === "favorites" && completeIndex
              ? "搜索全部收藏的作品或作者"
              : "筛选当前已读取范围"
        }
        placeholder={
          searching
            ? "搜索作品、作者或输入编号…"
            : view === "favorites" && completeIndex
              ? "搜索全部收藏的作品或作者…"
              : "筛选已读取的作品或作者…"
        }
      />
      {query && (
        <button
          type="button"
          className="text-button"
          aria-label="清空来源搜索"
          onClick={() => changeQuery("")}
        >
          ×
        </button>
      )}
      {searching && (
        <button
          type="submit"
          className="text-button"
          disabled={!connected || loading || !query.trim()}
          data-testid="source-search-submit"
        >
          {loading ? "读取中…" : "搜索"}
        </button>
      )}
    </form>
  );
  function densityControl() {
    return (
      <div className="density-control" role="group" aria-label="来源封面密度">
        <span>封面密度</span>
        {([5, 7, 9] as const).map((value) => (
          <button
            type="button"
            key={value}
            aria-label={"来源每行 " + value + " 部"}
            aria-pressed={density === value}
            onClick={() => {
              if (value === density) return;
              pendingAnchor.current = capture();
              void Promise.resolve(onDensityChange(value)).catch(() => {
                pendingAnchor.current = null;
                setNotice("封面密度未能保存。");
              });
            }}
          >
            {value}
          </button>
        ))}
      </div>
    );
  }
  function followingFeedback() {
    return followingError || pendingFollow ? (
      <div className="source-notice">
        {followingError ? (
          <p role="alert">{followingError}</p>
        ) : (
          <p role="status">已重新读取本机关注。请再次确认保存这次操作。</p>
        )}
        <button
          type="button"
          className="text-button"
          disabled={followingBusy}
          data-testid="source-following-reload"
          onClick={() => void readFollowing()}
        >
          重新读取本机关注
        </button>
        {pendingFollow && (
          <button
            type="button"
            className="text-button"
            disabled={followingBusy || !following}
            data-testid="source-following-retry"
            onClick={() => void changeFollow(pendingFollow)}
          >
            重试这次关注操作
          </button>
        )}
      </div>
    ) : null;
  }
  function grid(works: SourceWork[]) {
    return (
      <VirtualSourceGrid<SourceWork>
        ref={gridRef}
        items={works}
        density={density}
        itemKey={sourceWorkKey}
        renderItem={(work) => {
          const key = sourceWorkKey(work);
          return (
            <article
              key={key}
              data-source-work-key={key}
              className={
                "source-card" + (selectionKeys.has(key) ? " is-selected" : "")
              }
              data-testid={"source-card-" + key}
            >
              <div className="source-card-cover">
                <button
                  type="button"
                  className="source-cover-button"
                  data-testid={"source-open-" + key}
                  onClick={() => void openDetail(toWorkReference(work))}
                  aria-label={"查看《" + work.title + "》详情"}
                >
                  {scope && (
                    <SourceCover
                      adapter={adapter}
                      scope={scope}
                      work={work}
                      retryVersion={coverEpoch}
                      resolveMissing={view === "following"}
                    />
                  )}
                </button>
                {selectionMode && (
                  <input
                    type="checkbox"
                    aria-label={"选择 " + work.title}
                    checked={selection.includes(key)}
                    data-testid={"source-select-" + key}
                    onChange={(event) =>
                      setSelection((previous) =>
                        event.target.checked
                          ? [...previous, key]
                          : previous.filter((item) => item !== key),
                      )
                    }
                  />
                )}
              </div>
              <h3>
                <button
                  type="button"
                  onClick={() => void openDetail(toWorkReference(work))}
                >
                  {work.title}
                </button>
              </h3>
              <p>
                {work.authors.length
                  ? work.authors.join("、")
                  : "作者资料未取得"}
              </p>
              <p className="source-card-state">
                {sourceLabel(work.source)} · {inventoryLabel(inventory(work))}
              </p>
            </article>
          );
        }}
      />
    );
  }
  const body = detailRef ? (
    <div className="source-detail" data-testid="source-detail">
      <button
        type="button"
        className="text-button"
        data-testid="source-detail-back"
        onClick={back}
      >
        ← 返回列表
      </button>
      {detailLoading && <p role="status">正在读取作品详情…</p>}
      {(phoneNotice || phoneError) && (
        <p role="status" className="source-notice">
          {phoneError || phoneNotice}
        </p>
      )}
      {detailError && (
        <div className="source-notice">
          <p role="alert">{detailError}</p>
          <button
            type="button"
            className="text-button"
            disabled={detailLoading}
            data-testid="source-detail-reload"
            onClick={() => void openDetail(detailRef)}
          >
            重新读取当前作品
          </button>
        </div>
      )}
      {detail && scope && (
        <>
          <div className="source-detail-main">
            <SourceCover
              adapter={adapter}
              scope={scope}
              work={detail}
              retryVersion={coverEpoch}
            />
            <div className="source-detail-info">
              <p className="source-muted">
                {sourceLabel(source)} · 来源作品详情
              </p>
              <h1>{detail.title}</h1>
              <div className="source-detail-authors">
                {detail.authors.length ? (
                  detail.authors.map((author) => (
                    <div key={author}>
                      <span>{author}</span>
                      <button
                        type="button"
                        className="text-button"
                        disabled={!following || followingBusy}
                        data-testid={"source-follow-author-" + author}
                        onClick={() =>
                          void changeFollow({
                            kind: "author",
                            value: author,
                            desired: !following?.authors.includes(author),
                          })
                        }
                      >
                        {following?.authors.includes(author)
                          ? "取消作者关注"
                          : "关注作者"}
                      </button>
                    </div>
                  ))
                ) : (
                  <p>作者资料未取得</p>
                )}
              </div>
              <div className="source-tags">
                {detail.tags.map((tag) => (
                  <span key={tag}>{tag}</span>
                ))}
              </div>
              <dl className="source-facts">
                <div>
                  <dt>章节</dt>
                  <dd>
                    {detail.chapterCount === null
                      ? "未知"
                      : detail.chapterCount}
                  </dd>
                </div>
                <div>
                  <dt>页数</dt>
                  <dd>
                    {detail.pageCount === null ? "未知" : detail.pageCount}
                  </dd>
                </div>
                <div>
                  <dt>本地库存</dt>
                  <dd data-testid="source-detail-stock">
                    {inventoryLabel(inventory(detail))}
                  </dd>
                </div>
              </dl>
              <div className="source-actions">
                {onOpenLibrary && (
                  <button
                    type="button"
                    className="button secondary"
                    data-testid="source-open-library"
                    onClick={() => onOpenLibrary(detail)}
                  >
                    核对电脑文件
                  </button>
                )}
                {onMarkPhone && (
                  <button
                    type="button"
                    className="button secondary"
                    data-testid="source-phone-mark"
                    disabled={phoneBusy}
                    onClick={() =>
                      void onMarkPhone(detail).then((saved) =>
                        setPhoneNotice(
                          saved
                            ? "已标记手机已入库，电脑文件保留。"
                            : "手机标记未完成，请重试。",
                        ),
                      )
                    }
                  >
                    标记手机已入库
                  </button>
                )}
                {onUnmarkPhone &&
                  phoneSnapshot?.manualEntries
                    .filter(
                      (entry) =>
                        entry.reference?.source === detail.source &&
                        entry.reference.workId === detail.workId,
                    )
                    .map((entry) => (
                      <button
                        key={entry.id}
                        className="text-button"
                        data-testid={"source-phone-unmark-" + entry.id}
                        disabled={phoneBusy}
                        onClick={() =>
                          void onUnmarkPhone(entry.id).then((saved) =>
                            setPhoneNotice(
                              saved
                                ? "已撤销手动标记；导入名单仍保留。"
                                : "撤销未完成，请重试。",
                            ),
                          )
                        }
                      >
                        撤销手动标记
                      </button>
                    ))}
                <button
                  type="button"
                  className="button primary"
                  disabled={!onDownload || !downloadReady || downloadBusy}
                  data-testid="source-download"
                  onClick={() => onDownload?.(detail)}
                >
                  {downloadBusy ? "正在准备下载…" : "下载到电脑"}
                </button>
                <button
                  type="button"
                  className="button secondary"
                  disabled={organizing}
                  data-testid="source-detail-booklist"
                  onClick={() => void organize([detail])}
                >
                  加入书单
                </button>
                <button
                  type="button"
                  className="text-button"
                  data-testid="source-favorite"
                  disabled={favoriteBusy || detail.favorite === null}
                  aria-pressed={
                    detail.favorite === null ? undefined : detail.favorite
                  }
                  onClick={() => void changeFavorite()}
                >
                  {favoriteBusy
                    ? "正在确认网站状态…"
                    : detail.favorite === null
                      ? "收藏状态待核对"
                      : detail.favorite
                        ? "取消网站收藏"
                        : "收藏到网站"}
                </button>
                <button
                  type="button"
                  className="text-button"
                  disabled={!following || followingBusy}
                  data-testid="source-follow-work"
                  onClick={() =>
                    void changeFollow({
                      kind: "work",
                      value: detail.workId,
                      desired: !following?.works.some(
                        (work) => work.workId === detail.workId,
                      ),
                    })
                  }
                >
                  {following?.works.some(
                    (work) => work.workId === detail.workId,
                  )
                    ? "取消作品关注"
                    : "关注作品"}
                </button>
              </div>
              <p className="source-muted">
                网站收藏、本机关注与本地书单分别保存。JM
                下载经单独确认后加入电脑队列。
                <button
                  type="button"
                  className="text-button"
                  data-testid="source-detail-cover-retry"
                  onClick={retryCovers}
                >
                  重试封面
                </button>
              </p>
              {followingFeedback()}
              <section className="source-description">
                <h2>简介</h2>
                <p className={expanded ? "" : "is-collapsed"}>
                  {detail.description ?? "来源未提供简介。"}
                </p>
                {detail.description && detail.description.length > 180 && (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setExpanded(!expanded)}
                  >
                    {expanded ? "收起简介" : "展开简介"}
                  </button>
                )}
              </section>
              <section>
                <h2>来源信息</h2>
                <p>
                  {sourceLabel(source)} · {detail.workId}
                </p>
                <p className="source-muted">
                  当前仅取得作品元数据，章节目录与本地文件尚未接入。
                </p>
              </section>
            </div>
          </div>
        </>
      )}
    </div>
  ) : (
    <>
      <div className="page-heading source-heading">
        <div>
          <h1>
            {authorSearch
              ? "作者作品"
              : view === "favorites"
                ? "在线收藏"
                : view === "following"
                  ? "关注"
                  : "来源搜索"}
          </h1>
          <p className="source-muted">
            {sourceLabel(source)} ·{" "}
            {loadingAccounts
              ? "正在恢复账号…"
              : account?.state === "connected"
                ? (account.displayName ?? account.accountId)
                : "尚未连接账号"}
          </p>
        </div>
        {!searchHost && searchControl}
      </div>
      <div className="source-tabs" role="group" aria-label="来源">
        {sources.map((item) => (
          <button
            type="button"
            key={item}
            className={source === item ? "active" : ""}
            aria-pressed={source === item}
            data-testid={"source-tab-" + item}
            onClick={() => changeSource(item)}
          >
            {sourceLabel(item)}
          </button>
        ))}
      </div>
      {!connected ? (
        <div className="source-empty" data-testid="source-account-required">
          <h2>
            {loadingAccounts
              ? "正在恢复账号会话"
              : !adapter.available
                ? "请使用桌面应用"
                : account?.state === "expired"
                  ? "账号需要重新登录"
                  : "连接当前来源账号"}
          </h2>
          <p>
            {loadingAccounts
              ? "读取完成后可继续操作。"
              : !adapter.available
                ? "浏览器预览不会连接真实来源，也不会显示模拟的登录成功。"
                : "未连接不代表空收藏。连接后可读取该账号的来源数据。"}
          </p>
          <button
            type="button"
            className="button primary"
            disabled={loadingAccounts}
            onClick={() => onOpenAccounts(source)}
          >
            前往账号设置
          </button>
          <button
            type="button"
            className="text-button"
            disabled={loadingAccounts}
            onClick={() => void refreshAccounts()}
          >
            重新读取账号状态
          </button>
        </div>
      ) : (
        <>
          {view === "following" && !authorSearch && (
            <div className="source-following-tabs source-tabs">
              <button
                type="button"
                aria-pressed={followingTab === "authors"}
                onClick={() => {
                  setFollowingTab("authors");
                  clearSelection();
                }}
              >
                作者关注 {following?.authors.length ?? "—"}
              </button>
              <button
                type="button"
                aria-pressed={followingTab === "works"}
                onClick={() => {
                  setFollowingTab("works");
                  clearSelection();
                }}
              >
                作品关注 {following?.works.length ?? "—"}
              </button>
              <button
                type="button"
                className="text-button"
                disabled={followingBusy}
                onClick={() => void readFollowing()}
              >
                重新读取本机关注
              </button>
            </div>
          )}
          <div className="source-toolbar">
            <div className="source-toolbar-leading">
              <button
                type="button"
                className="text-button"
                data-testid="source-cover-retry"
                onClick={retryCovers}
              >
                重试封面
              </button>
              {view === "favorites" && source === "JM" && (
                <label>
                  网站收藏夹{" "}
                  <select
                    data-testid="source-folder"
                    value={folder ?? ""}
                    disabled={loading}
                    onChange={(event) => {
                      collectionReadAll.current = false;
                      collector.current?.stopReadAll();
                      setSort("source");
                      setFolder(event.target.value || null);
                      clearSelection();
                      pendingAnchor.current = null;
                      savedAnchor.current = null;
                      main()?.scrollTo(0, 0);
                    }}
                  >
                    <option value="">全部收藏</option>
                    {pageInfo?.folders.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                        {item.count === null ? "" : " · " + item.count}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {searching && (
                <label>
                  查询方式{" "}
                  <select
                    value={queryMode}
                    onChange={(event) => {
                      setQueryMode(event.target.value as "search" | "detail");
                      clearSelection();
                      listRequest.current += 1;
                      setLoading(false);
                      setItems([]);
                      setPageInfo(null);
                    }}
                    data-testid="source-query-mode"
                  >
                    <option value="search">关键词搜索</option>
                    <option value="detail">单个编号或链接</option>
                  </select>
                </label>
              )}
              {view === "favorites" && (
                <button
                  type="button"
                  className="button secondary"
                  disabled={loading}
                  data-testid="source-refresh"
                  onClick={() => {
                    refreshCollection();
                  }}
                >
                  刷新收藏
                </button>
              )}
              {authorSearch && (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => {
                    setAuthorSearch(false);
                    setQuery("");
                    clearSelection();
                  }}
                >
                  返回本机关注
                </button>
              )}
            </div>
            {!(
              view === "following" &&
              followingTab === "authors" &&
              !authorSearch
            ) && densityControl()}
          </div>
          {notice && (
            <p
              role="status"
              className="source-notice"
              data-testid="source-notice"
            >
              {notice}
            </p>
          )}
          {error && (
            <div className="source-notice">
              <p role="alert">{error}</p>
              <button
                type="button"
                className="text-button"
                disabled={loading}
                data-testid="source-retry"
                onClick={() =>
                  void readList(
                    lastRead.current.kind,
                    lastRead.current.query,
                    lastRead.current.folderId,
                    lastRead.current.page,
                    lastRead.current.append,
                  )
                }
              >
                重试读取
              </button>
              <button
                type="button"
                className="text-button"
                onClick={() => onOpenAccounts(source)}
              >
                账号设置
              </button>
            </div>
          )}
          {view === "following" && followingFeedback()}
          {view === "following" &&
          !authorSearch &&
          followingTab === "authors" ? (
            <>
              <p className="source-muted">
                这里的关注保存在本机当前账号下。搜索作者只读取一次来源结果，不启动持续检查。
              </p>
              <div className="source-authors" data-testid="source-authors">
                <div className="source-author-head">
                  <span>作者</span>
                  <span>来源</span>
                  <span>检查状态</span>
                  <span>操作</span>
                </div>
                {(following?.authors ?? [])
                  .filter((name) => name.includes(query.trim()))
                  .map((author) => (
                    <div className="source-author-row" key={author}>
                      <strong>{author}</strong>
                      <span>{sourceLabel(source)}</span>
                      <span className="source-muted">尚未自动检查</span>
                      <div className="source-actions">
                        <button
                          type="button"
                          className="button secondary"
                          onClick={() => {
                            setAuthorSearch(true);
                            setQuery(author);
                            setQueryMode("search");
                            clearSelection();
                            void readList("search", author, null);
                          }}
                        >
                          搜索该作者
                        </button>
                        <button
                          type="button"
                          className="text-button"
                          disabled={followingBusy}
                          onClick={() =>
                            void changeFollow({
                              kind: "author",
                              value: author,
                              desired: false,
                            })
                          }
                        >
                          取消关注
                        </button>
                      </div>
                    </div>
                  ))}
              </div>
              {followingBusy && <p role="status">正在读取本机关注…</p>}
              {following && !following.authors.length && (
                <p className="source-empty">
                  还没有关注作者。可从作品详情加入本机关注。
                </p>
              )}
            </>
          ) : (
            <>
              <div className="source-results-heading">
                <span>
                  {view === "following" && !authorSearch
                    ? "已关注作品 " + followedWorks.length + " 部"
                    : "已读取 " +
                      items.length +
                      " 部" +
                      (totalKnown
                        ? " / 来源报告 " + pageInfo!.total + " 部"
                        : " · 总数未知")}
                </span>
                <div className="source-actions">
                  <label className="source-sort">
                    排序{" "}
                    <select
                      value={sort}
                      onChange={(event) => changeSort(event.target.value)}
                    >
                      <option value="source">
                        {source === "Pica" && view === "favorites"
                          ? "收藏时间：从新到旧"
                          : "来源顺序"}
                      </option>
                      <option value="source-reverse">
                        {source === "Pica" && view === "favorites"
                          ? "收藏时间：从旧到新"
                          : "来源倒序"}
                      </option>
                      <option value="title">
                        作品名称：升序（已读取范围）
                      </option>
                      <option value="title-desc">
                        作品名称：降序（已读取范围）
                      </option>
                    </select>
                  </label>
                  <button
                    type="button"
                    className="text-button"
                    data-testid="source-toggle-selection"
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
                      type="button"
                      className="text-button"
                      data-testid="source-select-all"
                      disabled={!visible.length}
                      onClick={() => setSelection(visible.map(sourceWorkKey))}
                    >
                      全选当前已读取范围
                    </button>
                  )}
                </div>
              </div>
              {view !== "following" && pageInfo && (
                <p className="source-muted" data-testid="source-completeness">
                  {error ||
                  (view === "favorites" && collectionState.error !== null)
                    ? "本次读取未完成，保留上次已读结果"
                    : complete
                      ? "已读取完整范围"
                      : "范围尚未读全，已读取页面不代表全部作品"}{" "}
                  · 手机名单与电脑文件分别核对；下载从详情单本确认
                </p>
              )}
              {grid(visible)}
              {view === "favorites" && (
                <div
                  ref={sentinel}
                  data-testid="collection-sentinel"
                  className="collection-status"
                >
                  <p role="status" data-testid="collection-progress">
                    {collectionState.phase === "error"
                      ? "读取已停止，已读内容保留，请点击重试读取"
                      : collectionState.phase === "complete"
                        ? "已读取全部收藏"
                        : collectionState.phase === "restoring"
                          ? "正在读取本机缓存…"
                          : collectionState.phase === "verifying"
                            ? "正在核对来源首页…"
                            : collectionState.phase === "reading"
                              ? "正在读取下一页…"
                              : autoPaused
                                ? "自动续读已暂停"
                                : query.trim()
                                  ? "仅筛选已读取范围；清空筛选后继续自动读取"
                                  : "向下滚动继续读取"}
                    {" · 已读取 " +
                      collectionRecords +
                      (collectionState.snapshot?.total === null ||
                      !collectionState.snapshot
                        ? " · 总数未知"
                        : " / " + collectionState.snapshot.total)}
                    {source === "Pica" &&
                      " 条来源记录 · " + collectionWorks + " 部不同作品"}
                    {source === "Pica" &&
                      collectionDuplicates > 0 &&
                      " · " + collectionDuplicates + " 条重复记录"}
                  </p>
                  {collectionState.displaySnapshot && (
                    <p
                      className="source-muted"
                      data-testid="collection-freshness"
                    >
                      {collectionState.freshness === "cached"
                        ? "本机缓存，尚待核对"
                        : collectionState.freshness === "verified-cache"
                          ? "本机缓存，首页已核对"
                          : "本次已读取结果"}
                      {" · " +
                        new Date(
                          collectionState.displaySnapshot.updatedAt,
                        ).toLocaleString()}
                    </p>
                  )}
                  {reversePreparing && collectionState.phase !== "error" && (
                    <p role="status">
                      正在准备完整来源倒序，当前仍显示已读来源顺序。可暂停或改回来源顺序。
                    </p>
                  )}
                  {collectionState.error !== null && (
                    <p role="alert">
                      {sourceErrorMessage(collectionState.error)}
                    </p>
                  )}
                  {collectionState.cacheWarning && (
                    <p role="status">{collectionState.cacheWarning}</p>
                  )}
                  {collectionState.phase === "error" && (
                    <button
                      type="button"
                      className="text-button"
                      data-testid="collection-retry"
                      onClick={() => {
                        setAutoPaused(false);
                        resumeCollection(true);
                      }}
                    >
                      重试读取
                    </button>
                  )}
                  {!complete && collectionState.phase !== "error" && (
                    <button
                      type="button"
                      className="text-button"
                      data-testid="collection-pause"
                      onClick={() => {
                        if (autoPaused) {
                          setAutoPaused(false);
                        } else {
                          setAutoPaused(true);
                          collector.current?.pause();
                        }
                      }}
                    >
                      {autoPaused ? "继续自动读取" : "暂停自动读取"}
                    </button>
                  )}
                </div>
              )}
              {loading && (
                <p role="status">正在读取第 {lastRead.current.page} 页…</p>
              )}
              {!loading && !error && !visible.length && (
                <div className="source-empty" data-testid="source-empty">
                  <h2>
                    {searching && !pageInfo
                      ? "搜索当前来源"
                      : query.trim()
                        ? "当前范围没有匹配作品"
                        : pageInfo
                          ? "当前来源范围没有作品"
                          : "尚未读取作品"}
                  </h2>
                  <p>
                    {searching && !pageInfo
                      ? "提交关键词，或切换到单个编号 / 链接直接查看。"
                      : "可修改搜索条件、重新读取或切换来源。"}
                  </p>
                </div>
              )}
              {view !== "favorites" && items.length >= 1000 && !complete && (
                <p role="status" className="source-notice">
                  当前最多保留 1000
                  部来源作品。请缩小关键词或收藏夹范围后继续读取。
                </p>
              )}
              {view === "search" && pageInfo && !complete && !error && (
                <button
                  type="button"
                  className="button secondary"
                  disabled={loading || items.length >= 1000}
                  data-testid="source-next-page"
                  onClick={() =>
                    void readList(
                      lastListQuery.current.kind,
                      lastListQuery.current.query,
                      lastListQuery.current.folderId,
                      pageInfo.page + 1,
                      true,
                    )
                  }
                >
                  继续读取下一页
                </button>
              )}
              {selectedWorks.length > 0 && (
                <div
                  className="source-selection-bar"
                  data-testid="source-selection-bar"
                >
                  <strong>已选 {selectedWorks.length} 部</strong>
                  <span>包含已读范围中未进入视口的作品</span>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setSelection([])}
                  >
                    取消选择
                  </button>
                  <button
                    type="button"
                    className="button secondary"
                    data-testid="source-batch-booklist"
                    disabled={organizing}
                    onClick={() => void organize(selectedWorks)}
                  >
                    加入书单
                  </button>
                  <button type="button" className="button primary" disabled>
                    真实下载尚未接入
                  </button>
                </div>
              )}
            </>
          )}
        </>
      )}
      {view === "following" && followingTab === "works" && !authorSearch && (
        <p className="source-muted">
          这里的作品关注保存在本机当前账号下，仅手动查看来源，尚未启用自动监控。
        </p>
      )}
    </>
  );
  return (
    <section
      ref={host}
      hidden={!active}
      className={
        "source-workbench" +
        (selectedWorks.length && !detailRef ? " has-source-selection" : "")
      }
      data-testid="source-workbench"
      data-source={source}
      data-origin={adapter.mode}
    >
      {active && searchHost ? createPortal(searchControl, searchHost) : null}
      {body}
    </section>
  );
}

export interface SourceWorkGridProps {
  works: SourceWork[];
  scopes: SourceScope[];
  adapter: SourceAdapter;
  density: 5 | 7 | 9;
  onOpenWork(reference: WorkReference): void;
  onAddToBooklists?(references: WorkReference[]): Promise<boolean>;
}
/** A read-only real-metadata projection for local booklists; it never uses demo inventory. */
export function SourceWorkGrid({
  works,
  scopes,
  adapter,
  density,
  onOpenWork,
  onAddToBooklists,
}: SourceWorkGridProps) {
  const [coverEpoch] = useState(0);
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState("");
  const lock = useRef(false);
  async function organize(work: SourceWork) {
    if (!onAddToBooklists || lock.current) return;
    lock.current = true;
    setPending(true);
    try {
      setNotice(
        (await onAddToBooklists([toWorkReference(work)]))
          ? "书单已保存。"
          : "书单操作未完成。",
      );
    } catch {
      setNotice("书单未能保存，请重试。");
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  return (
    <>
      {notice && (
        <p role="status" className="source-notice">
          {notice}
        </p>
      )}
      <div
        className="source-grid"
        data-testid="source-reference-grid"
        data-density={density}
      >
        {mergeSourceWorks([], works).map((work) => {
          const scope = scopes.find((item) => item.source === work.source);
          return (
            <article
              className="source-card"
              key={sourceWorkKey(work)}
              data-source-work-key={sourceWorkKey(work)}
              data-testid={"source-reference-card-" + sourceWorkKey(work)}
            >
              <button
                type="button"
                className="source-cover-button"
                onClick={() => onOpenWork(toWorkReference(work))}
                aria-label={"查看《" + work.title + "》来源详情"}
              >
                {scope ? (
                  <SourceCover adapter={adapter} scope={scope} work={work} />
                ) : (
                  <div className="source-cover">
                    <span>连接来源后读取封面</span>
                  </div>
                )}
              </button>
              <h3>
                <button
                  type="button"
                  onClick={() => onOpenWork(toWorkReference(work))}
                >
                  {work.title}
                </button>
              </h3>
              <p>
                {work.authors.length
                  ? work.authors.join("、")
                  : "作者资料未取得"}
              </p>
              <p className="source-card-state">
                {sourceLabel(work.source)} · 库存状态待核对
              </p>
              {onAddToBooklists && (
                <button
                  type="button"
                  className="text-button"
                  disabled={pending}
                  onClick={() => void organize(work)}
                >
                  加入其他书单
                </button>
              )}
            </article>
          );
        })}
      </div>
    </>
  );
}
