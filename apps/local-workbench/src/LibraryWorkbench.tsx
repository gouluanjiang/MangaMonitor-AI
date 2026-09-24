import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  LibraryAdapter,
  LibraryItem,
  LibrarySnapshot,
} from "./library-types.ts";
import { LibraryController, libraryErrorMessage } from "./library-runtime.ts";
import { filterLibraryItems, normalizeLibraryText } from "./library-model.ts";
import {
  formatTimestamp,
  formatWorkDate,
  readSortPreference,
  writeSortPreference,
} from "./work-dates.ts";
import { getLibraryCoverCache } from "./library-cover-cache.ts";
import { observeCover } from "./cover-visibility.ts";
import type { CoverPriority } from "./cover-scheduler.ts";
import type {
  LibraryCoverLease,
  LibraryCoverResult,
} from "./library-cover-cache.ts";
import { libraryItemStatus, readableLibraryItem } from "./inventory-model.ts";
import type { SourceWork } from "./source-types.ts";
import type { SourceGridHandle, GridAnchor } from "./VirtualSourceGrid.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import "./library-workbench.css";
import {
  libraryFilterLabels,
  libraryFilterMatches,
  librarySorts,
} from "./library-matching.ts";
import type { LibraryFilter, LibrarySort } from "./library-matching.ts";

export function useLibrary(adapter: LibraryAdapter, enabled: boolean) {
  const [controller] = useState(() => new LibraryController(adapter));
  const [state, setState] = useState(() => controller.getState());
  useEffect(() => {
    if (!enabled) return;
    const unsubscribe = controller.subscribe((next) => {
      getLibraryCoverCache(adapter).setScope(
        next.snapshot.rootId,
        next.snapshot.generation,
      );
      setState(next);
    });
    void controller.read();
    return () => {
      unsubscribe();
      controller.dispose();
      getLibraryCoverCache(adapter).clear();
    };
  }, [adapter, controller, enabled]);
  return { ...state, controller };
}
export type LibraryState = ReturnType<typeof useLibrary>;
function LibraryCover({
  adapter,
  snapshot,
  item,
}: {
  adapter: LibraryAdapter;
  snapshot: LibrarySnapshot;
  item: LibraryItem;
}) {
  const root = useRef<HTMLDivElement>(null),
    cache = getLibraryCoverCache(adapter);
  const [result, setResult] = useState<LibraryCoverResult>(),
    [visible, setVisible] = useState(false),
    [retry, setRetry] = useState(0);
  useEffect(() => {
    let disposed = false,
      shown = false,
      lease: LibraryCoverLease | undefined;
    let priority: CoverPriority = "visible";
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    setResult(
      snapshot.rootId
        ? cache.peek(snapshot.rootId, snapshot.generation, item.id)
        : undefined,
    );
    setVisible(false);
    const update = (nextPriority: CoverPriority | null) => {
      const next = nextPriority !== null;
      if (nextPriority) {
        priority = nextPriority;
        lease?.setPriority(priority);
      }
      if (next === shown) return;
      shown = next;
      setVisible(next);
      if (!next) {
        clearTimeout(retryTimer);
        lease?.release();
        lease = undefined;
        return;
      }
      if (!snapshot.rootId || !item.coverAvailable) return;
      const current = cache.acquire(
        snapshot.rootId,
        snapshot.generation,
        item.id,
        () => adapter.cover(snapshot.rootId!, snapshot.generation, item.id),
        priority,
      );
      lease = current;
      void current.promise.then((value) => {
        if (!disposed && shown && lease === current) {
          setResult(value);
          if (value.status === "cancelled")
            retryTimer = setTimeout(() => {
              if (disposed || !shown) return;
              lease?.release();
              lease = undefined;
              shown = false;
              update(priority);
            }, 500);
        }
      });
    };
    const stopObserving = root.current
      ? observeCover(root.current, update)
      : undefined;
    return () => {
      disposed = true;
      clearTimeout(retryTimer);
      lease?.release();
      stopObserving?.();
    };
  }, [
    adapter,
    cache,
    snapshot.rootId,
    snapshot.generation,
    item.id,
    item.coverAvailable,
    retry,
  ]);
  return (
    <div
      className="source-cover"
      ref={root}
      data-testid={"library-cover-" + item.id}
    >
      {visible && result?.status === "ready" ? (
        <img
          src={result.url}
          decoding="async"
          alt=""
          onError={() => {
            if (
              snapshot.rootId &&
              cache.invalidate(
                snapshot.rootId,
                snapshot.generation,
                item.id,
                result.url,
              )
            )
              setResult({ status: "error" });
          }}
        />
      ) : (
        <span>
          {item.state === "unsupported"
            ? "此格式暂不支持预览"
            : !item.coverAvailable
              ? "封面暂不可用"
              : result?.status === "error"
                ? "封面暂时无法读取"
                : visible
                  ? "正在读取封面…"
                  : "封面未读取"}
        </span>
      )}
      {result?.status === "error" && (
        <button
          type="button"
          className="library-cover-retry"
          onKeyDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation();
            if (snapshot.rootId)
              cache.retry(snapshot.rootId, snapshot.generation, item.id);
            setRetry((value) => value + 1);
          }}
        >
          重试封面
        </button>
      )}
    </div>
  );
}
export function LibraryControls({
  library,
  compact = false,
}: {
  library: LibraryState;
  compact?: boolean;
}) {
  const { snapshot, busy, error, controller } = library;
  return (
    <div className={"library-read-controls" + (compact ? " is-compact" : "")}>
      <div className="source-actions">
        {snapshot.rootId && (
          <button
            className="button secondary"
            data-testid="library-refresh"
            disabled={busy}
            onClick={() => void controller.scan("start")}
          >
            刷新漫画库
          </button>
        )}
        <button
          className={snapshot.rootId ? "text-button" : "button secondary"}
          data-testid="library-choose"
          disabled={busy}
          onClick={() => void controller.choose()}
        >
          {snapshot.rootId ? "更换目录" : "选择电脑漫画目录"}
        </button>
        {snapshot.phase === "reading" && !error && (
          <button
            className="text-button"
            data-testid="library-pause"
            onClick={() => void controller.scan("pause")}
          >
            暂停读取
          </button>
        )}
        {snapshot.phase === "paused" && !error && (
          <button
            className="button secondary"
            data-testid="library-resume"
            disabled={busy}
            onClick={() =>
              void controller.scan(
                snapshot.freshness === "cached" ? "start" : "resume",
              )
            }
          >
            {snapshot.freshness === "cached" ? "重新读取目录" : "继续读取"}
          </button>
        )}
        {(error || snapshot.phase === "error") && (
          <button
            className="button secondary"
            data-testid="library-retry"
            disabled={busy}
            onClick={() =>
              void (snapshot.rootId
                ? controller.scan("start")
                : controller.read())
            }
          >
            重试读取
          </button>
        )}
      </div>
      <p
        className="source-muted library-path"
        title={snapshot.rootPath ?? undefined}
        data-testid="library-root-path"
      >
        {compact && snapshot.rootPath
          ? "目录：" + snapshot.rootPath.split(/[\\/]/).filter(Boolean).at(-1)
          : (snapshot.rootPath ?? "尚未选择电脑目录")}
      </p>
      <p className="source-muted" data-testid="library-progress" role="status">
        {snapshot.phase === "reading" && !error
          ? "正在读取"
          : snapshot.phase === "paused"
            ? "读取已暂停"
            : snapshot.phase === "complete"
              ? "目录已读完"
              : error || snapshot.phase === "error"
                ? "读取未完成"
                : "尚未读取"}{" "}
        · {snapshot.items.length} 个电脑作品
        {snapshot.skipped > 0 ? ` · 跳过 ${snapshot.skipped} 项` : ""}
        {snapshot.freshness === "cached" && !compact
          ? " · 上次目录记录，可重新读取以发现增删"
          : ""}
      </p>
      {error && (
        <p className="source-notice" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
export function LibrarySettingsPanel({ library }: { library: LibraryState }) {
  return (
    <section className="settings-card" aria-labelledby="library-title">
      <h2 id="library-title">漫画库</h2>
      <LibraryControls library={library} />
      <p className="settings-help">
        这里浏览电脑中的实际文件。在线页面按该来源的成功下载记录与实际文件标记已入库；未登记的旧漫画不会自动匹配。新下载一本一个
        ZIP，封面仅在本次运行内缓存。
      </p>
      <p className="settings-help">
        整理过文件名或格式后，可以导入整理时生成的路径映射，保留已有下载记录的文件位置，再重新读取目录。
      </p>
      <button
        className="button secondary"
        data-testid="library-import-paths"
        disabled={
          library.busy ||
          !library.snapshot.rootId ||
          library.snapshot.phase === "reading"
        }
        onClick={() => void library.controller.importPaths()}
      >
        导入 ZIP 整理映射
      </button>
      {library.migrationNotice && (
        <p role="status">{library.migrationNotice}</p>
      )}
    </section>
  );
}
function LibraryDetail({
  item,
  library,
  onBack,
}: {
  item: LibraryItem;
  library: LibraryState;
  onBack(): void;
}) {
  const [opening, setOpening] = useState(false);
  const [locationMessage, setLocationMessage] = useState("");
  const request = useRef(0);
  const openingLock = useRef(false);
  useEffect(() => {
    request.current += 1;
    openingLock.current = false;
    setOpening(false);
    setLocationMessage("");
    return () => {
      request.current += 1;
    };
  }, [item.id, library.snapshot.rootId, library.snapshot.generation]);
  async function reveal() {
    const { rootId, generation } = library.snapshot;
    if (!rootId || openingLock.current) return;
    openingLock.current = true;
    setOpening(true);
    setLocationMessage("");
    const current = ++request.current;
    try {
      await library.controller.adapter.reveal(rootId, generation, item.id);
      if (request.current === current)
        setLocationMessage("已请求在文件资源管理器中显示此作品。");
    } catch (cause) {
      if (request.current === current)
        setLocationMessage(libraryErrorMessage(cause));
    } finally {
      if (request.current === current) {
        openingLock.current = false;
        setOpening(false);
      }
    }
  }
  return (
    <div className="source-detail" data-testid="library-detail">
      <button
        className="text-button"
        data-testid="library-detail-back"
        onClick={onBack}
      >
        ← 返回列表
      </button>
      <div className="source-detail-main">
        <LibraryCover
          adapter={library.controller.adapter}
          snapshot={library.snapshot}
          item={item}
        />
        <div className="source-detail-info">
          <p className="source-muted">
            电脑文件 ·{" "}
            {item.format === "directory"
              ? "作品文件夹"
              : item.format.toUpperCase()}
          </p>
          <h1>{item.title}</h1>
          <p>
            {item.authors.length ? item.authors.join("、") : "作者资料未取得"}
          </p>
          <div className="source-tags">
            {item.tags.map((tag) => (
              <span key={tag}>{tag}</span>
            ))}
          </div>
          <dl className="source-facts">
            <div>
              <dt>状态</dt>
              <dd data-testid="library-detail-stock">
                {library.error ? "文件待核对" : libraryItemStatus(item)}
              </dd>
            </div>
            <div>
              <dt>页数</dt>
              <dd>{item.pageCount ?? "未知"}</dd>
            </div>
            <div>
              <dt>读取</dt>
              <dd>
                {item.state === "indexed"
                  ? "目录已识别"
                  : item.state === "unsupported"
                    ? "格式暂不支持"
                    : "当前作品无法读取"}
              </dd>
            </div>
          </dl>
          <p className="source-muted" data-testid="library-added-at">
            入库时间：
            {item.addedAt == null
              ? "历史记录未知"
              : new Date(item.addedAt).toLocaleString()}
          </p>
          <p className="source-muted" data-testid="library-version-updated-at">
            版本更新：
            {formatWorkDate(item.versionUpdatedAt, true) ?? "版本时间未知"}
          </p>
          {(item.state !== "indexed" || item.errorCode) && (
            <p className="source-notice" role="status">
              {libraryErrorMessage(item.errorCode)} 请核对文件后重新读取漫画库。
            </p>
          )}
          <h2>电脑位置</h2>
          <div className="source-actions">
            <button
              className="button primary"
              data-testid="library-reveal"
              disabled={opening || library.busy || !library.snapshot.rootId}
              onClick={() => void reveal()}
            >
              {opening ? "正在打开…" : "打开文件位置"}
            </button>
          </div>
          {locationMessage && (
            <p
              className="source-notice"
              role="status"
              data-testid="library-location-status"
            >
              {locationMessage}
            </p>
          )}
          <p className="library-path">{item.relativePath}</p>
          <p className="source-muted">
            {item.bytes.toLocaleString()} 字节 · 文件保持原样。
          </p>
          {item.description && (
            <>
              <h2>简介</h2>
              <p>{item.description}</p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
export function LibraryWorkbench({
  library,
  active,
  density,
  onDensityChange,
  query,
  externalWork,
  externalEntryId,
  requestKey = 0,
}: {
  library: LibraryState;
  active: boolean;
  density: 5 | 7 | 9;
  onDensityChange(value: 5 | 7 | 9): void | Promise<unknown>;
  query: string;
  externalWork?: SourceWork | null;
  externalEntryId?: string | null;
  requestKey?: number;
}) {
  const [sort, setSort] = useState<LibrarySort>(() =>
      readSortPreference("library", librarySorts, "added-desc"),
    ),
    [filter, setFilter] = useState<LibraryFilter>("all"),
    [detailId, setDetailId] = useState<string | null>(null),
    [densitySaving, setDensitySaving] = useState(false);
  const grid = useRef<SourceGridHandle>(null),
    root = useRef<HTMLDivElement>(null),
    anchor = useRef<GridAnchor | null>(null),
    densityAnchor = useRef<{
      density: 5 | 7 | 9;
      anchor: GridAnchor | null;
    } | null>(null),
    scroll = useRef(0),
    activeRef = useRef(active);
  activeRef.current = active;
  const items = useMemo(
    () => filterLibraryItems(library.snapshot.items, query, sort, filter),
    [library.snapshot.items, query, sort, filter],
  );
  const detail = library.snapshot.items.find((item) => item.id === detailId);
  const searchedItems = useMemo(
    () => filterLibraryItems(library.snapshot.items, query),
    [library.snapshot.items, query],
  );
  useEffect(() => {
    setDetailId(null);
  }, [query, library.snapshot.rootId]);
  useEffect(() => {
    if (!externalWork || requestKey === 0) return;
    setFilter("all");
    const exact = library.snapshot.items.find(
      (item) => item.id === externalEntryId,
    );
    setDetailId(exact?.id ?? null);
  }, [requestKey]);
  useLayoutEffect(() => {
    const pending = densityAnchor.current;
    if (pending?.density === density) {
      // Restore only after the new density commits. Restoring from the save
      // promise can start on the old grid and be canceled by its effect cleanup.
      grid.current?.restore(pending.anchor);
      densityAnchor.current = null;
    }
  }, [density]);
  useLayoutEffect(() => {
    const main = root.current?.closest("main");
    if (!main) return;
    const remember = () => {
      if (activeRef.current) scroll.current = main.scrollTop;
    };
    main.addEventListener("scroll", remember, { passive: true });
    return () => main.removeEventListener("scroll", remember);
  }, []);
  useLayoutEffect(() => {
    if (active) {
      const frame = requestAnimationFrame(() => {
        const main = root.current?.closest("main");
        if (main) main.scrollTop = scroll.current;
      });
      return () => cancelAnimationFrame(frame);
    }
  }, [active]);
  function open(item: LibraryItem) {
    anchor.current = grid.current?.capture(item.id) ?? null;
    setDetailId(item.id);
    root.current?.closest("main")?.scrollTo(0, 0);
  }
  function back() {
    setDetailId(null);
    requestAnimationFrame(() => grid.current?.restore(anchor.current));
  }
  return (
    <div
      ref={root}
      hidden={!active}
      className="library-workbench"
      data-testid="library-workbench"
    >
      {detail ? (
        <LibraryDetail
          key={detail.id}
          item={detail}
          library={library}
          onBack={back}
        />
      ) : (
        <>
          <div className="page-heading library-heading">
            <div>
              <div className="product-name">MangaMonitor</div>
              <h1>漫画库</h1>
              <p>浏览电脑漫画库中的作品与文件信息</p>
            </div>
          </div>
          <div className="library-toolbar">
            <div className="source-density" role="group" aria-label="封面密度">
              封面密度
              {([5, 7, 9] as const).map((value) => (
                <button
                  key={value}
                  aria-pressed={density === value}
                  aria-label={"每行 " + value + " 部"}
                  data-testid={"library-density-" + value}
                  disabled={densitySaving}
                  onClick={() => {
                    if (value === density) return;
                    densityAnchor.current = {
                      density: value,
                      anchor: grid.current?.capture() ?? null,
                    };
                    setDensitySaving(true);
                    void Promise.resolve(onDensityChange(value))
                      .catch(() => {
                        densityAnchor.current = null;
                      })
                      .finally(() => setDensitySaving(false));
                  }}
                >
                  {value}
                </button>
              ))}
            </div>
          </div>
          <>
            <LibraryControls library={library} compact />
            <div
              className="result-filters"
              role="group"
              aria-label="漫画库状态筛选"
            >
              {(Object.keys(libraryFilterLabels) as LibraryFilter[]).map(
                (value) => (
                  <button
                    key={value}
                    data-testid={"library-filter-" + value}
                    aria-pressed={filter === value}
                    onClick={() => {
                      setFilter(value);
                      root.current?.closest("main")?.scrollTo(0, 0);
                    }}
                  >
                    {libraryFilterLabels[value]}{" "}
                    <span>
                      {
                        searchedItems.filter((item) =>
                          libraryFilterMatches(item, value),
                        ).length
                      }
                    </span>
                  </button>
                ),
              )}
            </div>
            <div className="library-list-heading">
              <p>
                {items.length} 个作品{query ? "匹配搜索" : ""}
                {library.error
                  ? " · 文件待核对"
                  : ` · 已入库 ${items.filter(readableLibraryItem).length} · 待核对 ${items.filter((item) => !readableLibraryItem(item)).length}`}
              </p>
              <label>
                排序{" "}
                <select
                  data-testid="library-sort"
                  value={sort}
                  onChange={(event) => {
                    const value = event.target.value as LibrarySort;
                    setSort(value);
                    writeSortPreference("library", value);
                  }}
                >
                  <option value="added-desc">入库时间：从新到旧</option>
                  <option value="added-asc">入库时间：从旧到新</option>
                  <option value="updated-desc">版本更新：从新到旧</option>
                  <option value="updated-asc">版本更新：从旧到新</option>
                  <option value="title">作品标题</option>
                  <option value="modified">文件修改时间</option>
                </select>
              </label>
            </div>
            {sort.startsWith("added-") && (
              <p className="source-muted">
                按首次成功记录到漫画库的时间排序；历史时间未知的作品排在最后。重新读取不会改变入库时间。
              </p>
            )}
            {sort.startsWith("updated-") && (
              <p
                className="source-muted"
                data-testid="library-version-sort-scope"
              >
                按本地下载版本的更新时间排序，版本时间未知的作品排在最后。网站后续更新不会改变本地版本时间。
                {library.snapshot.phase !== "complete"
                  ? "目录尚未读完，排序仅覆盖已读取作品。"
                  : ""}
              </p>
            )}
            {externalWork && (
              <p className="source-notice">
                正在核对 {externalWork.source} · {externalWork.workId}
                。打开对应电脑作品，在详情中确认关联；同标题仍需你确认。
              </p>
            )}
            {!library.snapshot.rootId ? (
              <div className="source-empty" data-testid="library-empty">
                <h2>选择电脑漫画目录</h2>
                <p>
                  读取电脑目录中的 ZIP 与已有作品文件夹，核对作品和来源编号。
                </p>
              </div>
            ) : items.length === 0 ? (
              <p className="source-empty">
                {query || filter !== "all"
                  ? "没有匹配的电脑作品。"
                  : "此目录暂未读到作品，已读取的进度会保留。"}
              </p>
            ) : (
              <VirtualSourceGrid
                ref={grid}
                items={items}
                density={density}
                testId="library-grid"
                itemKey={(item) => item.id}
                renderItem={(item) => (
                  <article
                    className="source-card"
                    data-testid={"library-card-" + item.id}
                    data-library-id={item.id}
                  >
                    <div className="source-card-cover">
                      <div
                        className="library-cover-open"
                        role="button"
                        tabIndex={0}
                        aria-label={"查看《" + item.title + "》电脑详情"}
                        data-testid={"library-open-" + item.id}
                        onClick={() => open(item)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            open(item);
                          }
                        }}
                      >
                        <LibraryCover
                          adapter={library.controller.adapter}
                          snapshot={library.snapshot}
                          item={item}
                        />
                      </div>
                    </div>
                    <h3>
                      <button onClick={() => open(item)}>{item.title}</button>
                    </h3>
                    <p>
                      {item.authors.length
                        ? item.authors.join("、")
                        : item.fileName}
                    </p>
                    <p className="source-card-state">
                      {library.error ? "文件待核对" : libraryItemStatus(item)}
                      {item.state !== "indexed"
                        ? item.state === "unsupported"
                          ? " · 格式暂不支持"
                          : " · 无法读取"
                        : ""}
                      {item.errorCode === "LIBRARY_COVER_ONLY"
                        ? " · 正文未读取"
                        : item.errorCode === "LIBRARY_DOWNLOAD_INCOMPLETE"
                          ? " · 含下载中章节"
                          : ""}
                    </p>
                    <p
                      className="source-card-date"
                      title={
                        formatWorkDate(item.versionUpdatedAt, true) ?? undefined
                      }
                    >
                      版本更新：
                      {formatWorkDate(item.versionUpdatedAt) ?? "版本时间未知"}
                    </p>
                    <p
                      className="source-card-date"
                      title={formatTimestamp(item.addedAt, true) ?? undefined}
                    >
                      入库时间：
                      {formatTimestamp(item.addedAt) ?? "历史记录未知"}
                    </p>
                  </article>
                )}
              />
            )}
          </>
        </>
      )}
    </div>
  );
}
