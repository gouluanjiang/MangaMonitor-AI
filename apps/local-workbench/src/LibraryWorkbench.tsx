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
  LibraryReference,
  LibrarySnapshot,
} from "./library-types.ts";
import { LibraryController, libraryErrorMessage } from "./library-runtime.ts";
import {
  filterLibraryItems,
  normalizeLibraryText,
  parseLibraryReference,
} from "./library-model.ts";
import { getLibraryCoverCache } from "./library-cover-cache.ts";
import type {
  LibraryCoverLease,
  LibraryCoverResult,
} from "./library-cover-cache.ts";
import type {
  PhoneLibraryAdapter,
  PhoneLibrarySnapshot,
} from "./phone-library-types.ts";
import { emptyPhoneLibrary } from "./phone-library-types.ts";
import {
  createPhoneItemMatcher,
  phoneLibraryRows,
  phoneNameKey,
} from "./phone-library-model.ts";
import type { SourceWork } from "./source-types.ts";
import type { SourceGridHandle, GridAnchor } from "./VirtualSourceGrid.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import "./library-workbench.css";

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
export function usePhoneLibrary(
  adapter: PhoneLibraryAdapter,
  enabled: boolean,
) {
  const [snapshot, setSnapshot] = useState(emptyPhoneLibrary);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [ready, setReady] = useState(false);
  const state = useRef(snapshot);
  state.current = snapshot;
  const lock = useRef(false),
    epoch = useRef(0);
  const execute = useCallback(
    async (operation: () => Promise<PhoneLibrarySnapshot | null>) => {
      if (lock.current) return false;
      lock.current = true;
      setBusy(true);
      setError("");
      const token = epoch.current;
      try {
        const next = await operation();
        if (token !== epoch.current) return false;
        if (next) {
          state.current = next;
          setSnapshot(next);
          setReady(true);
        }
        return next !== null;
      } catch (cause) {
        if (token === epoch.current) {
          const code = (cause as { code?: string })?.code;
          setError(
            code === "PHONE_LIBRARY_INVALID_TXT"
              ? "名单格式无法读取，请选择每行一个文件名的 UTF-8 或带 BOM 的 UTF-16 TXT。原名单保留。"
              : code === "PHONE_LIBRARY_EMPTY_TXT"
                ? "TXT 名单为空，原名单保留。"
                : code === "PHONE_LIBRARY_LIMIT_EXCEEDED"
                  ? "名单超过 20,000 条或 8 MiB，原名单保留。"
                  : code === "REVISION_CONFLICT"
                    ? "手机名单已有变化，请重新读取后再操作。原名单保留。"
                    : "手机名单未能保存或读取，原名单保留。请重新读取后重试。",
          );
        }
        return false;
      } finally {
        if (token === epoch.current) {
          lock.current = false;
          setBusy(false);
        }
      }
    },
    [],
  );
  const read = useCallback(
    () => execute(() => adapter.read()),
    [adapter, execute],
  );
  useEffect(() => {
    if (!enabled) return;
    void read();
    return () => {
      epoch.current++;
      lock.current = false;
    };
  }, [enabled, read]);
  return {
    snapshot,
    busy,
    error,
    ready,
    read,
    import: () => execute(() => adapter.import(state.current.revision)),
    mark: (name: string, reference: LibraryReference | null) =>
      execute(() => adapter.mark(state.current.revision, name, reference)),
    unmark: (entryId: string) =>
      execute(() => adapter.unmark(state.current.revision, entryId)),
  };
}
export type PhoneLibraryState = ReturnType<typeof usePhoneLibrary>;

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
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    setResult(undefined);
    setVisible(false);
    const update = (next: boolean) => {
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
              update(true);
            }, 500);
        }
      });
    };
    const observer = new IntersectionObserver(
      (entries) => update(entries.some((entry) => entry.isIntersecting)),
      { root: root.current?.closest("main") ?? null, rootMargin: "120px" },
    );
    if (root.current) observer.observe(root.current);
    return () => {
      disposed = true;
      clearTimeout(retryTimer);
      lease?.release();
      observer.disconnect();
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
          alt=""
          onError={() => {
            if (snapshot.rootId)
              cache.invalidate(snapshot.rootId, snapshot.generation, item.id);
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
export function LibraryControls({ library }: { library: LibraryState }) {
  const { snapshot, busy, error, controller } = library;
  return (
    <div className="library-read-controls">
      <div className="source-actions">
        <button
          className="button secondary"
          data-testid="library-choose"
          disabled={busy}
          onClick={() => void controller.choose()}
        >
          选择电脑漫画目录
        </button>
        {snapshot.rootId && (
          <button
            className="text-button"
            data-testid="library-refresh"
            disabled={busy}
            onClick={() => void controller.scan("start")}
          >
            重新读取电脑目录
          </button>
        )}
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
      <p className="source-muted library-path" data-testid="library-root-path">
        {snapshot.rootPath ?? "尚未选择电脑目录"}
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
        {snapshot.freshness === "cached"
          ? " · 上次目录记录，尚未重新核对文件"
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
export function PhoneLibraryControls({ phone }: { phone: PhoneLibraryState }) {
  return (
    <div className="library-read-controls">
      <div className="source-actions">
        <button
          className="button secondary"
          data-testid="phone-library-import"
          disabled={phone.busy || !phone.ready}
          onClick={() => void phone.import()}
        >
          导入 / 更新手机 TXT 名单
        </button>
        <button
          className="text-button"
          data-testid="phone-library-read"
          disabled={phone.busy}
          onClick={() => void phone.read()}
        >
          重新读取手机名单
        </button>
      </div>
      <p className="source-muted">
        更新导入名单，保留手动标记；不会改动漫画文件。手机中的作品显示“已入库”，仅在电脑上的作品显示“已下载”。
      </p>
      {phone.snapshot.importFileName && (
        <p className="source-muted" data-testid="phone-import-info">
          {phone.snapshot.importFileName} · 导入{" "}
          {phone.snapshot.importedNames.length} 条
        </p>
      )}
      {phone.error && (
        <p role="alert" className="source-notice">
          {phone.error}
        </p>
      )}
    </div>
  );
}
export function LibrarySettingsPanel({
  library,
  phone,
}: {
  library: LibraryState;
  phone: PhoneLibraryState;
}) {
  return (
    <section className="settings-card" aria-labelledby="library-title">
      <h2 id="library-title">电脑与手机漫画库</h2>
      <h3>电脑文件</h3>
      <LibraryControls library={library} />
      <p className="settings-help">
        选择目录后分批读取作品文件夹、ZIP 与 CBZ。RAR
        只列出文件名。只读取文件，封面仅在本次运行内缓存。
      </p>
      <h3>手机名单</h3>
      <PhoneLibraryControls phone={phone} />
      <p className="settings-help">
        你手动将漫画传到手机后，可标记已入库或导入最新
        TXT。电脑文件继续保留，程序不传输、移动或删除文件。
      </p>
    </section>
  );
}
function LibraryDetail({
  item,
  library,
  phone,
  onBack,
  onAddToBooklists,
  externalWork,
}: {
  item: LibraryItem;
  library: LibraryState;
  phone: PhoneLibraryState;
  onBack(): void;
  onAddToBooklists(refs: LibraryReference[]): Promise<boolean>;
  externalWork?: SourceWork | null;
}) {
  const [source, setSource] = useState<"JM" | "Pica">(
    externalWork?.source ?? item.sourceRef?.source ?? "JM",
  );
  const [input, setInput] = useState(
    externalWork?.workId ?? item.sourceRef?.workId ?? "",
  );
  const [notice, setNotice] = useState("");
  const ref = parseLibraryReference(source, input);
  const owned = createPhoneItemMatcher(phone.snapshot)(item) === "owned";
  const marks = phone.snapshot.manualEntries.filter(
    (entry) =>
      (entry.reference === null &&
        phoneNameKey(entry.name) === phoneNameKey(item.fileName)) ||
      (item.sourceRef &&
        entry.reference?.source === item.sourceRef.source &&
        entry.reference?.workId === item.sourceRef.workId),
  );
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
                {!phone.ready
                  ? "电脑文件存在 · 手机待核对"
                  : owned
                    ? "已入库 · 手机名单"
                    : "已下载 · 电脑文件"}
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
          {(item.state !== "indexed" || item.errorCode) && (
            <p className="source-notice" role="status">
              {libraryErrorMessage(item.errorCode)}{" "}
              已下载表示发现电脑文件，不代表内容完整性校验通过。
            </p>
          )}
          <div className="source-actions">
            <button
              className="button secondary"
              data-testid="phone-mark"
              disabled={phone.busy || !phone.ready}
              onClick={() =>
                void phone
                  .mark(item.fileName, item.sourceRef)
                  .then((saved) =>
                    setNotice(
                      saved
                        ? "已标记手机已入库，电脑文件保留。"
                        : "标记未完成。",
                    ),
                  )
              }
            >
              标记手机已入库
            </button>
            {item.sourceRef && (
              <button
                className="button secondary"
                data-testid="library-detail-booklist"
                onClick={() => void onAddToBooklists([item.sourceRef!])}
              >
                加入书单
              </button>
            )}
          </div>
          {marks.map((entry) => (
            <button
              className="text-button"
              key={entry.id}
              data-testid={"phone-unmark-" + entry.id}
              disabled={phone.busy}
              onClick={() => void phone.unmark(entry.id)}
            >
              撤销手动标记
            </button>
          ))}
          {marks.length > 0 && (
            <p className="source-muted">
              撤销只移除手动标记；若导入名单仍有该作品，仍显示已入库。
            </p>
          )}
          <section className="library-link-form">
            <h2>关联来源作品</h2>
            <p className="source-muted">
              标题相似只作为候选。确认来源与编号后，才能与在线收藏准确对应。这里只保存关联，不访问网站。
            </p>
            {item.sourceRef && (
              <p data-testid="library-reference">
                当前：{item.sourceRef.source} · {item.sourceRef.workId}（
                {item.identityEvidence === "manual"
                  ? "手动确认"
                  : item.identityEvidence === "metadata"
                    ? "漫画信息文件"
                    : "文件名编号"}
                ）
              </p>
            )}
            <div className="source-actions">
              <select
                aria-label="关联来源"
                data-testid="library-source"
                value={source}
                onChange={(event) =>
                  setSource(event.target.value as "JM" | "Pica")
                }
              >
                <option>JM</option>
                <option>Pica</option>
              </select>
              <input
                aria-label="来源作品编号"
                data-testid="library-source-id"
                value={input}
                onChange={(event) => setInput(event.target.value)}
                placeholder={source === "JM" ? "JM 编号" : "24 位 Pica 编号"}
              />
              <button
                className="button secondary"
                data-testid="library-link"
                disabled={!ref || library.busy}
                onClick={() => {
                  if (ref) void library.controller.link(item.id, ref);
                }}
              >
                确认关联
              </button>
              {item.sourceRef && (
                <button
                  className="text-button"
                  data-testid="library-unlink"
                  disabled={library.busy}
                  onClick={() => void library.controller.link(item.id, null)}
                >
                  取消来源关联
                </button>
              )}
            </div>
            {input && !ref && (
              <p role="status" className="source-muted">
                请输入有效的 {source} 作品编号。
              </p>
            )}
          </section>
          {(notice || library.error || phone.error) && (
            <p role="status" className="source-notice">
              {library.error || phone.error || notice}
            </p>
          )}
          <h2>电脑位置</h2>
          <p className="library-path">{item.relativePath}</p>
          <p className="source-muted">
            {item.bytes.toLocaleString()} 字节 ·
            文件保持原样。转入手机后电脑文件继续保留。
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
  phone,
  active,
  density,
  onDensityChange,
  query,
  onBooklists,
  onAddToBooklists,
  externalWork,
  requestKey = 0,
}: {
  library: LibraryState;
  phone: PhoneLibraryState;
  active: boolean;
  density: 5 | 7 | 9;
  onDensityChange(value: 5 | 7 | 9): void | Promise<unknown>;
  query: string;
  onBooklists(): void;
  onAddToBooklists(refs: LibraryReference[]): Promise<boolean>;
  externalWork?: SourceWork | null;
  requestKey?: number;
}) {
  const [tab, setTab] = useState<"pc" | "phone">("pc"),
    [sort, setSort] = useState<"title" | "modified">("title"),
    [detailId, setDetailId] = useState<string | null>(null);
  const grid = useRef<SourceGridHandle>(null),
    root = useRef<HTMLDivElement>(null),
    anchor = useRef<GridAnchor | null>(null),
    scroll = useRef(0),
    activeRef = useRef(active);
  activeRef.current = active;
  const items = useMemo(
    () => filterLibraryItems(library.snapshot.items, query, sort),
    [library.snapshot.items, query, sort],
  );
  const phoneItems = useMemo(
    () =>
      phoneLibraryRows(phone.snapshot)
        .filter((row) =>
          normalizeLibraryText(row.name).includes(normalizeLibraryText(query)),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [phone.snapshot, query],
  );
  const itemStatus = useMemo(
    () => createPhoneItemMatcher(phone.snapshot),
    [phone.snapshot],
  );
  const detail = library.snapshot.items.find((item) => item.id === detailId);
  useEffect(() => {
    setDetailId(null);
  }, [query, library.snapshot.rootId]);
  useEffect(() => {
    if (!externalWork || requestKey === 0) return;
    setTab("pc");
    const exact = library.snapshot.items.find(
      (item) =>
        item.sourceRef?.source === externalWork.source &&
        item.sourceRef?.workId === externalWork.workId,
    );
    setDetailId(exact?.id ?? null);
  }, [requestKey]);
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
          phone={phone}
          onBack={back}
          onAddToBooklists={onAddToBooklists}
          externalWork={externalWork}
        />
      ) : (
        <>
          <div className="page-heading library-heading">
            <div>
              <div className="product-name">MangaMonitor</div>
              <h1>漫画库</h1>
              <p>手机已入库 · 电脑已下载</p>
            </div>
          </div>
          <div className="library-toolbar">
            <div className="tabs" aria-label="漫画库范围">
              <button
                data-testid="pc-tab"
                className={tab === "pc" ? "active" : ""}
                aria-pressed={tab === "pc"}
                onClick={() => setTab("pc")}
              >
                电脑文件
              </button>
              <button
                data-testid="phone-tab"
                className={tab === "phone" ? "active" : ""}
                aria-pressed={tab === "phone"}
                onClick={() => setTab("phone")}
              >
                手机名单
              </button>
              <button onClick={onBooklists}>本地书单</button>
            </div>
            <div className="source-density" role="group" aria-label="封面密度">
              封面密度
              {([5, 7, 9] as const).map((value) => (
                <button
                  key={value}
                  aria-pressed={density === value}
                  aria-label={"每行 " + value + " 部"}
                  data-testid={"library-density-" + value}
                  onClick={() => {
                    const saved = grid.current?.capture() ?? null;
                    void Promise.resolve(onDensityChange(value)).then(() =>
                      grid.current?.restore(saved),
                    );
                  }}
                >
                  {value}
                </button>
              ))}
            </div>
          </div>
          {tab === "pc" ? (
            <>
              <LibraryControls library={library} />
              <div className="library-list-heading">
                <p>
                  {items.length} 个作品{query ? "匹配搜索" : ""}
                  {phone.ready
                    ? ` · 已入库 ${items.filter((item) => itemStatus(item) === "owned").length} · 已下载 ${items.filter((item) => itemStatus(item) === "downloaded").length}`
                    : " · 手机名单未读取，状态待核对"}
                </p>
                <label>
                  排序{" "}
                  <select
                    data-testid="library-sort"
                    value={sort}
                    onChange={(event) =>
                      setSort(event.target.value as "title" | "modified")
                    }
                  >
                    <option value="title">作品标题</option>
                    <option value="modified">文件修改时间</option>
                  </select>
                </label>
              </div>
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
                    读取现有作品文件夹、ZIP 和 CBZ。也可以先打开手机名单，导入
                    TXT 查看已入库作品。
                  </p>
                </div>
              ) : items.length === 0 ? (
                <p className="source-empty">
                  {query
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
                        {!phone.ready
                          ? "电脑文件存在 · 手机待核对"
                          : itemStatus(item) === "owned"
                            ? "已入库 · 手机名单"
                            : "已下载 · 电脑文件"}
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
                    </article>
                  )}
                />
              )}
            </>
          ) : (
            <>
              <PhoneLibraryControls phone={phone} />
              <p className="source-muted" data-testid="phone-library-count">
                已入库 {phoneItems.length} 部 · 名单记录，无需连接手机
              </p>
              {phoneItems.length === 0 ? (
                <div className="source-empty">
                  <h2>{query ? "没有匹配的手机作品" : "导入手机名单"}</h2>
                  <p>
                    导入文件名 TXT
                    或在作品详情中手动标记已入库。这里不读取手机漫画图片。
                  </p>
                </div>
              ) : (
                <VirtualSourceGrid
                  ref={grid}
                  items={phoneItems}
                  density={density}
                  testId="phone-library-grid"
                  itemKey={(row) => row.id}
                  renderItem={(row) => (
                    <article
                      className="phone-library-card"
                      data-testid={"phone-row-" + row.id}
                    >
                      <span className="phone-library-state">已入库</span>
                      <h3>{row.name}</h3>
                      <p>
                        {row.imported ? "手机 TXT 名单" : "手动确认"}
                        {row.imported && row.manualEntries.length
                          ? " · 含手动标记"
                          : ""}
                      </p>
                      {row.manualEntries.map((entry) => (
                        <button
                          className="text-button"
                          key={entry.id}
                          disabled={phone.busy}
                          data-testid={"phone-unmark-" + entry.id}
                          onClick={() => void phone.unmark(entry.id)}
                        >
                          撤销手动标记
                          {entry.reference
                            ? ` · ${entry.reference.source} ${entry.reference.workId}`
                            : ""}
                        </button>
                      ))}
                    </article>
                  )}
                />
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
