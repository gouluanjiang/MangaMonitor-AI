import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { works } from "./catalog.ts";
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

const STORAGE_KEY = "mangamonitor.workbench.demo.v1";
const labels: Record<TaskStage, string> = {
  queued: "等待下载",
  downloading: "正在下载",
  verifying: "校验图片",
  packing: "生成 ZIP",
  importing: "校验 ZIP 并入库",
  sync_pending: "已入库，等待同步",
  completed: "已完成",
  error: "需要处理",
};
type Page = "library" | "discovery" | "queue" | "authors" | "settings";
type Filter = "all" | "owned" | "ready" | "review";
const pageNames: Record<Page, string> = {
  library: "漫画库",
  discovery: "发现与复核",
  queue: "下载队列",
  authors: "作者与监控",
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
  const [state, setState] = useState(readSavedDemo);
  const [page, setPage] = useState<Page>("library");
  const [detail, setDetail] = useState<string | null>(null);
  const [detailTab, setDetailTab] = useState("chapters");
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("all");
  const [sort, setSort] = useState("updated");
  const [selection, setSelection] = useState<string[]>([]);
  const [confirmation, setConfirmation] = useState<string[] | null>(null);
  const [queueFilter, setQueueFilter] = useState("all");
  const [notice, setNotice] = useState("");
  const [storageFailed, setStorageFailed] = useState(false);
  const contentRef = useRef<HTMLElement>(null);
  const savedScroll = useRef(0);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
      setStorageFailed(false);
    } catch {
      setStorageFailed(true);
    }
  }, [state]);
  useEffect(() => {
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
    setPage(next);
    setDetail(null);
    setSelection([]);
    setQuery("");
    setSource("all");
    setFilter(next === "discovery" ? "ready" : "all");
    contentRef.current?.scrollTo(0, 0);
  };
  const openWork = (id: string) => {
    savedScroll.current = contentRef.current?.scrollTop ?? 0;
    setDetail(id);
    setDetailTab("chapters");
    contentRef.current?.scrollTo(0, 0);
  };
  const backToList = () => {
    setDetail(null);
    window.requestAnimationFrame(() =>
      contentRef.current?.scrollTo(0, savedScroll.current),
    );
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
      return match && filtered && (source === "all" || work.source === source);
    })
    .sort((a, b) =>
      sort === "title"
        ? a.title.localeCompare(b.title, "zh-CN")
        : b.updated.localeCompare(a.updated),
    );
  const chosen = selection.map(lookup).filter(canSelect);
  const currentWork = detail ? lookup(detail) : null;
  const queueTasks = state.tasks.filter(
    (task) =>
      queueFilter === "all" ||
      (queueFilter === "active" && localStages.includes(task.stage)) ||
      (queueFilter === "error" && task.stage === "error") ||
      (queueFilter === "done" &&
        ["sync_pending", "completed"].includes(task.stage)),
  );

  function renderLibrary() {
    return (
      <>
        <div className="page-heading">
          <div>
            <div className="eyebrow">YOUR PERSONAL COLLECTION</div>
            <h1>{pageNames[page]}</h1>
            <p>好故事，慢慢收藏。让每一部作品都有自己的位置。</p>
          </div>
          <div className="collection-count">
            <strong>{String(works.length).padStart(2, "0")}</strong>
            <span>部作品 · {ownedCount} 部已入库</span>
          </div>
        </div>
        <div className="library-summary">
          <div>
            <span className="summary-line" />
            <span>你的漫画角落</span>
            <span className="quiet">／</span>
            <span className="quiet">ZIP 收藏 · 按作品整理</span>
          </div>
          <button className="text-button" onClick={() => navigate("queue")}>
            查看下载队列 <span className="small-count">{unfinished}</span>
            <Icon name="arrow" size={16} />
          </button>
        </div>
        <div className="library-toolbar">
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
                  setSelection([]);
                }}
                aria-pressed={filter === value}
              >
                {text}
                {value === "review" && <span className="tab-dot" />}
              </button>
            ))}
          </div>
          <div className="toolbar-selects">
            <label className="sr-only" htmlFor="source-filter">
              来源筛选
            </label>
            <select
              id="source-filter"
              value={source}
              onChange={(event) => setSource(event.target.value)}
            >
              <option value="all">全部来源</option>
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
              <option value="updated">最近更新</option>
              <option value="title">作品名称</option>
            </select>
            <span className="view-icon">
              <Icon name="grid" size={17} />
            </span>
          </div>
        </div>
        <div className="results-heading">
          <span>
            共 {visibleWorks.length} 部作品{query && ` · 搜索“${query}”`}
          </span>
          <button
            className="text-button quiet"
            onClick={() =>
              setSelection(
                visibleWorks.filter(canSelect).map((work) => work.id),
              )
            }
            disabled={!visibleWorks.some(canSelect)}
          >
            选择可下载作品
          </button>
        </div>
        {visibleWorks.length ? (
          <div className="cover-grid">
            {visibleWorks.map((work) => {
              const badge = workBadge(work);
              return (
                <article
                  className={`work-card ${selection.includes(work.id) ? "selected" : ""}`}
                  key={work.id}
                  data-testid={`card-${work.id}`}
                >
                  <div className="cover-wrap">
                    <button
                      className="cover-button"
                      onClick={() => openWork(work.id)}
                      data-testid={`open-${work.id}`}
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
                    {work.status !== "review" && (
                      <label
                        className="card-select"
                        title={
                          canSelect(work)
                            ? "选择作品"
                            : isOwned(work)
                              ? "已入库"
                              : "已在队列中"
                        }
                      >
                        <input
                          type="checkbox"
                          aria-label={`选择《${work.title}》`}
                          data-testid={`select-${work.id}`}
                          checked={selection.includes(work.id)}
                          disabled={!canSelect(work)}
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
                    <span>{work.finished ? "已完结" : "连载中"}</span>
                  </div>
                </article>
              );
            })}
          </div>
        ) : (
          <div className="empty-state">
            <Icon name="search" size={32} />
            <h2>没有找到这部作品</h2>
            <p>试试作品名、作者，或换一个筛选条件。</p>
            <button
              className="button secondary"
              onClick={() => {
                setQuery("");
                setFilter("all");
                setSource("all");
              }}
            >
              清空筛选
            </button>
          </div>
        )}
        <div className="library-footnote">
          <span>所有封面与作品均为原创示意内容</span>
          <span>8 部作品，8 个小小的世界</span>
        </div>
        {chosen.length > 0 && (
          <div className="selection-bar">
            <span className="selected-count">{chosen.length}</span>
            <div>
              <strong>部作品已选择</strong>
              <small>一次确认，自动下载并整理入库</small>
            </div>
            <button className="text-button" onClick={() => setSelection([])}>
              取消选择
            </button>
            <button
              className="button primary"
              data-testid="batch-download"
              onClick={() => setConfirmation(chosen.map((work) => work.id))}
            >
              <Icon name="download" size={17} />
              下载并入库
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
                navigate("library");
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
              <span>{work.finished ? "已完结" : "连载中"}</span>
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
                <li>生成并验证 ZIP</li>
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
            <h1>作者与监控</h1>
            <p>关注喜欢的创作者，让新故事来到你的漫画库。</p>
          </div>
        </div>
        <div className="inline-message muted">
          <Icon name="discover" />
          <p>这里展示示例作者及其作品。真实关注与云端监控将在后续接入。</p>
        </div>
        <div className="authors-list">
          {works.map((work, index) => (
            <button
              key={work.id}
              className="author-row"
              onClick={() => {
                setPage("library");
                setFilter("all");
                setQuery(work.author);
              }}
            >
              <span
                className="author-avatar"
                style={{ background: work.accent }}
              >
                {work.author.slice(0, 1)}
              </span>
              <div>
                <strong>{work.author}</strong>
                <span>
                  {work.source} · {work.title}
                </span>
              </div>
              <span className="author-index">
                {String(index + 1).padStart(2, "0")}
              </span>
              <Icon name="arrow" size={18} />
            </button>
          ))}
        </div>
      </>
    );
  }

  function renderSettings() {
    return (
      <>
        <div className="page-heading">
          <div>
            <div className="eyebrow">MAKE ROOM FOR YOUR COLLECTION</div>
            <h1>设置</h1>
            <p>先把习惯确定下来，再接上真实的漫画库。</p>
          </div>
        </div>
        <section className="settings-section">
          <h2>已确认的收藏方式</h2>
          {[
            ["新增作品格式", "ZIP 压缩包"],
            ["打包方式", "一部作品一个文件，包内按章节分目录"],
            ["保存位置", "漫画库／作品名.zip"],
            ["确认下载之后", "有空闲名额时自动执行"],
            ["关闭应用窗口", "安全暂停并退出，下次打开恢复"],
            ["阅读功能", "首版不内置阅读器"],
          ].map(([key, value]) => (
            <div className="setting-row" key={key}>
              <span>{key}</span>
              <strong>{value}</strong>
            </div>
          ))}
        </section>
        <section className="settings-section">
          <h2>关于这个样例</h2>
          <p className="settings-description">
            这是本地工作台的前端交互样例。作品、封面、文件路径和任务状态均为示意内容。队列仅保存在当前浏览器中，还没有连接
            GitHub、Rust 下载器或你的本地漫画库。
          </p>
          <div className="setting-row">
            <span>模拟数据保存</span>
            <strong className={storageFailed ? "warning" : "success"}>
              {storageFailed
                ? "浏览器存储不可用，仅本次会话有效"
                : "当前浏览器 · 可刷新恢复"}
            </strong>
          </div>
          <div className="setting-row">
            <div>
              <span>重新体验</span>
              <p className="fine-print">
                重置模拟队列和页面选择，保留所有真实文件。
              </p>
            </div>
            <button
              className="button secondary"
              onClick={() => {
                setState(initialDemoState());
                setSelection([]);
                setNotice("样例已重置");
              }}
            >
              <Icon name="refresh" size={16} />
              重置样例
            </button>
          </div>
        </section>
      </>
    );
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <button
          className="brand"
          aria-label="MangaMonitor 首页"
          onClick={() => navigate("library")}
        >
          <span className="brand-mark">
            M<span />
          </span>
          <span>
            MangaMonitor<small>你的私人漫画工作台</small>
          </span>
        </button>
        <div className="workspace-label">
          个人工作台<span>V1</span>
        </div>
        <nav aria-label="主要导航">
          {(
            [
              ["library", "library"],
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
              onClick={() => navigate(value)}
            >
              <Icon name={icon} size={19} />
              <span>{pageNames[value]}</span>
              {value === "queue" && unfinished > 0 && (
                <span className="nav-count">{unfinished}</span>
              )}
              {value === "discovery" && <span className="nav-dot" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <span className="tiny-kicker">A SHELF OF STORIES</span>
          <p>
            留一点空间，
            <br />
            给下一个好故事。
          </p>
          <div className="little-shelf">
            <i />
            <i />
            <i />
            <i />
            <i />
          </div>
        </div>
        <div className="sidebar-bottom">
          <button
            className={`nav-item ${page === "settings" ? "active" : ""}`}
            data-testid="nav-settings"
            onClick={() => navigate("settings")}
          >
            <Icon name="settings" size={19} />
            <span>设置</span>
          </button>
          <div className="profile">
            <span className="profile-avatar">私</span>
            <div>
              <strong>我的漫画库</strong>
              <span>本地收藏 · 工作台预览</span>
            </div>
            <span className="profile-dot" />
          </div>
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
          <label className="search-box">
            <Icon name="search" size={17} />
            <input
              data-testid="search-input"
              aria-label="搜索作品或作者"
              placeholder="搜索作品、作者…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                if (detail || !["library", "discovery"].includes(page)) {
                  setDetail(null);
                  setPage("library");
                  setFilter("all");
                }
              }}
            />
            {query && (
              <button
                className="icon-button"
                aria-label="清空搜索"
                onClick={() => setQuery("")}
              >
                <Icon name="close" size={14} />
              </button>
            )}
          </label>
          <div className="demo-label" data-testid="demo-label">
            <span />
            交互样例 · 模拟数据
          </div>
        </header>
        <main className="content" ref={contentRef} tabIndex={-1}>
          {storageFailed && (
            <div role="status" className="storage-warning">
              浏览器存储不可用，模拟进度只在本次会话中保留。
            </div>
          )}
          {currentWork
            ? renderDetail(currentWork)
            : ["library", "discovery"].includes(page)
              ? renderLibrary()
              : page === "queue"
                ? renderQueue()
                : page === "authors"
                  ? renderAuthors()
                  : renderSettings()}
        </main>
        <footer className="statusbar">
          <span>
            <span className="statusbar-dot" />
            {state.paused
              ? "模拟队列已暂停"
              : activeTask
                ? `模拟执行中 · ${lookup(activeTask.workId).title}`
                : "模拟队列就绪"}
          </span>
          <span>原创示意作品 · 不访问真实漫画或本地文件</span>
        </footer>
      </div>
      {confirmation && (
        <Dialog
          title="下载并入库"
          testId="confirm-dialog"
          onClose={() => setConfirmation(null)}
        >
          <p className="dialog-intro">
            确认这 {confirmation.length} 部作品，队列会自动完成下载、校验、ZIP
            打包和入库。
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
                  <span>{work.chapters} 章</span>
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
      {state.closed && (
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
