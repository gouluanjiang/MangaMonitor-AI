import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import type {
  DownloadAdapter,
  DownloadContexts,
  DownloadSource,
  DownloadTask,
} from "./download-types.ts";
import {
  DownloadAttentionTracker,
  DownloadAttentionAggregator,
  type DownloadAttentionNotice,
} from "./download-attention.ts";
import {
  isContentHidden,
  rememberContentWork,
  subscribeContentFilter,
  getContentFilterRevision,
} from "./content-filter.ts";
import { downloadSubmissionKey } from "./download-input.ts";
import { browseScope } from "./browse-session.ts";
import { useBrowseSession, useBrowseSessionState } from "./useBrowseSession.ts";
import {
  DownloadController,
  downloadErrorMessage,
  downloadPhaseLabel,
  downloadNeedsAttention,
  downloadTaskLabel,
  filterDownloadTasks,
  isDownloadPresent,
  getDownloadScope,
  canControlDownload,
  downloadQueueFilters,
  downloadQueueSummary,
  downloadCompletedAt,
  downloadAttentionReason,
  downloadBatchProgress,
  type DownloadQueueFilter,
} from "./download-runtime.ts";
import { sourceLabel } from "./source-types.ts";
import type { AccountSummary } from "./source-types.ts";
import { Icon } from "./icons.tsx";
import "./native-downloads.css";

export function useDownloads(
  adapter: DownloadAdapter,
  enabled: boolean,
  contexts: DownloadContexts,
  onDownloaded: () => void,
  onAttention?: (notice: DownloadAttentionNotice) => void,
) {
  const [controller] = useState(() => new DownloadController(adapter));
  const [state, setState] = useState(() => controller.getState());
  const completed = useRef<Set<string> | null>(null),
    callback = useRef(onDownloaded);
  callback.current = onDownloaded;
  const attentionCallback = useRef(onAttention);
  attentionCallback.current = onAttention;
  const contextIdentity = JSON.stringify([contexts.JM, contexts.Pica]);
  useEffect(() => {
    controller.cancelPlan();
  }, [controller, contextIdentity]);
  useEffect(() => {
    if (!enabled) return;
    const tracker = new DownloadAttentionTracker();
    const aggregator = new DownloadAttentionAggregator((notice) =>
      attentionCallback.current?.(notice),
    );
    if (controller.getState().ready)
      tracker.observe(controller.getState().snapshot);
    const unsubscribe = controller.subscribe((next) => {
      if (next.ready) aggregator.push(tracker.observe(next.snapshot));
      setState(next);
    });
    void controller.read();
    return () => {
      unsubscribe();
      aggregator.dispose();
      controller.dispose();
    };
  }, [controller, enabled]);
  useEffect(() => {
    if (!state.ready) return;
    const next = new Set(
      state.snapshot.tasks
        .filter((task) => task.phase === "downloaded")
        .map((task) => task.id + ":" + task.libraryEntryId),
    );
    if (
      completed.current &&
      state.snapshot.tasks.some(
        (task) =>
          isDownloadPresent(task) &&
          !completed.current!.has(task.id + ":" + task.libraryEntryId),
      )
    )
      callback.current();
    completed.current = next;
  }, [state.ready, state.snapshot]);
  return { ...state, controller };
}
export type DownloadsState = ReturnType<typeof useDownloads>;
export const unfinishedDownloadCount = (tasks: DownloadTask[]) =>
  tasks.filter((task) => !isContentHidden(task) && !isDownloadPresent(task))
    .length;
export function downloadStatusText(
  downloads: Pick<DownloadsState, "ready" | "snapshot" | "error">,
) {
  if (!downloads.ready) return "下载队列尚未读取";
  if (downloads.error) return "下载状态待确认";
  const visible = downloads.snapshot.tasks.filter(
    (task) => !isContentHidden(task),
  );
  const running =
    visible.find((task) =>
      ["downloading", "verifying", "saving"].includes(task.phase),
    ) ?? visible.find((task) => task.phase === "queued");
  if (running) return `${downloadPhaseLabel(running.phase)} · ${running.title}`;
  return visible.some((task) => task.phase === "paused")
    ? "下载已暂停"
    : visible.some(downloadNeedsAttention)
      ? "下载任务需要处理"
      : "下载队列就绪";
}
function HistoryConfirmation({
  downloads,
  tasks,
  onClose,
}: {
  downloads: DownloadsState;
  tasks: DownloadTask[];
  onClose(): void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="dialog download-confirmation"
      data-testid="download-history-confirmation"
      aria-label="整理下载历史"
      onCancel={(event) => {
        event.preventDefault();
        if (!downloads.busy) onClose();
      }}
    >
      <div className="dialog-heading">
        <h2>整理下载历史</h2>
      </div>
      <p>
        从队列移除以下 {tasks.length} 条完成记录。漫画文件和漫画库索引会保留。
      </p>
      {downloads.error && (
        <p role="alert" className="source-notice">
          {downloads.error}
        </p>
      )}
      <ul className="download-batch-preview">
        {tasks.map((task) => (
          <li key={task.id}>
            {task.title} · {sourceLabel(task.source)} {task.workId}
          </li>
        ))}
      </ul>
      <div className="dialog-actions">
        <button
          className="button secondary"
          disabled={downloads.busy}
          onClick={onClose}
        >
          取消
        </button>
        <button
          className="button primary"
          data-testid="download-history-confirm"
          disabled={downloads.busy}
          onClick={() =>
            void downloads.controller.removeHistory(tasks).then((done) => {
              if (done) onClose();
            })
          }
        >
          移除记录，保留文件
        </button>
      </div>
    </dialog>
  );
}
export function DownloadSettingsPanel({
  downloads,
  onOpenQueue,
}: {
  downloads: DownloadsState;
  onOpenQueue(): void;
}) {
  return (
    <section className="settings-card" aria-labelledby="native-download-title">
      <h2 id="native-download-title">下载队列</h2>
      <p className="settings-copy">
        从 JM 或哔咔来源详情进入，也可以多选收藏或每行粘贴一个编号。一次最多选择
        500 本，核对后依次下载。
      </p>
      <dl className="settings-facts">
        <div>
          <dt>当前执行方式</dt>
          <dd>多本依次下载，同一时间处理一本</dd>
        </div>
        <div>
          <dt>保存格式</dt>
          <dd>一本一个 ZIP，包含元数据、章节目录和图片</dd>
        </div>
        <div>
          <dt>关闭再打开</dt>
          <dd>恢复任务记录，未完成任务暂停，点击继续后执行</dd>
        </div>
        <div>
          <dt>已入库</dt>
          <dd>以电脑漫画库中的实际作品文件确认</dd>
        </div>
        <div>
          <dt>下载临时文件</dt>
          <dd>完成后清理下载临时文件，电脑作品副本继续保留。</dd>
        </div>
      </dl>
      <p role="status" className="settings-help">
        {downloadStatusText(downloads)}
      </p>
      <button className="button secondary" onClick={onOpenQueue}>
        打开下载队列
      </button>
      <p className="settings-help">
        当前同时处理一本。之前保存的自定义资源偏好保留，暂不改变这一路径的并发数。
      </p>
    </section>
  );
}
export function NativeDownloads({
  downloads,
  active,
  attentionRequestKey = 0,
  contexts,
  accounts,
  selectedSource,
  onSourceChange,
  input,
  onInputChange,
  onPrepare,
  onChooseLibrary,
  onOpenAccounts,
  onOpenDownloaded,
  onReprepare,
}: {
  downloads: DownloadsState;
  active: boolean;
  attentionRequestKey?: number;
  contexts: DownloadContexts;
  accounts: AccountSummary[];
  selectedSource: DownloadSource;
  onSourceChange(source: DownloadSource): void;
  input: string;
  onInputChange(value: string): void;
  onPrepare(): void;
  onChooseLibrary(): void;
  onOpenAccounts(): void;
  onOpenDownloaded(task: DownloadTask): void;
  onReprepare(task: DownloadTask): void;
}) {
  const [filter, setFilter] = useBrowseSessionState<DownloadQueueFilter>(
    "downloads/filter",
    "active",
  );
  const [query, setQuery] = useBrowseSessionState("downloads/query", "");
  const [queueSource, setQueueSource] = useBrowseSessionState<
    DownloadSource | "all"
  >("downloads/source", "all");
  useEffect(() => {
    if (!attentionRequestKey) return;
    setFilter("error");
    setQuery("");
    setQueueSource("all");
  }, [attentionRequestKey, setFilter, setQuery, setQueueSource]);
  const root = useRef<HTMLDivElement>(null);
  const [historySelection, setHistorySelection] = useState<
    DownloadTask[] | null
  >(null);
  const scope = getDownloadScope(accounts, selectedSource);
  const context = contexts[selectedSource];
  useEffect(() => {
    if (!active) return;
    void downloads.controller.read(true);
    const recheck = () => void downloads.controller.read(true);
    window.addEventListener("focus", recheck);
    return () => window.removeEventListener("focus", recheck);
  }, [active, downloads.controller]);
  const contentRevision = useSyncExternalStore(
    subscribeContentFilter,
    getContentFilterRevision,
  );
  const visibleTasks = useMemo(() => {
    downloads.snapshot.tasks.forEach(rememberContentWork);
    return downloads.snapshot.tasks.filter((task) => !isContentHidden(task));
  }, [downloads.snapshot, contentRevision]);
  const inputRows = input
    .split(/\r?\n/)
    .map((row) => row.trim())
    .filter(Boolean);
  const inputSubmitting =
    inputRows.length > 0 &&
    inputRows.every((row) =>
      downloads.submittingKeys.includes(
        downloadSubmissionKey(selectedSource, row),
      ),
    );
  const tasks = useMemo(
    () => filterDownloadTasks(visibleTasks, filter, query, queueSource),
    [visibleTasks, filter, query, queueSource],
  );
  const itemKeys = useMemo(() => tasks.map((task) => task.id), [tasks]);
  useBrowseSession({
    scope: browseScope("downloads", filter, queueSource, query),
    active,
    root,
    itemKeys,
  });
  const counts = Object.fromEntries(
    downloadQueueFilters.map((value) => [
      value,
      filterDownloadTasks(visibleTasks, value, query, queueSource).length,
    ]),
  );
  const summary = downloadQueueSummary(visibleTasks);
  const batch = downloads.recentBatch
    ? downloadBatchProgress(downloads.snapshot.tasks, downloads.recentBatch)
    : null;
  const labels = {
    active: "下载中",
    error: "下载失败／需要处理",
    downloaded: "已下载",
  };
  return (
    <>
      {historySelection && (
        <HistoryConfirmation
          downloads={downloads}
          tasks={historySelection.filter((task) => !isContentHidden(task))}
          onClose={() => setHistorySelection(null)}
        />
      )}
      <div
        ref={root}
        hidden={!active}
        className="native-downloads"
        data-testid="native-downloads"
      >
        <div className="page-heading">
          <div>
            <div className="eyebrow">MangaMonitor</div>
            <h1>
              下载队列{" "}
              <span className="heading-count">
                {unfinishedDownloadCount(downloads.snapshot.tasks)}
              </span>
            </h1>
            <p>多选漫画或粘贴作品编号，直接加入队列，依次完整保存到电脑。</p>
          </div>
        </div>
        <form
          className="native-download-add"
          onSubmit={(event) => {
            event.preventDefault();
            onPrepare();
          }}
        >
          <label htmlFor="download-source">下载来源</label>
          <select
            id="download-source"
            data-testid="download-source"
            value={selectedSource}
            onChange={(event) =>
              onSourceChange(event.target.value as DownloadSource)
            }
          >
            <option value="JM">JM</option>
            <option value="Pica">哔咔</option>
          </select>
          <label htmlFor="source-download-input">
            {sourceLabel(selectedSource)} 编号或作品链接
          </label>
          <div className="source-actions">
            <textarea
              id="source-download-input"
              data-testid="download-input"
              value={input}
              maxLength={102400}
              rows={3}
              placeholder={
                sourceLabel(selectedSource) +
                " 编号或作品链接，每行一个，最多 500 本"
              }
              onChange={(event) => onInputChange(event.target.value)}
            />
            <button
              className="button primary"
              data-testid="download-prepare"
              disabled={inputSubmitting || !downloads.ready || !input.trim()}
            >
              {inputSubmitting ? "正在加入…" : "加入下载"}
            </button>
          </div>
        </form>
        <p className="quiet">
          一次最多选择 500
          本；已有任务不会重复加入，失败作品可重新提交重试。同一时间处理一本，无法加入的作品单独列出，其余作品继续下载。
        </p>
        {!scope && (
          <p className="source-notice">
            请先连接{sourceLabel(selectedSource)}账号。
            <button className="text-button" onClick={onOpenAccounts}>
              打开账号设置
            </button>
          </p>
        )}
        {!context && scope && (
          <p className="source-notice">
            请先选择电脑漫画目录。
            <button
              className="text-button"
              data-testid="download-choose-library"
              onClick={onChooseLibrary}
            >
              选择电脑目录
            </button>
          </p>
        )}
        <div className="queue-overview">
          <div>
            <span className="activity-dot" />
            <strong data-testid="native-download-status">
              {downloadStatusText(downloads)}
            </strong>
          </div>
          <button
            className="text-button"
            data-testid="download-read"
            disabled={downloads.reading || downloads.busy}
            onClick={() => void downloads.controller.read()}
          >
            {downloads.reading ? "正在读取…" : "重新读取队列"}
          </button>
        </div>
        <div className="source-actions download-queue-controls">
          <button
            className="button secondary"
            data-testid="download-pause-all"
            disabled={
              downloads.busy ||
              !downloads.snapshot.tasks.some((task) =>
                ["queued", "downloading", "verifying", "saving"].includes(
                  task.phase,
                ),
              )
            }
            onClick={() => void downloads.controller.pauseAll()}
          >
            暂停队列
          </button>
          {(["JM", "Pica"] as const).map((source) => {
            const current = getDownloadScope(accounts, source);
            const resumable = visibleTasks
              .filter(
                (task) =>
                  task.source === source &&
                  task.allowedActions.includes("resume"),
              )
              .slice(0, 50);
            return (
              <button
                key={source}
                className="button secondary"
                data-testid={`download-resume-many-${source}`}
                disabled={downloads.busy || !current || !resumable.length}
                onClick={() => {
                  if (current)
                    void downloads.controller.resumeMany(current, resumable);
                }}
              >
                继续{sourceLabel(source)} {resumable.length} 本
              </button>
            );
          })}
        </div>
        {downloads.error && (
          <p
            role="alert"
            className="source-notice"
            data-testid="download-error"
          >
            {downloads.error}
          </p>
        )}
        {downloads.submissionIssues.length > 0 && (
          <details
            className="source-notice"
            data-testid="download-submission-issues"
          >
            <summary>
              本次有 {downloads.submissionIssues.length} 项未能加入
            </summary>
            {downloads.submissionIssues.map((issue, index) => (
              <p key={index}>
                {sourceLabel(issue.source)} · {issue.input}：{issue.message}
              </p>
            ))}
          </details>
        )}
        {downloads.ready && (
          <div className="download-queue-summary">
            <p className="quiet" data-testid="download-summary">
              当前队列：处理中 {summary.processing} · 等待 {summary.waiting} ·
              暂停 {summary.paused} · 需处理 {summary.attention} · 已下载{" "}
              {summary.downloaded}（不受下方筛选影响）
            </p>
            {batch && (
              <p className="quiet" data-testid="download-batch-progress">
                最近加入的批次：已完成 {batch.completed} / {batch.total} 本 ·
                需处理 {batch.attention} 本
                <span>
                  {" "}
                  · 本次打开期间最近一次加入的作品，完成数包含已整理的历史记录
                </span>
              </p>
            )}
          </div>
        )}
        <div className="tabs queue-tabs">
          {downloadQueueFilters.map((value) => (
            <button
              key={value}
              aria-pressed={filter === value}
              data-testid={"download-filter-" + value}
              className={filter === value ? "active" : ""}
              onClick={() => setFilter(value)}
            >
              {labels[value]}{" "}
              <span className="download-tab-count">{counts[value]}</span>
            </button>
          ))}
        </div>
        <div className="download-history-toolbar source-actions">
          <input
            aria-label="筛选下载任务"
            data-testid="download-history-query"
            placeholder="搜索队列中的标题或编号"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <select
            aria-label="筛选队列来源"
            data-testid="download-history-source"
            value={queueSource}
            onChange={(event) =>
              setQueueSource(event.target.value as DownloadSource | "all")
            }
          >
            <option value="all">全部来源</option>
            <option value="JM">JM</option>
            <option value="Pica">哔咔</option>
          </select>
          {filter === "downloaded" && (
            <button
              className="text-button"
              data-testid="download-history-clear"
              disabled={downloads.busy || !tasks.length}
              onClick={() => setHistorySelection(tasks.slice(0, 50))}
            >
              整理当前筛选结果（{Math.min(tasks.length, 50)} 条）
            </button>
          )}
        </div>
        {downloads.ready && (
          <p className="quiet" data-testid="download-filter-summary">
            当前筛选：下载中 {counts.active} · 需处理 {counts.error} · 已下载{" "}
            {counts.downloaded} · 当前显示 {tasks.length} 条
          </p>
        )}
        <div className="task-list">
          {tasks.map((task) => (
            <article
              className={
                "task-card" +
                (downloadNeedsAttention(task) ? " task-error" : "") +
                (isDownloadPresent(task) ? " task-complete" : "")
              }
              key={task.id}
              data-browse-key={task.id}
              data-testid={"download-task-" + task.id}
            >
              <div className="download-task-icon">
                <Icon
                  name={isDownloadPresent(task) ? "check" : "download"}
                  size={28}
                />
              </div>
              <div className="task-content">
                <div className="task-title-row">
                  <div>
                    <h3>{task.title}</h3>
                    <span className="quiet">
                      {sourceLabel(task.source)} · {task.workId}
                    </span>
                  </div>
                  <span
                    className="status"
                    data-testid={"download-phase-" + task.id}
                  >
                    {downloadTaskLabel(task)}
                  </span>
                </div>
                {isDownloadPresent(task) ? (
                  <>
                    <div className="task-bottom download-completion-meta">
                      <span>
                        完成时间：
                        {downloadCompletedAt(task) ? (
                          <time
                            data-testid={"download-completed-at-" + task.id}
                            dateTime={downloadCompletedAt(task)!}
                          >
                            {new Date(
                              downloadCompletedAt(task)!,
                            ).toLocaleString("zh-CN", { hour12: false })}
                          </time>
                        ) : (
                          "未知"
                        )}
                      </span>
                      <button
                        className="text-button"
                        data-testid={"download-open-" + task.id}
                        onClick={() => onOpenDownloaded(task)}
                      >
                        查看电脑文件
                      </button>
                    </div>
                    <details
                      className="download-completion-details"
                      data-testid={"download-details-" + task.id}
                    >
                      <summary>查看详情</summary>
                      <p className="quiet">
                        {task.filesDone} / {task.filesTotal ?? "未知"} 张 ·{" "}
                        {task.bytesDone.toLocaleString()} 字节
                      </p>
                      <p className="download-destination quiet">
                        {task.destinationDisplay}
                      </p>
                      <p className="quiet">电脑文件已保存并登记到漫画库。</p>
                      <button
                        className="text-button"
                        data-testid={"download-history-remove-" + task.id}
                        disabled={downloads.busy}
                        onClick={() => setHistorySelection([task])}
                      >
                        移除历史记录
                      </button>
                    </details>
                  </>
                ) : (
                  <>
                    <div
                      className={
                        "progress-track" +
                        (task.phase === "error" ? " error" : "")
                      }
                      role="progressbar"
                      aria-label={
                        task.title +
                        (task.phase === "downloaded"
                          ? " 历史完成进度"
                          : " 下载图片")
                      }
                      aria-valuenow={
                        task.filesTotal === null ? undefined : task.filesDone
                      }
                      aria-valuemin={0}
                      aria-valuemax={task.filesTotal ?? undefined}
                    >
                      <span
                        style={{
                          width: task.filesTotal
                            ? `${(task.filesDone / task.filesTotal) * 100}%`
                            : "0%",
                        }}
                      />
                    </div>
                    <div className="task-bottom">
                      <span>
                        {task.phase === "downloaded" && "历史完成："}
                        {task.filesDone} / {task.filesTotal ?? "未知"} 张 ·{" "}
                        {task.bytesDone.toLocaleString()} 字节
                      </span>
                      <div className="source-actions">
                        {task.allowedActions.map((action) => (
                          <button
                            key={action}
                            className="text-button"
                            disabled={
                              (action === "pause" && downloads.busy) ||
                              downloads.submittingKeys.includes(
                                downloadSubmissionKey(task.source, task.workId),
                              ) ||
                              !canControlDownload(
                                task,
                                action,
                                getDownloadScope(accounts, task.source),
                              )
                            }
                            data-testid={`download-${action}-${task.id}`}
                            onClick={() => {
                              if (action !== "pause") {
                                onReprepare(task);
                                return;
                              }
                              void downloads.controller.control(
                                getDownloadScope(accounts, task.source),
                                task,
                                action,
                              );
                            }}
                          >
                            {action === "pause"
                              ? "暂停"
                              : action === "resume"
                                ? "继续"
                                : "重试"}
                          </button>
                        ))}
                        {downloadNeedsAttention(task) && (
                          <button
                            className="text-button"
                            data-testid={"download-recheck-" + task.id}
                            disabled={downloads.busy || downloads.reading}
                            onClick={() => void downloads.controller.read(true)}
                          >
                            {task.phase === "downloaded"
                              ? "重新核对文件"
                              : "刷新任务状态"}
                          </button>
                        )}
                        {task.phase === "downloaded" && (
                          <button
                            className="text-button"
                            data-testid={`download-history-remove-${task.id}`}
                            disabled={downloads.busy}
                            onClick={() => setHistorySelection([task])}
                          >
                            移除历史记录
                          </button>
                        )}
                        {task.phase === "downloaded" &&
                          task.localFiles === "missing" && (
                            <button
                              className="text-button"
                              disabled={
                                downloads.submittingKeys.includes(
                                  downloadSubmissionKey(
                                    task.source,
                                    task.workId,
                                  ),
                                ) || !downloads.ready
                              }
                              data-testid={"download-reprepare-" + task.id}
                              onClick={() => onReprepare(task)}
                            >
                              重新下载
                            </button>
                          )}
                      </div>
                    </div>
                    {task.phase !== "downloaded" &&
                      task.filesTotal !== null &&
                      task.filesDone === task.filesTotal && (
                        <p
                          className="source-notice"
                          data-testid="download-finalization-pending"
                        >
                          图片已下载齐，保存或入库尚未完成。继续或重试时会先校验并复用已有进度。
                        </p>
                      )}
                    {task.errorCode && (
                      <p className="source-notice">
                        <strong>{downloadAttentionReason(task)}。 </strong>
                        {downloadErrorMessage(task.errorCode)}
                      </p>
                    )}
                    {((!getDownloadScope(accounts, task.source) &&
                      task.allowedActions.some(
                        (action) => action !== "pause",
                      )) ||
                      /SESSION|AUTH|ACCOUNT|CREDENTIAL|TOKEN|SOURCE_MISMATCH/.test(
                        task.errorCode ?? "",
                      )) && (
                      <p className="quiet">
                        请连接{sourceLabel(task.source)}账号后继续或重试。
                        <button
                          className="text-button"
                          data-testid={"download-accounts-" + task.id}
                          onClick={onOpenAccounts}
                        >
                          打开账号设置
                        </button>
                      </p>
                    )}
                    {task.phase === "error" &&
                      /ROOT|DIRECTORY|DESTINATION|LIBRARY|INDEX_/.test(
                        task.errorCode ?? "",
                      ) && (
                        <p className="quiet">
                          <button
                            className="text-button"
                            data-testid={"download-select-directory-" + task.id}
                            onClick={onChooseLibrary}
                          >
                            查看漫画库设置
                          </button>
                        </p>
                      )}
                    <p className="download-destination quiet">
                      {task.destinationDisplay}
                    </p>
                    {task.phase === "paused" && (
                      <p className="quiet">
                        {task.allowedActions.includes("resume")
                          ? "进度已保留，点击继续后执行。"
                          : "正在暂停，当前图片处理结束后可继续。"}
                      </p>
                    )}
                    {task.phase === "downloaded" &&
                      task.localFiles === "missing" && (
                        <p className="quiet">
                          原保存位置的作品文件已移除，保留历史完成记录。点击重新下载可直接加入队列。
                        </p>
                      )}
                    {task.phase === "downloaded" &&
                      task.localFiles === "incomplete" && (
                        <p className="quiet">
                          原目录或文件与这条完成记录不匹配，请核对电脑文件。历史完成记录保留。
                        </p>
                      )}
                    {task.phase === "downloaded" &&
                      task.localFiles === "unavailable" && (
                        <p className="quiet">
                          保存目录当前不可用，尚不能确认文件状态。请重新选择可访问的保存目录。
                          <button
                            className="text-button"
                            data-testid={"download-select-directory-" + task.id}
                            onClick={onChooseLibrary}
                          >
                            选择电脑目录
                          </button>
                        </p>
                      )}
                  </>
                )}
              </div>
            </article>
          ))}
        </div>
        {downloads.ready && tasks.length === 0 && (
          <div className="empty-state" data-testid="download-empty">
            <Icon name="download" size={32} />
            <h2>
              {query.trim() || queueSource !== "all"
                ? "当前筛选没有结果"
                : filter === "active"
                  ? "当前没有下载中的任务"
                  : filter === "error"
                    ? "没有需要处理的任务"
                    : "暂无已下载记录"}
            </h2>
            <p>
              {query.trim() || queueSource !== "all"
                ? "可以清空搜索或切换来源，查看其他任务。"
                : filter === "active"
                  ? "正常完成的任务可在「已下载」查看；也可以在上方添加新下载。"
                  : filter === "error"
                    ? "下载失败或文件状态异常的任务会显示在这里。"
                    : "完成保存和入库且文件正常的作品会显示在这里。"}
            </p>
          </div>
        )}
      </div>
    </>
  );
}
