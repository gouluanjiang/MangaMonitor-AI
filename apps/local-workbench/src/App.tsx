import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { works, activeFixture } from "./catalog.ts";
import {
  closeDemo,
  enqueueWorks,
  initialDemoState,
  reopenDemo,
  restoreDemoState,
  retryDemoTask,
  setDemoOnline,
  setDemoPaused,
  tickDemo,
  toggleTaskPause,
} from "./demo-store.ts";
import type { DemoTask, TaskStage, Work } from "./types.ts";
import { Icon } from "./icons.tsx";
import { WorkbenchSettings } from "./WorkbenchSettings.tsx";
import { initialPreferences, decodeBackgroundImage } from "./preferences.ts";
import type { WorkbenchPreferences } from "./preferences.ts";

import { BooklistControls, BooklistPicker } from "./BooklistControls.tsx";
import { initialBooklists, removeBooklistMembers } from "./booklists.ts";
import type { BooklistsDocument, WorkReference } from "./booklists.ts";
import {
  createWorkbenchPersistence,
  NATIVE_BACKGROUND_BYTES,
  persistenceErrorMessage,
} from "./persistence.ts";
import type { DocumentSnapshot } from "./persistence.ts";
import "./booklists.css";
import { AccountSettings } from "./AccountSettings.tsx";
import { SourceWorkbench } from "./SourceWorkbench.tsx";
import {
  LibraryWorkbench,
  LibrarySettingsPanel,
  useLibrary,
  usePhoneLibrary,
} from "./LibraryWorkbench.tsx";
import { createLibraryAdapter } from "./library-runtime.ts";
import {
  createDownloadAdapter,
  isDownloadPresent,
  getDownloadScope,
} from "./download-runtime.ts";
import {
  NativeDownloads,
  DownloadSettingsPanel,
  useDownloads,
  downloadStatusText,
  unfinishedDownloadCount,
} from "./NativeDownloads.tsx";
import type {
  DownloadContext,
  DownloadContexts,
  DownloadSource,
  DownloadTask,
} from "./download-types.ts";
import { parseLibraryReference } from "./library-model.ts";
import { createPhoneLibraryAdapter } from "./phone-library-runtime.ts";
import { NativeBooklistMembers } from "./NativeBooklistMembers.tsx";
import { createSourceAdapter, sourceErrorMessage } from "./source-runtime.ts";
import { boundSourceCache } from "./source-memory.ts";
import { sources, sourceWorkKey, sourceLabel } from "./source-types.ts";
import type {
  AccountSummary,
  Source,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
const sourceAdapter = createSourceAdapter();
const libraryAdapter = createLibraryAdapter();
const downloadAdapter = createDownloadAdapter();
const phoneLibraryAdapter = createPhoneLibraryAdapter();
const persistence = createWorkbenchPersistence({ fixture: activeFixture });
const workReference = (work: Work): WorkReference => ({
  source: work.source,
  workId: work.id,
});
const referenceMatches = (reference: WorkReference, work: Work) =>
  reference.source === work.source && reference.workId === work.id;

const STORAGE_KEY =
  "mangamonitor.workbench.demo.v1" + (activeFixture ? "." + activeFixture : "");
const labels: Record<TaskStage, string> = {
  queued: "等待下载",
  downloading: "正在下载",
  verifying: "校验图片",
  packing: "保存作品",
  importing: "校验 ZIP 并入库",
  sync_pending: "已入库，等待同步",
  completed: "已完成",
  error: "需要处理",
};
type Page =
  "library" | "favorites" | "discovery" | "queue" | "authors" | "settings";
type Filter = "all" | "owned" | "ready" | "review";
const pageNames: Record<Page, string> = {
  library: "漫画库",
  favorites: "在线收藏",
  discovery: "发现",
  queue: "下载队列",
  authors: "关注",
  settings: "设置",
};
const localStages: TaskStage[] = [
  "queued",
  "downloading",
  "verifying",
  "packing",
  "importing",
];
const lookup = (id: string) => works.find((work) => work.id === id)!;

function readSavedDemo() {
  try {
    return restoreDemoState(localStorage.getItem(STORAGE_KEY));
  } catch {
    return initialDemoState();
  }
}

function Dialog({
  children,
  onClose,
  title,
  testId,
  dismissible = true,
}: {
  children: ReactNode;
  onClose: () => void;
  title: string;
  testId?: string;
  dismissible?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        if (dismissible) onClose();
      }}
      data-testid={testId}
    >
      <div className="dialog-heading">
        <h2>{title}</h2>
        {dismissible && (
          <button
            className="icon-button"
            aria-label="关闭对话框"
            onClick={onClose}
          >
            <Icon name="close" />
          </button>
        )}
      </div>
      {children}
    </dialog>
  );
}

export default function App() {
  const [downloadInput, setDownloadInput] = useState("");
  const [downloadSource, setDownloadSource] = useState<DownloadSource>("JM");
  const [downloadFeedback, setDownloadFeedback] = useState(false);
  const [downloadLibraryRefresh, setDownloadLibraryRefresh] = useState(false);
  const [pendingDownloadedWork, setPendingDownloadedWork] =
    useState<DownloadTask | null>(null);
  const [libraryNavigationKey, setLibraryNavigationKey] = useState(0);
  const library = useLibrary(libraryAdapter, persistence.native);
  const phoneLibrary = usePhoneLibrary(phoneLibraryAdapter, persistence.native);
  const [requestedLibraryWork, setRequestedLibraryWork] =
    useState<SourceWork | null>(null);
  const [libraryRequestKey, setLibraryRequestKey] = useState(0);
  const [accounts, setAccounts] = useState<AccountSummary[]>(() =>
    sources.map((source) => ({
      source,
      sessionId: null,
      accountId: null,
      displayName: null,
      state: "disconnected",
      remembered: false,
      errorCode: null,
    })),
  );
  const [loadingAccounts, setLoadingAccounts] = useState(persistence.native);
  const [accountsError, setAccountsError] = useState("");
  const accountsRef = useRef(accounts);
  accountsRef.current = accounts;
  const contextForSource = (source: DownloadSource): DownloadContext | null => {
    const scope = getDownloadScope(accounts, source);
    return scope && library.snapshot.rootId
      ? {
          scope,
          rootId: library.snapshot.rootId,
          generation: library.snapshot.generation,
        }
      : null;
  };
  const downloadContexts: DownloadContexts = {
    JM: contextForSource("JM"),
    Pica: contextForSource("Pica"),
  };
  const downloads = useDownloads(
    downloadAdapter,
    persistence.native,
    downloadContexts,
    () => setDownloadLibraryRefresh(true),
  );
  useEffect(() => {
    if (!downloadLibraryRefresh || library.busy) return;
    setDownloadLibraryRefresh(false);
    void library.controller.read();
  }, [downloadLibraryRefresh, library.busy, library.controller]);
  const [sourceCache, setSourceCache] = useState<
    Record<string, { scope: SourceScope; work: SourceWork }>
  >({});
  const lastSourceView = useRef<"favorites" | "search" | "following">(
    "favorites",
  );
  const [requestedSource, setRequestedSource] = useState<Source>();
  const [requestedWork, setRequestedWork] = useState<WorkReference>();
  const [sourceRequestKey, setSourceRequestKey] = useState(0);
  const [sourceSearchHost, setSourceSearchHost] =
    useState<HTMLDivElement | null>(null);
  const mergeAccounts = useCallback((updates: AccountSummary[]) => {
    const next = accountsRef.current.map(
      (previous) =>
        updates.find((update) => update.source === previous.source) ?? previous,
    );
    accountsRef.current = next;
    setAccounts(next);
    setSourceCache((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([, entry]) =>
          next.some(
            (account) =>
              account.source === entry.scope.source &&
              account.state === "connected" &&
              account.sessionId === entry.scope.sessionId,
          ),
        ),
      ),
    );
    setAccountsError("");
  }, []);
  const cacheSourceWorks = useCallback(
    (scope: SourceScope, incoming: SourceWork[]) => {
      if (
        !accountsRef.current.some(
          (account) =>
            account.source === scope.source &&
            account.state === "connected" &&
            account.sessionId === scope.sessionId,
        )
      )
        return;
      setSourceCache((previous) => {
        const next = { ...previous };
        for (const work of incoming)
          if (work.source === scope.source) {
            delete next[sourceWorkKey(work)];
            next[sourceWorkKey(work)] = { scope, work };
          }
        return boundSourceCache(next);
      });
    },
    [],
  );
  useEffect(() => {
    if (!persistence.native) return;
    let disposed = false;
    void sourceAdapter
      .accounts()
      .then((updates) => {
        if (!disposed) mergeAccounts(updates);
      })
      .catch((cause) => {
        if (!disposed) setAccountsError(sourceErrorMessage(cause));
      })
      .finally(() => {
        if (!disposed) setLoadingAccounts(false);
      });
    return () => {
      disposed = true;
    };
  }, [mergeAccounts]);
  const [state, setState] = useState(() =>
    persistence.native ? { ...initialDemoState(), tasks: [] } : readSavedDemo(),
  );
  const [page, setPage] = useState<Page>("library");
  const [detail, setDetail] = useState<string | null>(null);
  const [detailTab, setDetailTab] = useState("chapters");
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [settingsQuery, setSettingsQuery] = useState("");
  const [source, setSource] = useState("all");
  const [sort, setSort] = useState("updated");
  const [selection, setSelection] = useState<string[]>([]);
  const [selectionMode, setSelectionMode] = useState(false);
  const [libraryTab, setLibraryTab] = useState("all");
  const [toolbarStuck, setToolbarStuck] = useState(false);
  const [confirmation, setConfirmation] = useState<string[] | null>(null);
  const [queueFilter, setQueueFilter] = useState("all");
  const [notice, setNotice] = useState("");
  const [storageFailed, setStorageFailed] = useState(false);
  const [preferences, setPreferences] = useState(initialPreferences);
  const [preferencesReady, setPreferencesReady] = useState(false);
  const [preferencesFailed, setPreferencesFailed] = useState(false);
  const [preferencesSaving, setPreferencesSaving] = useState(false);
  const [preferencesError, setPreferencesError] = useState("");
  const preferencesSnapshot =
    useRef<DocumentSnapshot<WorkbenchPreferences> | null>(null);
  const preferencesBusy = useRef(false);
  const preferencesRead = useRef(0);
  const [booklists, setBooklists] = useState(initialBooklists);
  const [booklistsReady, setBooklistsReady] = useState(false);
  const [booklistsSaving, setBooklistsSaving] = useState(false);
  const [booklistsError, setBooklistsError] = useState("");
  const [selectedBooklistId, setSelectedBooklistId] = useState<string | null>(
    null,
  );
  const [booklistPickerMembers, setBooklistPickerMembers] = useState<
    WorkReference[] | null
  >(null);
  const booklistsSnapshot = useRef<DocumentSnapshot<BooklistsDocument> | null>(
    null,
  );
  const pickerCompletion = useRef<((saved: boolean) => void) | null>(null);
  const openSourceBooklistPicker = useCallback(
    (refs: WorkReference[]) =>
      new Promise<boolean>((resolve) => {
        pickerCompletion.current?.(false);
        pickerCompletion.current = resolve;
        setBooklistPickerMembers(refs);
      }),
    [],
  );
  function closeBooklistPicker(saved = false) {
    pickerCompletion.current?.(saved);
    pickerCompletion.current = null;
    setBooklistPickerMembers(null);
  }
  useEffect(
    () => () => {
      pickerCompletion.current?.(false);
    },
    [],
  );
  const booklistsBusy = useRef(false);
  const booklistsRead = useRef(0);
  const viewIdentity = useRef("");
  viewIdentity.current = JSON.stringify([
    page,
    detail,
    libraryTab,
    selectedBooklistId,
    query,
    source,
    filter,
  ]);
  const currentBooklist = booklists.lists.find(
    (list) => list.id === selectedBooklistId && !list.archived,
  );
  const booklistView = page === "library" && libraryTab === "booklists";
  const libraryActive =
    persistence.native && page === "library" && !booklistView;
  const sourceActive =
    persistence.native && ["favorites", "discovery", "authors"].includes(page);
  const sourceView =
    page === "favorites"
      ? "favorites"
      : page === "discovery"
        ? "search"
        : page === "authors"
          ? "following"
          : lastSourceView.current;
  lastSourceView.current = sourceView;
  function openSourceWork(ref: WorkReference) {
    setRequestedSource(ref.source);
    setRequestedWork(ref);
    setSourceRequestKey((key) => key + 1);
    navigate("discovery");
  }
  function chooseDownloadLibrary() {
    navigate("library");
    setLibraryTab("all");
    setQuery("");
    setLibraryNavigationKey((value) => value + 1);
    setNotice("选择电脑漫画目录后，返回下载队列继续准备这本作品。");
  }
  function showDownloadLibrary(work: SourceWork) {
    navigate("library");
    setLibraryTab("all");
    setQuery("");
    setRequestedLibraryWork(work);
    setLibraryRequestKey((value) => value + 1);
  }
  function openDownloaded(task: DownloadTask) {
    if (!isDownloadPresent(task)) return;
    setPendingDownloadedWork(task);
    if (!library.snapshot.items.some((item) => item.id === task.libraryEntryId))
      setDownloadLibraryRefresh(true);
  }
  useEffect(() => {
    if (!pendingDownloadedWork || downloadLibraryRefresh || library.busy)
      return;
    const task = pendingDownloadedWork;
    setPendingDownloadedWork(null);
    if (
      !downloads.snapshot.tasks.some(
        (current) => current.id === task.id && isDownloadPresent(current),
      )
    ) {
      setNotice("当前电脑文件状态已变化，请在下载队列核对。");
      return;
    }
    if (!library.snapshot.items.some((item) => item.id === task.libraryEntryId))
      setNotice("这本作品已下载，当前目录尚未读取到对应文件。请核对电脑目录。");
    showDownloadLibrary({
      source: task.source,
      workId: task.workId,
      title: task.title,
      authors: [],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: null,
      pageCount: null,
      coverAvailable: false,
    });
  }, [
    pendingDownloadedWork,
    downloadLibraryRefresh,
    library.busy,
    library.snapshot.items,
    downloads.snapshot.tasks,
  ]);
  async function beginDownload(
    input: string,
    work?: SourceWork,
    requestedSource: DownloadSource = work?.source ?? downloadSource,
  ) {
    const downloadScope = getDownloadScope(
      accountsRef.current,
      requestedSource,
    );
    const startingLibrary = library.controller.getState().snapshot;
    const downloadContext: DownloadContext | null =
      downloadScope && startingLibrary.rootId
        ? {
            scope: downloadScope,
            rootId: startingLibrary.rootId,
            generation: startingLibrary.generation,
          }
        : null;
    setDownloadSource(requestedSource);
    setDownloadInput(input);
    setDownloadFeedback(true);
    if (!downloadScope) {
      setNotice(`请先连接${sourceLabel(requestedSource)}账号。`);
      navigate("settings");
      return;
    }
    if (!downloadContext) {
      chooseDownloadLibrary();
      return;
    }
    if (downloads.controller.getState().busy) return;
    // An earlier queue check may have observed files before this click. Wait
    // for it, then request one fresh check for this explicit preparation.
    if (downloads.controller.getState().reading)
      await downloads.controller.read(false);
    await downloads.controller.read(true);
    const currentDownloads = downloads.controller.getState();
    if (
      !currentDownloads.ready ||
      currentDownloads.error ||
      currentDownloads.busy
    )
      return;
    const currentLibrary = library.controller.getState().snapshot;
    const currentAccount = accountsRef.current.find(
      (account) => account.source === requestedSource,
    );
    if (
      currentAccount?.state !== "connected" ||
      currentAccount.sessionId !== downloadScope.sessionId ||
      currentLibrary.rootId !== downloadContext.rootId ||
      currentLibrary.generation !== downloadContext.generation
    ) {
      setNotice("账号或电脑目录已改变，请核对后重新准备下载。");
      return;
    }
    const id =
      work?.workId ?? parseLibraryReference(requestedSource, input)?.workId;
    const missingEntries = new Set(
      currentDownloads.snapshot.tasks
        .filter(
          (task) =>
            task.source === requestedSource &&
            task.workId === id &&
            task.phase === "downloaded" &&
            task.localFiles === "missing",
        )
        .map((task) => task.libraryEntryId),
    );
    const existing = id
      ? currentLibrary.items.find(
          (item) =>
            item.sourceRef?.source === requestedSource &&
            item.sourceRef.workId === id &&
            !missingEntries.has(item.id),
        )
      : undefined;
    if (existing && id) {
      setNotice("电脑已有该作品副本，请先核对电脑文件。");
      showDownloadLibrary({
        source: requestedSource,
        workId: id,
        title: existing.title,
        authors: existing.authors,
        description: existing.description,
        tags: existing.tags,
        favorite: null,
        chapterCount: null,
        pageCount: existing.pageCount,
        coverAvailable: existing.coverAvailable,
      });
      return;
    }
    void downloads.controller.prepare(downloadContext, id ?? input);
  }
  function openSourceFavorites(source: Source) {
    setRequestedSource(source);
    setRequestedWork(undefined);
    setSourceRequestKey((key) => key + 1);
    navigate("favorites");
  }
  const unavailableMembers =
    currentBooklist?.members.filter(
      (member) => !works.some((work) => referenceMatches(member, work)),
    ) ?? [];
  const [appearanceDraft, setAppearanceDraft] = useState<
    WorkbenchPreferences["appearance"] | null
  >(null);
  const [failedBackground, setFailedBackground] = useState<string | null>(null);
  const appearance = appearanceDraft ?? preferences.appearance;
  const displayBackground =
    appearance.backgroundImage === failedBackground
      ? null
      : appearance.backgroundImage;
  const contentRef = useRef<HTMLElement>(null);
  const savedListAnchor = useRef<Anchor | null>(null);
  type Anchor = { id: string; grid: string; offset: number; scroll: number };
  const settingsOrigin = useRef<{
    page: Page;
    anchor: Anchor | null;
    detail: string | null;
  } | null>(null);
  function updateToolbarSurface() {
    const container = contentRef.current;
    const toolbar = container?.querySelector(".library-toolbar");
    setToolbarStuck(
      Boolean(
        container &&
        toolbar &&
        toolbar.getBoundingClientRect().top <=
          container.getBoundingClientRect().top + 1,
      ),
    );
  }
  useEffect(() => {
    updateToolbarSurface();
    const container = contentRef.current;
    if (!container) return;
    const observer = new ResizeObserver(updateToolbarSurface);
    observer.observe(container);
    container
      .querySelectorAll(".recent-section, .page-heading, .library-toolbar")
      .forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, [page, detail, query, source, filter, libraryTab, appearance.density]);
  function captureAnchor(preferredId?: string): Anchor | null {
    const container = contentRef.current;
    if (!container) return null;
    const containerTop = container.getBoundingClientRect().top;
    const toolbar = container
      .querySelector(".library-toolbar")
      ?.getBoundingClientRect();
    const top =
      toolbar && toolbar.top <= containerTop + 1
        ? toolbar.bottom
        : containerTop;
    const card = Array.from(
      container.querySelectorAll<HTMLElement>("[data-work-id]"),
    ).find(
      (card) =>
        card.getBoundingClientRect().bottom > top &&
        (!preferredId || card.dataset.workId === preferredId),
    );
    return {
      id: card?.dataset.workId ?? "",
      grid: card?.closest(".cover-grid")?.getAttribute("data-testid") ?? "",
      offset: card ? card.getBoundingClientRect().top - containerTop : 0,
      scroll: container.scrollTop,
    };
  }
  function restoreAnchor(anchor: Anchor | null) {
    window.requestAnimationFrame(() => {
      const container = contentRef.current;
      if (!container || !anchor) return;
      const card = Array.from(
        container.querySelectorAll<HTMLElement>("[data-work-id]"),
      ).find(
        (card) =>
          card.dataset.workId === anchor.id &&
          card.closest(".cover-grid")?.getAttribute("data-testid") ===
            anchor.grid,
      );
      if (card)
        container.scrollTop +=
          card.getBoundingClientRect().top -
          container.getBoundingClientRect().top -
          anchor.offset;
      else container.scrollTop = anchor.scroll;
    });
  }
  function clearScopeSelection() {
    if (selection.length) setNotice("选择范围已改变，已清空临时选择");
    setSelection([]);
  }

  async function loadPreferences() {
    if (preferencesBusy.current) return;
    preferencesBusy.current = true;
    setPreferencesSaving(true);
    const request = ++preferencesRead.current;
    try {
      const loaded = await persistence.preferences.read();
      const dataUrl = loaded.value.appearance.backgroundImage;
      const decoded =
        dataUrl === null ||
        (await decodeBackgroundImage(
          dataUrl,
          persistence.native ? NATIVE_BACKGROUND_BYTES : undefined,
        ));
      if (request !== preferencesRead.current) return;
      preferencesSnapshot.current = loaded;
      setPreferences(loaded.value);
      setFailedBackground(decoded ? null : dataUrl);
      setPreferencesFailed(false);
      setPreferencesError("");
      setPreferencesReady(true);
    } catch (error) {
      if (request !== preferencesRead.current) return;
      preferencesSnapshot.current = null;
      setPreferencesFailed(true);
      setPreferencesError(persistenceErrorMessage(error));
    } finally {
      if (request === preferencesRead.current) {
        preferencesBusy.current = false;
        setPreferencesSaving(false);
      }
    }
  }
  async function loadBooklists() {
    if (booklistsBusy.current) return;
    booklistsBusy.current = true;
    setBooklistsSaving(true);
    const request = ++booklistsRead.current;
    try {
      const loaded = await persistence.booklists.read();
      if (request !== booklistsRead.current) return;
      booklistsSnapshot.current = loaded;
      setBooklists(loaded.value);
      setBooklistsReady(true);
      setBooklistsError("");
      setSelectedBooklistId((previous) =>
        loaded.value.lists.some(
          (list) => !list.archived && list.id === previous,
        )
          ? previous
          : (loaded.value.lists.find((list) => !list.archived)?.id ?? null),
      );
    } catch (error) {
      if (request !== booklistsRead.current) return;
      booklistsSnapshot.current = null;
      setBooklistsReady(false);
      setBooklistsError(persistenceErrorMessage(error));
    } finally {
      if (request === booklistsRead.current) {
        booklistsBusy.current = false;
        setBooklistsSaving(false);
      }
    }
  }
  useEffect(() => {
    void loadPreferences();
    void loadBooklists();
    return () => {
      preferencesRead.current += 1;
      booklistsRead.current += 1;
      // StrictMode starts a fresh read after invalidating the first effect.
      preferencesBusy.current = false;
      booklistsBusy.current = false;
    };
  }, []);

  async function commitPreferences(
    next: WorkbenchPreferences,
  ): Promise<boolean> {
    const previous = preferencesSnapshot.current;
    if (!previous || preferencesBusy.current) return false;
    preferencesBusy.current = true;
    setPreferencesSaving(true);
    try {
      const saved = await persistence.preferences.write(previous, next);
      preferencesSnapshot.current = saved;
      setPreferences(saved.value);
      setPreferencesFailed(false);
      setPreferencesError("");
      if (saved.value.appearance.backgroundImage !== failedBackground)
        setFailedBackground(null);
      return true;
    } catch (error) {
      setPreferencesFailed(true);
      setPreferencesError(persistenceErrorMessage(error));
      return false;
    } finally {
      preferencesBusy.current = false;
      setPreferencesSaving(false);
    }
  }
  async function commitBooklists(next: BooklistsDocument): Promise<boolean> {
    const previous = booklistsSnapshot.current;
    if (!previous || booklistsBusy.current) return false;
    booklistsBusy.current = true;
    setBooklistsSaving(true);
    try {
      const saved = await persistence.booklists.write(previous, next);
      booklistsSnapshot.current = saved;
      setBooklists(saved.value);
      setBooklistsError("");
      return true;
    } catch (error) {
      setBooklistsError(persistenceErrorMessage(error));
      return false;
    } finally {
      booklistsBusy.current = false;
      setBooklistsSaving(false);
    }
  }
  const changeDensity = async (density: 5 | 7 | 9) => {
    const anchor = captureAnchor();
    const before = viewIdentity.current;
    if (
      !(await commitPreferences({
        ...preferences,
        appearance: { ...preferences.appearance, density },
      }))
    ) {
      setNotice("封面密度未能保存，请稍后重试");
      return;
    }
    // The native library restores its virtual-grid item after density commits.
    // The demo anchor has no matching cards there and would restore stale pixels.
    if (!libraryActive && viewIdentity.current === before)
      restoreAnchor(anchor);
  };

  useEffect(() => {
    if (persistence.native) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
      setStorageFailed(false);
    } catch {
      setStorageFailed(true);
    }
  }, [state]);
  useEffect(() => {
    if (persistence.native) return;
    const timer = window.setInterval(
      () => setState((previous) => tickDemo(previous)),
      1800,
    );
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 4500);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const taskFor = (workId: string) =>
    state.tasks.find((task) => task.workId === workId);
  const isOwned = (work: Work) =>
    work.status === "owned" ||
    ["sync_pending", "completed"].includes(taskFor(work.id)?.stage ?? "");
  const canSelect = (work: Work) =>
    work.status === "ready" && !taskFor(work.id);
  const ownedCount = works.filter(isOwned).length;
  const unfinished = state.tasks.filter(
    (task) => task.stage !== "completed",
  ).length;
  const activeTask =
    !state.paused && !state.closed
      ? state.tasks.find(
          (task) => !task.paused && localStages.includes(task.stage),
        )
      : undefined;
  const displayStage = (task: DemoTask) => {
    if (
      task.stage === "error" ||
      task.stage === "completed" ||
      task.stage === "sync_pending"
    )
      return labels[task.stage];
    if (state.paused || task.paused || state.closed) return "已暂停";
    if (task.id !== activeTask?.id)
      return task.stage === "queued" ? "等待下载" : "等待继续";
    return labels[task.stage];
  };
  const workBadge = (work: Work) => {
    const task = taskFor(work.id);
    if (task?.stage === "error") return { text: "需要处理", tone: "warning" };
    if (isOwned(work)) return { text: "已入库", tone: "success" };
    if (work.status === "review") return { text: "待复核", tone: "warning" };
    if (task) return { text: displayStage(task), tone: "muted" };
    return { text: "可下载", tone: "muted" };
  };
  const navigate = (next: Page) => {
    if (next === page && !detail) return;
    if (next === "settings") {
      settingsOrigin.current = { page, anchor: captureAnchor(), detail };
      setPage(next);
      setDetail(null);
      contentRef.current?.scrollTo(0, 0);
      return;
    }
    if (page === "settings" && settingsOrigin.current?.page === next) {
      const origin = settingsOrigin.current;
      setPage(next);
      setDetail(origin.detail);
      restoreAnchor(origin.anchor);
      settingsOrigin.current = null;
      return;
    }
    settingsOrigin.current = null;
    setPage(next);
    setDetail(null);
    setSelection([]);
    setSelectionMode(false);
    setQuery("");
    setSource(next === "favorites" ? "JM" : "all");
    setFilter(next === "discovery" ? "ready" : "all");
    contentRef.current?.scrollTo(0, 0);
  };
  const openWork = (id: string) => {
    savedListAnchor.current = captureAnchor(id);
    setDetail(id);
    setDetailTab("chapters");
    contentRef.current?.scrollTo(0, 0);
  };
  const backToList = () => {
    setDetail(null);
    restoreAnchor(savedListAnchor.current);
  };
  const toggleSelection = (id: string) =>
    setSelection((previous) =>
      previous.includes(id)
        ? previous.filter((item) => item !== id)
        : [...previous, id],
    );
  const confirmDownload = () => {
    const selected = (confirmation ?? []).map(lookup).filter(canSelect);
    setState((previous) => enqueueWorks(previous, selected));
    setConfirmation(null);
    setSelection([]);
    setDetail(null);
    setPage("queue");
    setQueueFilter("all");
    setNotice(`${selected.length} 部作品已加入模拟队列`);
    contentRef.current?.scrollTo(0, 0);
  };

  const visibleWorks = works
    .filter((work) => {
      const match =
        `${work.title} ${work.subtitle} ${work.author} ${work.tags.join(" ")}`
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase());
      const filtered =
        filter === "all" ||
        (filter === "owned" && isOwned(work)) ||
        (filter === "ready" && work.status === "ready" && !isOwned(work)) ||
        (filter === "review" && work.status === "review");
      return (
        match &&
        filtered &&
        (page !== "library" ||
          (booklistView
            ? currentBooklist?.members.some((member) =>
                referenceMatches(member, work),
              )
            : isOwned(work))) &&
        (source === "all" || work.source === source)
      );
    })
    .sort((a, b) =>
      sort === "title"
        ? a.title.localeCompare(b.title, "zh-CN")
        : b.updated.localeCompare(a.updated),
    );
  const selectedWorks = selection.map(lookup).filter(Boolean);
  const chosen = selectedWorks.filter(canSelect);
  const currentWork = detail ? lookup(detail) : null;
  const queueTasks = state.tasks.filter(
    (task) =>
      queueFilter === "all" ||
      (queueFilter === "active" && localStages.includes(task.stage)) ||
      (queueFilter === "error" && task.stage === "error") ||
      (queueFilter === "done" &&
        ["sync_pending", "completed"].includes(task.stage)),
  );

  function renderGrid(items: Work[], gridId: string) {
    return (
      <div
        className="cover-grid"
        data-density={appearance.density}
        data-testid={gridId}
      >
        {items.map((work) => {
          const badge = workBadge(work);
          return (
            <article
              className={`work-card ${gridId !== "recent-grid" && selection.includes(work.id) ? "selected" : ""}`}
              key={work.id}
              data-work-id={work.id}
              data-testid={
                gridId === "recent-grid"
                  ? `recent-card-${work.id}`
                  : `card-${work.id}`
              }
            >
              <div className="cover-wrap">
                <button
                  className="cover-button"
                  onClick={() => openWork(work.id)}
                  data-testid={
                    gridId === "recent-grid"
                      ? `recent-open-${work.id}`
                      : `open-${work.id}`
                  }
                  aria-label={`查看《${work.title}》详情`}
                >
                  <img
                    src={work.cover}
                    alt={`${work.title}原创示意封面`}
                    width="400"
                    height="560"
                    loading="lazy"
                  />
                  <span className="cover-open">
                    查看作品 <Icon name="arrow" size={15} />
                  </span>
                </button>
                <span className="source-badge">{work.source}</span>
                {selectionMode && gridId !== "recent-grid" && (
                  <label
                    className="card-select"
                    title="选择作品；下载资格单独核对"
                  >
                    <input
                      type="checkbox"
                      aria-label={`选择《${work.title}》`}
                      data-testid={`select-${work.id}`}
                      checked={selection.includes(work.id)}
                      onChange={() => toggleSelection(work.id)}
                    />
                    <span>
                      <Icon name="check" size={13} />
                    </span>
                  </label>
                )}
              </div>
              <div className="card-meta">
                <button
                  className="title-button"
                  onClick={() => openWork(work.id)}
                >
                  {work.title}
                </button>
                <span className="chapter-count">{work.chapters} 章</span>
              </div>
              <p className="card-author">
                {work.author}
                <span>·</span>
                {work.tags[0]}
              </p>
              <div className="card-footer">
                <span className={`status ${badge.tone}`}>
                  <i />
                  {badge.text}
                </span>
                <span>{work.pages} 页</span>
              </div>
            </article>
          );
        })}
      </div>
    );
  }
  function renderLibrary() {
    return (
      <>
        <div
          className={
            "page-heading " + (page === "library" ? "library-heading" : "")
          }
        >
          <div>
            <div className="product-name">MangaMonitor</div>
            <h1>{pageNames[page]}</h1>
            {page !== "library" && (
              <p>
                {page === "favorites"
                  ? "收藏与本地库存对照 · 示例账号尚未连接"
                  : "新发现汇总，选择后一次确认下载"}
              </p>
            )}
          </div>
        </div>
        {page === "library" &&
          !query &&
          source === "all" &&
          libraryTab === "all" && (
            <section className="recent-section" aria-label="最近入库">
              <h2>最近入库</h2>
              {renderGrid(
                works
                  .filter(isOwned)
                  .sort((a, b) => b.updated.localeCompare(a.updated)),
                "recent-grid",
              )}
            </section>
          )}
        <div className={`library-toolbar${toolbarStuck ? " is-stuck" : ""}`}>
          {page === "library" ? (
            <div className="tabs" aria-label="漫画库范围">
              <button
                className={libraryTab === "all" ? "active" : ""}
                aria-pressed={libraryTab === "all"}
                onClick={() => {
                  setLibraryTab("all");
                  clearScopeSelection();
                  setFilter("all");
                }}
              >
                全部作品
              </button>
              <button
                className={libraryTab === "booklists" ? "active" : ""}
                aria-pressed={libraryTab === "booklists"}
                onClick={() => {
                  setLibraryTab("booklists");
                  clearScopeSelection();
                  setFilter("all");
                }}
              >
                本地书单
              </button>
            </div>
          ) : (
            <div className="tabs" aria-label="作品范围">
              {(
                [
                  ["all", "全部作品"],
                  ["owned", "已入库"],
                  ["ready", "待下载"],
                  ["review", "待复核"],
                ] as const
              ).map(([value, text]) => (
                <button
                  key={value}
                  className={filter === value ? "active" : ""}
                  onClick={() => {
                    setFilter(value);
                    clearScopeSelection();
                  }}
                  aria-pressed={filter === value}
                >
                  {text}
                  {value === "review" && <span className="tab-dot" />}
                </button>
              ))}
            </div>
          )}
          <div className="toolbar-selects">
            <label className="sr-only" htmlFor="source-filter">
              来源筛选
            </label>
            <select
              id="source-filter"
              value={source}
              onChange={(event) => {
                setSource(event.target.value);
                clearScopeSelection();
              }}
            >
              {page !== "favorites" && <option value="all">全部来源</option>}
              <option>JM</option>
              <option>Pica</option>
            </select>
            <label className="sr-only" htmlFor="sort">
              作品排序
            </label>
            <select
              id="sort"
              value={sort}
              onChange={(event) => setSort(event.target.value)}
            >
              <option value="updated">
                {page === "library" ? "最近入库" : "最近更新"}
              </option>
              <option value="title">作品名称</option>
            </select>
            <div className="density-control" role="group" aria-label="封面密度">
              <span>封面密度</span>
              {([5, 7, 9] as const).map((density) => (
                <button
                  key={density}
                  aria-label={`每行 ${density} 部`}
                  aria-pressed={appearance.density === density}
                  disabled={
                    !preferencesReady ||
                    !preferencesSnapshot.current ||
                    preferencesSaving
                  }
                  onClick={() => changeDensity(density)}
                >
                  {density}
                </button>
              ))}
            </div>
          </div>
        </div>
        {booklistView && (
          <BooklistControls
            document={booklists}
            selectedId={selectedBooklistId}
            disabled={!booklistsReady || booklistsSaving}
            onChange={commitBooklists}
            onReload={loadBooklists}
            reloadDisabled={booklistsSaving}
            onSelect={(id) => {
              setSelectedBooklistId(id);
              clearScopeSelection();
            }}
          />
        )}
        <div className="results-heading">
          <span>
            共 {visibleWorks.length} 部作品
            {booklistView &&
              currentBooklist &&
              ` · 书单关联 ${currentBooklist.members.length} 部`}
            {query && ` · 搜索“${query}”`}
          </span>
          {(!booklistView || currentBooklist) && (
            <div className="selection-controls">
              <button
                className="text-button"
                data-testid="toggle-selection"
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
                  data-testid="select-all-works"
                  disabled={!visibleWorks.length}
                  onClick={() =>
                    setSelection(visibleWorks.map((work) => work.id))
                  }
                >
                  全选作品（当前筛选）
                </button>
              )}
              <button
                className="text-button"
                onClick={() => {
                  setSelectionMode(true);
                  setSelection(
                    visibleWorks.filter(canSelect).map((work) => work.id),
                  );
                }}
                disabled={!visibleWorks.some(canSelect)}
              >
                全选待下载（当前筛选）
              </button>
            </div>
          )}
        </div>
        {booklistView &&
        (!currentBooklist ||
          (persistence.native &&
            unavailableMembers.length > 0 &&
            visibleWorks.length === 0)) ? null : visibleWorks.length ? (
          renderGrid(visibleWorks, "cover-grid")
        ) : (
          <div className="empty-state">
            <Icon name="search" size={32} />
            <h2>
              {booklistView && !currentBooklist?.members.length
                ? "这份书单还没有作品"
                : "没有找到这部作品"}
            </h2>
            <p>
              {booklistView && !currentBooklist?.members.length
                ? "从作品详情或多选栏加入作品，已入库与未下载的作品都能整理。"
                : "试试作品名、作者，或换一个筛选条件。"}
            </p>
            {booklistView && !currentBooklist?.members.length && (
              <button
                className="button secondary"
                onClick={() => navigate("discovery")}
              >
                前往发现选书
              </button>
            )}
            <button
              className="button secondary"
              onClick={() => {
                setQuery("");
                setFilter("all");
                if (page !== "favorites") setSource("all");
                setSelection([]);
              }}
            >
              清空筛选
            </button>
          </div>
        )}
        {booklistView &&
          currentBooklist &&
          persistence.native &&
          unavailableMembers.length > 0 && (
            <NativeBooklistMembers
              key={currentBooklist.id}
              members={unavailableMembers}
              accounts={accounts}
              adapter={sourceAdapter}
              cache={sourceCache}
              onWorksChanged={cacheSourceWorks}
              onAccountsChange={mergeAccounts}
              density={appearance.density}
              onOpenWork={openSourceWork}
              onAddToBooklists={openSourceBooklistPicker}
              query={query}
              sourceFilter={source}
              removeDisabled={!booklistsReady || booklistsSaving}
              onRemove={(member) =>
                commitBooklists(
                  removeBooklistMembers(
                    booklists,
                    currentBooklist.id,
                    [member],
                    Date.now(),
                  ),
                )
              }
            />
          )}
        {booklistView &&
          currentBooklist &&
          !persistence.native &&
          unavailableMembers.length > 0 && (
            <section
              className="unavailable-members"
              data-testid="unavailable-members"
            >
              <h3>部分作品资料暂不可用（{unavailableMembers.length}）</h3>
              <p>书单关联已保留，重新取得来源资料后可继续显示。</p>
              {unavailableMembers.map((member) => (
                <div key={member.source + ":" + member.workId}>
                  <span>
                    {member.source} · {member.workId}
                  </span>
                  <button
                    className="text-button"
                    disabled={!booklistsReady || booklistsSaving}
                    onClick={async () => {
                      await commitBooklists(
                        removeBooklistMembers(
                          booklists,
                          currentBooklist.id,
                          [member],
                          Date.now(),
                        ),
                      );
                    }}
                  >
                    移出书单
                  </button>
                </div>
              ))}
            </section>
          )}
        <div className="library-footnote">
          <span>
            {booklistView && persistence.native
              ? "书单关联保存在本机 · 库存待接入"
              : "所有封面与作品均为原创示意内容"}
          </span>
          <span>按行排列 · 向下浏览更多</span>
        </div>
        {selectedWorks.length > 0 && (
          <div className="selection-bar">
            <span className="selected-count">{selectedWorks.length}</span>
            <div>
              <strong>部作品已选择</strong>
              <small>包含当前筛选中未进入视口的所选作品</small>
            </div>
            <button className="text-button" onClick={() => setSelection([])}>
              取消选择
            </button>
            <button
              className="button secondary"
              data-testid="batch-booklist"
              disabled={!booklistsReady || booklistsSaving}
              onClick={() =>
                setBooklistPickerMembers(selectedWorks.map(workReference))
              }
            >
              加入书单
            </button>
            {booklistView && currentBooklist && (
              <button
                className="text-button"
                data-testid="remove-booklist-members"
                disabled={!booklistsReady || booklistsSaving}
                onClick={async () => {
                  if (
                    await commitBooklists(
                      removeBooklistMembers(
                        booklists,
                        currentBooklist.id,
                        selectedWorks.map(workReference),
                        Date.now(),
                      ),
                    )
                  ) {
                    setSelection([]);
                    setNotice("已移出书单，作品文件和队列未改变");
                  }
                }}
              >
                移出当前书单
              </button>
            )}
            <button
              className="button primary"
              disabled={!chosen.length}
              data-testid="batch-download"
              onClick={() => setConfirmation(chosen.map((work) => work.id))}
            >
              <Icon name="download" size={17} />
              下载并入库
              {chosen.length !== selectedWorks.length && `（${chosen.length}）`}
            </button>
          </div>
        )}
      </>
    );
  }

  function renderDetail(work: Work) {
    const task = taskFor(work.id);
    const badge = workBadge(work);
    return (
      <div data-testid="detail-page">
        <button
          className="text-button back-link"
          data-testid="back-library"
          onClick={backToList}
        >
          <Icon name="back" size={17} />
          返回{pageNames[page]}
        </button>
        <div className="detail-hero">
          <div className="detail-cover">
            <img
              src={work.cover}
              alt={`${work.title}原创示意封面`}
              width="400"
              height="560"
            />
            <span className="cover-caption">ORIGINAL DEMO ARTWORK</span>
          </div>
          <div className="detail-copy">
            <div className="eyebrow">{work.source} · 作品详情 · 示意内容</div>
            <h1>{work.title}</h1>
            <p className="detail-subtitle">{work.subtitle}</p>
            <button
              className="author-link"
              onClick={() => {
                navigate("discovery");
                setFilter("all");
                setSource(work.source);
                setQuery(work.author);
              }}
            >
              {work.author}
              <Icon name="arrow" size={14} />
            </button>
            <div className="tags">
              {work.tags.map((tag) => (
                <span key={tag}>{tag}</span>
              ))}
            </div>
            <p className="description">{work.description}</p>
            <div className="detail-numbers">
              <div>
                <strong>{work.chapters}</strong>
                <span>章节</span>
              </div>
              <div>
                <strong>{work.pages}</strong>
                <span>页</span>
              </div>
              <div>
                <strong>
                  {work.sizeMB}
                  <small> MB</small>
                </strong>
                <span>预计大小 · 示例</span>
              </div>
            </div>
            {work.status === "review" ? (
              <div className="inline-message warning">
                <Icon name="warning" />
                <div>
                  <strong>作者关系需要复核</strong>
                  <p>
                    示例来源存在同名作者，身份尚不确定。确认关系后才能下载。
                  </p>
                </div>
              </div>
            ) : isOwned(work) ? (
              <div className="inline-message success">
                <Icon name="check" />
                <div>
                  <strong>
                    {task?.stage === "sync_pending"
                      ? "已入库，等待同步"
                      : "作品已在示例漫画库"}
                  </strong>
                  <p>
                    漫画库／{work.title}.{work.id === "summer" ? "cbz" : "zip"}
                  </p>
                </div>
              </div>
            ) : null}
            <div className="detail-actions">
              {canSelect(work) ? (
                <button
                  className="button primary"
                  onClick={() => setConfirmation([work.id])}
                >
                  <Icon name="download" size={18} />
                  下载并入库
                </button>
              ) : task ? (
                <button
                  className="button primary"
                  onClick={() => navigate("queue")}
                >
                  查看队列任务
                  <Icon name="arrow" size={17} />
                </button>
              ) : isOwned(work) ? (
                <button
                  className="button secondary"
                  onClick={() => setDetailTab("files")}
                >
                  <Icon name="folder" size={17} />
                  查看文件信息
                </button>
              ) : (
                <button className="button secondary" disabled>
                  <Icon name="clock" size={17} />
                  等待关系复核
                </button>
              )}
              <span className={`status ${badge.tone}`}>
                <i />
                {badge.text}
              </span>
            </div>
            <button
              className="text-button detail-booklist"
              data-testid="detail-booklist"
              disabled={!booklistsReady || booklistsSaving}
              onClick={() => setBooklistPickerMembers([workReference(work)])}
            >
              <Icon name="book" size={16} />
              加入书单
            </button>
            <p className="detail-note">
              一部作品一个 ZIP · 按章节整理 · {work.updated} 更新
            </p>
          </div>
        </div>
        <div className="detail-bottom">
          <section>
            <div className="tabs detail-tabs">
              {[
                ["chapters", `章节 ${work.chapters}`],
                ["files", "本地文件"],
                ["task", "任务动态"],
              ].map(([value, label]) => (
                <button
                  key={value}
                  className={detailTab === value ? "active" : ""}
                  aria-pressed={detailTab === value}
                  onClick={() => setDetailTab(value)}
                >
                  {label}
                </button>
              ))}
            </div>
            {detailTab === "chapters" && (
              <div className="chapter-list">
                <div className="section-note">
                  章节目录示例 · 首版仅展示信息
                </div>
                {Array.from(
                  { length: Math.min(work.chapters, 6) },
                  (_, index) => (
                    <div className="chapter" key={index}>
                      <span className="chapter-number">
                        {String(index + 1).padStart(2, "0")}
                      </span>
                      <div>
                        <strong>
                          第 {index + 1} 章 ·{" "}
                          {
                            [
                              "故事的开始",
                              "意外的相遇",
                              "藏起来的信",
                              "下一站",
                              "远方的回声",
                              "新的约定",
                            ][index]
                          }
                        </strong>
                        <span>
                          {Math.round(work.pages / work.chapters)} 页 · 示意章节
                        </span>
                      </div>
                      <span
                        className={`status ${isOwned(work) ? "success" : "muted"}`}
                      >
                        {isOwned(work) ? "本地已有" : "随作品下载"}
                      </span>
                    </div>
                  ),
                )}
                {work.chapters > 6 && (
                  <p className="section-note">
                    预览展示前 6 章，完整目录由实际来源提供。
                  </p>
                )}
              </div>
            )}
            {detailTab === "files" && (
              <div className="file-info">
                <Icon name="folder" size={28} />
                <h3>{isOwned(work) ? "示例文件信息" : "下载后的文件结构"}</h3>
                <p>
                  漫画库／{work.title}.{work.id === "summer" ? "cbz" : "zip"}
                </p>
                <pre>{`${work.title}.${work.id === "summer" ? "cbz" : "zip"}\n├─ 第001章/\n│  ├─ 001.jpg\n│  └─ 002.jpg\n└─ 第002章/\n   └─ …`}</pre>
                <p className="quiet">
                  此处展示目录约定，未读取或写入电脑上的任何漫画文件。
                </p>
              </div>
            )}
            {detailTab === "task" && (
              <div className="file-info">
                <Icon name="clock" size={28} />
                <h3>{task ? displayStage(task) : "暂无下载任务"}</h3>
                <p>
                  {task
                    ? "此状态来自当前浏览器内的模拟队列。"
                    : "确认下载后，可在这里查看该作品的处理进度。"}
                </p>
                {task && (
                  <button
                    className="button secondary"
                    onClick={() => navigate("queue")}
                  >
                    在下载队列中查看
                    <Icon name="arrow" size={16} />
                  </button>
                )}
              </div>
            )}
          </section>
          <aside className="detail-aside">
            <Icon name="book" size={23} />
            <h3>收藏，从容一点</h3>
            <p>
              确认一次，剩下的交给队列。下载、校验、打包和入库，每一步都清楚可见。
            </p>
            <div className="mini-divider" />
            <span>保存格式</span>
            <strong>ZIP · 一部作品一个文件</strong>
            <span>阅读方式</span>
            <strong>使用你熟悉的外部阅读器</strong>
          </aside>
        </div>
      </div>
    );
  }

  function renderQueue() {
    return (
      <div data-testid="queue-page">
        <div className="page-heading">
          <div>
            <div className="eyebrow">A LITTLE PATIENCE, A NEW STORY</div>
            <h1>
              下载队列 <span className="heading-count">{unfinished}</span>
            </h1>
            <p>从下载到入库，安心交给队列。需要处理的作品会留在这里。</p>
          </div>
          <button
            className="button secondary"
            data-testid="pause-queue"
            onClick={() =>
              setState((previous) => setDemoPaused(previous, !previous.paused))
            }
          >
            <Icon name={state.paused ? "play" : "pause"} size={16} />
            {state.paused ? "继续队列" : "暂停队列"}
          </button>
        </div>
        <div className="queue-overview">
          <div>
            <span className={`activity-dot ${state.paused ? "paused" : ""}`} />
            <strong>
              {state.paused
                ? "队列已暂停"
                : activeTask
                  ? "队列正在工作"
                  : "当前没有执行中的下载"}
            </strong>
            <span>
              {state.paused
                ? "已保存模拟进度，随时继续"
                : "模拟单任务执行 · 自动整理入库"}
            </span>
          </div>
          <span className="quiet">
            {state.online ? "云端连接正常 · 模拟" : "云端离线 · 本地任务可继续"}
          </span>
        </div>
        <div className="queue-layout">
          <section className="queue-main">
            <div className="tabs queue-tabs">
              {[
                ["all", "全部任务"],
                ["active", "进行中"],
                ["error", "需要处理"],
                ["done", "已入库"],
              ].map(([value, text]) => (
                <button
                  key={value}
                  className={queueFilter === value ? "active" : ""}
                  aria-pressed={queueFilter === value}
                  onClick={() => setQueueFilter(value)}
                >
                  {text}
                  {value === "error" &&
                    state.tasks.some((task) => task.stage === "error") && (
                      <span className="tab-dot" />
                    )}
                </button>
              ))}
            </div>
            <div className="task-list">
              {queueTasks.map((task) => {
                const work = lookup(task.workId);
                const status = displayStage(task);
                return (
                  <article
                    className={`task-card ${task.stage === "error" ? "task-error" : ""}`}
                    key={task.id}
                    data-testid={`task-${work.id}`}
                  >
                    <button
                      className="task-cover"
                      onClick={() => openWork(work.id)}
                      aria-label={`查看《${work.title}》详情`}
                    >
                      <img src={work.cover} alt="" width="400" height="560" />
                    </button>
                    <div className="task-content">
                      <div className="task-title-row">
                        <div>
                          <h3>{work.title}</h3>
                          <span className="quiet">
                            {work.author} · {work.chapters} 章 · ZIP
                          </span>
                        </div>
                        <span
                          className={`status ${task.stage === "error" ? "warning" : ["completed", "sync_pending"].includes(task.stage) ? "success" : "muted"}`}
                        >
                          {status}
                        </span>
                      </div>
                      <div
                        className={`progress-track ${task.stage === "error" ? "error" : ""}`}
                        role="progressbar"
                        aria-label={`${work.title} 当前阶段进度`}
                        aria-valuenow={task.progress}
                        aria-valuemin={0}
                        aria-valuemax={100}
                      >
                        <span style={{ width: `${task.progress}%` }} />
                      </div>
                      <div className="task-bottom">
                        <span>
                          {task.stage === "error"
                            ? task.error
                            : task.stage === "sync_pending"
                              ? "文件已在示例库中，恢复联网后只同步状态"
                              : task.stage === "completed"
                                ? `漫画库／${work.title}.zip · 已同步`
                                : `${status} · 当前阶段 ${task.progress}%`}
                        </span>
                        {task.stage === "error" ? (
                          <button
                            className="text-button"
                            onClick={() =>
                              setState((previous) =>
                                retryDemoTask(previous, task.id),
                              )
                            }
                          >
                            <Icon name="refresh" size={14} />
                            重试
                          </button>
                        ) : localStages.includes(task.stage) ? (
                          <button
                            className="icon-button"
                            aria-label={`${task.paused ? "恢复" : "暂停"}《${work.title}》`}
                            onClick={() =>
                              setState((previous) =>
                                toggleTaskPause(previous, task.id),
                              )
                            }
                          >
                            <Icon
                              name={task.paused ? "play" : "pause"}
                              size={16}
                            />
                          </button>
                        ) : (
                          <Icon
                            name={
                              task.stage === "sync_pending" ? "cloud" : "check"
                            }
                            size={16}
                          />
                        )}
                      </div>
                    </div>
                  </article>
                );
              })}
            </div>
            {!queueTasks.length && (
              <div className="empty-state">
                <Icon name="check" size={32} />
                <h2>这里暂时没有任务</h2>
                <p>去漫画库挑选下一部想收藏的故事。</p>
                <button
                  className="button secondary"
                  onClick={() => navigate("library")}
                >
                  浏览漫画库
                </button>
              </div>
            )}
          </section>
          <aside className="queue-aside">
            <section className="destination-panel">
              <Icon name="folder" size={22} />
              <h3>你的作品会保存在</h3>
              <p className="destination">漫画库／作品名.zip</p>
              <span className="quiet">包内按章节分目录</span>
              <div className="mini-divider" />
              <ol className="flow-list">
                <li>下载图片</li>
                <li>校验内容</li>
                <li>保存并验证作品文件</li>
                <li>安全入库</li>
                <li>同步完成状态</li>
              </ol>
              <p className="fine-print">
                正常任务确认一次即可。单本异常单独处理，不影响其他作品。
              </p>
            </section>
            <section className="demo-controls">
              <div className="eyebrow">样例控制</div>
              <p>用模拟状态体验异常与恢复。</p>
              <button
                className="control-row"
                data-testid="demo-offline"
                aria-pressed={!state.online}
                onClick={() =>
                  setState((previous) =>
                    setDemoOnline(previous, !previous.online),
                  )
                }
              >
                <span>
                  <Icon name="cloud" size={16} />
                  模拟云端离线
                </span>
                <span className={`toggle ${!state.online ? "on" : ""}`} />
              </button>
              <button
                className="control-row"
                data-testid="demo-close"
                onClick={() => setState((previous) => closeDemo(previous))}
              >
                <span>
                  <Icon name="pause" size={16} />
                  演示安全退出
                </span>
                <Icon name="arrow" size={14} />
              </button>
              <p className="fine-print">只改变样例，不执行下载或系统操作。</p>
            </section>
          </aside>
        </div>
      </div>
    );
  }

  function renderAuthors() {
    return (
      <>
        <div className="page-heading">
          <div>
            <div className="eyebrow">FOLLOW THE STORYTELLERS</div>
            <h1>关注</h1>
            <p>关注喜欢的创作者，让新故事来到你的漫画库。</p>
          </div>
        </div>
        <div className="inline-message muted">
          <Icon name="discover" />
          <p>这里展示示例作者及其作品。真实关注与云端监控将在后续接入。</p>
        </div>
        <div className="authors-list">
          {works.map((work) => (
            <button
              key={work.id}
              className="author-row"
              onClick={() => {
                setPage("discovery");
                setFilter("all");
                setQuery(work.author);
                setSource(work.source);
                setSelection([]);
              }}
            >
              <div>
                <strong>{work.author}</strong>
                <span>
                  {work.source} · {work.title}
                </span>
              </div>
              <span className="author-check">尚未检查</span>
              <Icon name="arrow" size={18} />
            </button>
          ))}
        </div>
      </>
    );
  }

  function renderSettings() {
    return preferencesReady ? (
      <WorkbenchSettings
        downloadPanel={
          persistence.native ? (
            <DownloadSettingsPanel
              downloads={downloads}
              onOpenQueue={() => navigate("queue")}
            />
          ) : undefined
        }
        libraryPanel={
          persistence.native ? (
            <LibrarySettingsPanel library={library} phone={phoneLibrary} />
          ) : undefined
        }
        accountPanel={
          persistence.native ? (
            <AccountSettings
              adapter={sourceAdapter}
              accounts={accounts}
              onAccountsChange={mergeAccounts}
              onOpenFavorites={openSourceFavorites}
              loadingAccounts={loadingAccounts}
            />
          ) : undefined
        }
        preferences={preferences}
        onSave={commitPreferences}
        saveDisabled={!preferencesSnapshot.current || preferencesSaving}
        storageLabel={persistence.native ? "本机应用数据" : "当前浏览器"}
        onNativeChooseBackground={
          persistence.native ? persistence.chooseBackground : undefined
        }
        onPreview={setAppearanceDraft}
        searchQuery={settingsQuery}
        onBackgroundValidated={(dataUrl) => {
          if (dataUrl === failedBackground) setFailedBackground(null);
        }}
        storageFailed={preferencesFailed || storageFailed}
        onResetDemo={() => {
          setState(initialDemoState());
          setSelection([]);
          setNotice("样例已重置");
        }}
      />
    ) : (
      <p role="status">
        {preferencesError
          ? "设置尚未读入，请使用上方的重新读取按钮。"
          : "正在读取设置…"}
      </p>
    );
  }
  return (
    <div
      className="app-shell"
      data-background-mode={appearance.backgroundMode}
      style={
        {
          "--user-background": displayBackground
            ? `url("${displayBackground}")`
            : "none",
        } as CSSProperties
      }
    >
      <div className="workbench-background" aria-hidden="true" />
      <aside className="sidebar">
        <button
          className="brand"
          aria-label="MangaMonitor 首页"
          onClick={() => navigate("library")}
        >
          <span className="brand-mark">
            M<span />
          </span>
        </button>
        <nav aria-label="主要导航">
          {(
            [
              ["library", "library"],
              ["favorites", "heart"],
              ["discovery", "discover"],
              ["queue", "download"],
              ["authors", "people"],
            ] as const
          ).map(([value, icon]) => (
            <button
              key={value}
              data-testid={`nav-${value}`}
              className={`nav-item ${page === value ? "active" : ""}`}
              aria-current={page === value ? "page" : undefined}
              aria-label={pageNames[value]}
              title={pageNames[value]}
              onClick={() => navigate(value)}
            >
              <Icon name={icon} size={19} />
              <span className="nav-tooltip">{pageNames[value]}</span>
              {value === "queue" &&
                (persistence.native
                  ? unfinishedDownloadCount(downloads.snapshot.tasks)
                  : unfinished) > 0 && (
                  <span className="nav-count">
                    {persistence.native
                      ? unfinishedDownloadCount(downloads.snapshot.tasks)
                      : unfinished}
                  </span>
                )}
              {value === "discovery" && <span className="nav-dot" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button
            className={`nav-item ${page === "settings" ? "active" : ""}`}
            data-testid="nav-settings"
            aria-label="设置"
            title="设置"
            aria-current={page === "settings" ? "page" : undefined}
            onClick={() => navigate("settings")}
          >
            <Icon name="settings" size={19} />
            <span className="nav-tooltip">设置</span>
          </button>
        </div>
      </aside>
      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumb">
            工作台 <span>/</span> {pageNames[page]}
            {detail && (
              <>
                <span>/</span>
                <strong>作品详情</strong>
              </>
            )}
          </div>
          <div
            className="source-search-host"
            ref={setSourceSearchHost}
            hidden={!sourceActive}
          />
          {!sourceActive && (
            <label className="search-box">
              <Icon name="search" size={17} />
              <input
                data-testid="search-input"
                aria-label={page === "settings" ? "搜索设置" : "搜索作品或作者"}
                placeholder={
                  page === "settings" ? "搜索设置…" : "搜索作品、作者…"
                }
                value={page === "settings" ? settingsQuery : query}
                onChange={(event) => {
                  if (page === "settings") {
                    setSettingsQuery(event.target.value);
                    return;
                  }
                  setQuery(event.target.value);
                  clearScopeSelection();
                  setDetail(null);
                  if (!["library", "favorites", "discovery"].includes(page)) {
                    setPage("discovery");
                    setFilter("all");
                    setSource("all");
                  }
                }}
              />
              {(page === "settings" ? settingsQuery : query) && (
                <button
                  className="icon-button"
                  aria-label="清空搜索"
                  onClick={() => {
                    if (page === "settings") setSettingsQuery("");
                    else {
                      setQuery("");
                      clearScopeSelection();
                    }
                  }}
                >
                  <Icon name="close" size={14} />
                </button>
              )}
            </label>
          )}
          <div className="demo-label" data-testid="demo-label">
            <span />
            {persistence.native
              ? sourceActive
                ? "桌面开发版 · 真实来源"
                : page === "library"
                  ? "桌面开发版 · 双库记录"
                  : "桌面开发版 · 本机任务"
              : "交互样例 · 模拟数据"}
            {activeFixture && " · 100 条验收数据"}
          </div>
        </header>
        <main
          className={`content ${selectedWorks.length > 0 && !detail && page !== "settings" ? "has-selection" : ""}`}
          ref={contentRef}
          onScroll={updateToolbarSurface}
          tabIndex={-1}
        >
          {failedBackground &&
            appearance.backgroundImage === failedBackground && (
              <p role="status" className="storage-warning">
                已保存的背景图片暂时无法显示，原设置已保留；请在外观中重新选择图片。
              </p>
            )}
          {preferencesError && (
            <div
              className="storage-warning"
              role="alert"
              data-testid="preferences-error"
            >
              <span>{preferencesError}</span>
              <button
                className="text-button"
                disabled={preferencesSaving}
                data-testid="reload-preferences"
                onClick={() => void loadPreferences()}
              >
                重新读取设置
              </button>
            </div>
          )}
          {booklistsError && (
            <div
              className="storage-warning"
              role="alert"
              data-testid="booklists-error"
            >
              <span>{booklistsError}</span>
              <button
                className="text-button"
                disabled={booklistsSaving}
                data-testid="reload-booklists"
                onClick={() => void loadBooklists()}
              >
                重新读取书单
              </button>
            </div>
          )}
          {storageFailed && (
            <div role="status" className="storage-warning">
              浏览器存储不可用，模拟进度只在本次会话中保留。
            </div>
          )}
          {accountsError && (
            <p role="alert" className="storage-warning">
              {accountsError} 可在设置的账号页重新读取。
            </p>
          )}
          {persistence.native && (
            <LibraryWorkbench
              key={libraryNavigationKey}
              library={library}
              phone={phoneLibrary}
              active={libraryActive}
              density={appearance.density}
              onDensityChange={changeDensity}
              query={query}
              onBooklists={() => {
                setLibraryTab("booklists");
                setQuery("");
              }}
              onAddToBooklists={openSourceBooklistPicker}
              externalWork={requestedLibraryWork}
              requestKey={libraryRequestKey}
            />
          )}
          {persistence.native && (
            <NativeDownloads
              downloads={downloads}
              active={page === "queue"}
              contexts={downloadContexts}
              accounts={accounts}
              selectedSource={downloadSource}
              onSourceChange={(source) => {
                downloads.controller.cancelPlan();
                setDownloadSource(source);
                setDownloadInput("");
              }}
              input={downloadInput}
              onInputChange={setDownloadInput}
              onPrepare={() => beginDownload(downloadInput)}
              onChooseLibrary={chooseDownloadLibrary}
              onOpenAccounts={() => navigate("settings")}
              onConfirmed={() => navigate("queue")}
              onOpenDownloaded={openDownloaded}
              onReprepare={(task) => {
                if (
                  task.phase === "downloaded" &&
                  task.localFiles === "missing"
                )
                  void beginDownload(task.workId, undefined, task.source);
              }}
              showFeedback={downloadFeedback}
            />
          )}
          {persistence.native && (
            <SourceWorkbench
              adapter={sourceAdapter}
              onDownload={(work) => beginDownload(work.workId, work)}
              downloadReady={downloads.ready}
              downloadBusy={downloads.busy}
              librarySnapshot={library.snapshot}
              phoneSnapshot={phoneLibrary.snapshot}
              phoneBusy={phoneLibrary.busy || !phoneLibrary.ready}
              phoneReady={phoneLibrary.ready}
              phoneError={phoneLibrary.error}
              onMarkPhone={(work) =>
                phoneLibrary.mark(work.title, {
                  source: work.source,
                  workId: work.workId,
                })
              }
              onUnmarkPhone={phoneLibrary.unmark}
              onOpenLibrary={(work) => {
                navigate("library");
                setLibraryTab("all");
                setRequestedLibraryWork(work);
                setLibraryRequestKey((key) => key + 1);
                setQuery(work.title);
              }}
              accounts={accounts}
              onAccountsChange={mergeAccounts}
              onOpenAccounts={(source) => {
                setRequestedSource(source);
                navigate("settings");
              }}
              onAddToBooklists={openSourceBooklistPicker}
              onWorksChanged={cacheSourceWorks}
              view={sourceView}
              active={sourceActive}
              density={appearance.density}
              onDensityChange={changeDensity}
              requestedSource={requestedSource}
              requestedWork={requestedWork}
              requestKey={sourceRequestKey}
              loadingAccounts={loadingAccounts}
              searchHost={sourceSearchHost}
            />
          )}
          {sourceActive || libraryActive
            ? null
            : currentWork
              ? renderDetail(currentWork)
              : ["library", "favorites", "discovery"].includes(page)
                ? renderLibrary()
                : page === "queue"
                  ? persistence.native
                    ? null
                    : renderQueue()
                  : page === "authors"
                    ? renderAuthors()
                    : renderSettings()}
        </main>
        <footer className="statusbar">
          <span>
            <span className="statusbar-dot" />
            {persistence.native
              ? downloadStatusText(downloads)
              : state.paused
                ? "模拟队列已暂停"
                : activeTask
                  ? `模拟执行中 · ${lookup(activeTask.workId).title}`
                  : "模拟队列就绪"}
          </span>
          <span>
            {sourceActive
              ? "手机名单与电脑文件核对 · 单本下载"
              : libraryActive
                ? "电脑文件保留 · 手机由你手动转入"
                : persistence.native
                  ? "JM / 哔咔单本下载"
                  : "示例数据 · 尚未连接下载器"}
          </span>
        </footer>
      </div>
      {!persistence.native && confirmation && (
        <Dialog
          title="下载并入库"
          testId="confirm-dialog"
          onClose={() => setConfirmation(null)}
        >
          <p className="dialog-intro">
            确认这 {confirmation.length}{" "}
            部作品，队列会自动完成下载、校验和入库。
          </p>
          <div className="confirmation-list">
            {confirmation.map((id) => {
              const work = lookup(id);
              return (
                <div className="confirmation-work" key={id}>
                  <img src={work.cover} alt="" width="400" height="560" />
                  <div>
                    <strong>{work.title}</strong>
                    <span>漫画库／{work.title}.zip</span>
                  </div>
                  <span>
                    {work.source} · {work.chapters} 章
                  </span>
                </div>
              );
            })}
          </div>
          <div className="confirmation-note">
            <Icon name="folder" size={19} />
            <p>
              每部作品单独保存为 ZIP，内部按章节整理。已有同名文件时暂停处理。
            </p>
          </div>
          <p className="fine-print">
            本次是交互演示，不会发起真实下载或写入文件。
          </p>
          <div className="dialog-actions">
            <button
              className="button secondary"
              onClick={() => setConfirmation(null)}
            >
              取消
            </button>
            <button
              className="button primary"
              data-testid="confirm-download"
              onClick={confirmDownload}
            >
              确认下载并入库
              <Icon name="arrow" size={16} />
            </button>
          </div>
        </Dialog>
      )}
      {booklistPickerMembers && (
        <BooklistPicker
          document={booklists}
          members={booklistPickerMembers}
          disabled={!booklistsReady || booklistsSaving}
          onChange={async (next) => {
            const saved = await commitBooklists(next);
            if (saved) {
              pickerCompletion.current?.(true);
              pickerCompletion.current = null;
            }
            return saved;
          }}
          onReload={loadBooklists}
          reloadDisabled={booklistsSaving}
          onClose={() => closeBooklistPicker(false)}
        />
      )}
      {!persistence.native && state.closed && (
        <Dialog
          title="模拟工作台已安全退出"
          dismissible={false}
          onClose={() => undefined}
        >
          <div className="closed-message">
            <span className="closed-icon">
              <Icon name="check" size={32} />
            </span>
            <h3>
              {storageFailed
                ? "队列已停止，本次会话中可恢复"
                : "进度已保存，故事下次继续"}
            </h3>
            <p>
              {storageFailed
                ? "浏览器存储不可用，刷新或关闭此页面会丢失本次模拟进度。"
                : "样例队列已停止推进。重新打开后恢复已有任务和暂停选择。"}
            </p>
            <p className="fine-print">
              这是退出流程演示。浏览器仍保持打开，未执行真实文件写入。
            </p>
            <button
              className="button primary"
              data-testid="demo-reopen"
              onClick={() => setState((previous) => reopenDemo(previous))}
            >
              <Icon name="play" size={17} />
              重新打开工作台
            </button>
          </div>
        </Dialog>
      )}
      {notice && (
        <div role="status" className="toast">
          <Icon name="check" size={17} />
          {notice}
        </div>
      )}
    </div>
  );
}
