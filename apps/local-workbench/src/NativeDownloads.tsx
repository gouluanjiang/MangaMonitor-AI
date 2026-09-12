import { useEffect, useMemo, useRef, useState } from "react";
import type {
  DownloadAdapter,
  DownloadContext,
  DownloadContexts,
  DownloadSource,
  DownloadTask,
} from "./download-types.ts";
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
) {
  const [controller] = useState(() => new DownloadController(adapter));
  const [state, setState] = useState(() => controller.getState());
  const completed = useRef<Set<string> | null>(null),
    callback = useRef(onDownloaded);
  callback.current = onDownloaded;
  const contextIdentity = JSON.stringify([contexts.JM, contexts.Pica]);
  useEffect(() => {
    controller.cancelPlan();
  }, [controller, contextIdentity]);
  useEffect(() => {
    if (!enabled) return;
    const unsubscribe = controller.subscribe(setState);
    void controller.read();
    return () => {
      unsubscribe();
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
  tasks.filter((task) => !isDownloadPresent(task)).length;
export function downloadStatusText(
  downloads: Pick<DownloadsState, "ready" | "snapshot" | "error">,
) {
  if (!downloads.ready) return "下载队列尚未读取";
  if (downloads.error) return "下载状态待确认";
  const running = downloads.snapshot.tasks.find((task) =>
    ["queued", "downloading", "verifying", "saving"].includes(task.phase),
  );
  if (running) return `${downloadPhaseLabel(running.phase)} · ${running.title}`;
  return downloads.snapshot.tasks.some((task) => task.phase === "paused")
    ? "下载已暂停"
    : downloads.snapshot.tasks.some(downloadNeedsAttention)
      ? "下载任务需要处理"
      : "下载队列就绪";
}
function DownloadConfirmation({
  downloads,
  context,
  onConfirmed,
}: {
  downloads: DownloadsState;
  context: DownloadContext | null;
  onConfirmed(): void;
}) {
  const dialog = useRef<HTMLDialogElement>(null),
    plan = downloads.plan;
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  if (!plan) return null;
  return (
    <dialog
      ref={dialog}
      className="dialog download-confirmation"
      data-testid="download-confirmation"
      aria-label="确认下载到电脑"
      onCancel={(event) => {
        event.preventDefault();
        if (!downloads.busy) downloads.controller.cancelPlan();
      }}
    >
      <div className="dialog-heading">
        <h2>确认下载到电脑</h2>
        <button
          className="icon-button"
          aria-label="关闭下载确认"
          disabled={downloads.busy}
          onClick={() => downloads.controller.cancelPlan()}
        >
          <Icon name="close" />
        </button>
      </div>
      <h3 data-testid="download-plan-title">{plan.title}</h3>
      <p data-testid="download-plan-source">
        {sourceLabel(plan.source)} · {plan.workId}
        {plan.authors.length ? " · " + plan.authors.join("、") : ""}
      </p>
      <p className="source-muted">保存位置</p>
      <p
        className="download-destination"
        data-testid="download-plan-destination"
      >
        {plan.destinationDisplay}
      </p>
      <p>
        同一时间只下载一本。保存为作品文件夹，包含元数据、封面、章节目录和图片。
      </p>
      <p>
        {plan.source === "Pica"
          ? "哔咔保留原图格式。"
          : "JM 图片保存为 JPEG 格式。"}
      </p>
      <p>
        完成后显示电脑“已下载”。手机名单保持原样，之后传到手机时电脑副本继续保留。
      </p>
      <p>完成后清理下载临时文件，电脑作品副本继续保留。</p>
      <div className="dialog-actions">
        <button
          className="button secondary"
          data-testid="download-cancel"
          disabled={downloads.busy}
          onClick={() => downloads.controller.cancelPlan()}
        >
          取消
        </button>
        <button
          className="button primary"
          data-testid="download-confirm"
          disabled={downloads.busy || context === null}
          onClick={() => {
            if (context)
              void downloads.controller.confirm(context).then((done) => {
                if (done) onConfirmed();
              });
          }}
        >
          {downloads.busy ? "正在确认…" : "确认下载"}
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
      <h2 id="native-download-title">单本下载</h2>
      <p className="settings-copy">
        从 JM
        或哔咔来源详情进入，或在下载队列选择来源并输入编号，核对标题与保存目录后确认。
      </p>
      <dl className="settings-facts">
        <div>
          <dt>当前执行方式</dt>
          <dd>同一时间只下载一本</dd>
        </div>
        <div>
          <dt>保存格式</dt>
          <dd>作品文件夹／章节目录／图片</dd>
        </div>
        <div>
          <dt>关闭再打开</dt>
          <dd>恢复任务记录，未完成任务暂停，点击继续后执行</dd>
        </div>
        <div>
          <dt>手机已入库</dt>
          <dd>由手机名单或手动标记确认，电脑副本保留</dd>
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
  contexts,
  accounts,
  selectedSource,
  onSourceChange,
  input,
  onInputChange,
  onPrepare,
  onChooseLibrary,
  onOpenAccounts,
  onConfirmed,
  onOpenDownloaded,
  onReprepare,
  showFeedback,
}: {
  downloads: DownloadsState;
  active: boolean;
  contexts: DownloadContexts;
  accounts: AccountSummary[];
  selectedSource: DownloadSource;
  onSourceChange(source: DownloadSource): void;
  input: string;
  onInputChange(value: string): void;
  onPrepare(): void;
  onChooseLibrary(): void;
  onOpenAccounts(): void;
  onConfirmed(): void;
  onOpenDownloaded(task: DownloadTask): void;
  onReprepare(task: DownloadTask): void;
  showFeedback: boolean;
}) {
  const [filter, setFilter] = useState("all");
  const scope = getDownloadScope(accounts, selectedSource);
  const context = contexts[selectedSource];
  useEffect(() => {
    if (!active) return;
    void downloads.controller.read(true);
    const recheck = () => void downloads.controller.read(true);
    window.addEventListener("focus", recheck);
    return () => window.removeEventListener("focus", recheck);
  }, [active, downloads.controller]);
  const tasks = useMemo(
    () => filterDownloadTasks(downloads.snapshot.tasks, filter),
    [downloads.snapshot, filter],
  );
  return (
    <>
      {downloads.plan && (
        <DownloadConfirmation
          downloads={downloads}
          context={contexts[downloads.plan.source]}
          onConfirmed={onConfirmed}
        />
      )}
      {!active && showFeedback && downloads.error && (
        <p
          role="alert"
          className="source-notice"
          data-testid="download-feedback"
        >
          {downloads.error}
        </p>
      )}
      <div
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
            <p>选择 JM 或哔咔作品，确认后完整保存到电脑。</p>
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
            disabled={downloads.busy}
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
            <input
              id="source-download-input"
              data-testid="download-input"
              value={input}
              maxLength={2048}
              placeholder={sourceLabel(selectedSource) + " 编号或作品链接"}
              onChange={(event) => onInputChange(event.target.value)}
            />
            <button
              className="button primary"
              data-testid="download-prepare"
              disabled={downloads.busy || !downloads.ready || !input.trim()}
            >
              {downloads.busy ? "正在处理…" : "准备下载"}
            </button>
          </div>
        </form>
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
        {downloads.error && (
          <p
            role="alert"
            className="source-notice"
            data-testid="download-error"
          >
            {downloads.error}
          </p>
        )}
        <div className="tabs queue-tabs">
          {[
            ["all", "全部任务"],
            ["active", "进行中"],
            ["error", "需要处理"],
            ["downloaded", "已下载"],
          ].map(([value, label]) => (
            <button
              key={value}
              aria-pressed={filter === value}
              data-testid={"download-filter-" + value}
              className={filter === value ? "active" : ""}
              onClick={() => setFilter(value)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="task-list">
          {tasks.map((task) => (
            <article
              className={
                "task-card" +
                (downloadNeedsAttention(task) ? " task-error" : "")
              }
              key={task.id}
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
                <div
                  className={
                    "progress-track" + (task.phase === "error" ? " error" : "")
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
                          downloads.busy ||
                          !canControlDownload(
                            task,
                            action,
                            getDownloadScope(accounts, task.source),
                          )
                        }
                        data-testid={`download-${action}-${task.id}`}
                        onClick={() => {
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
                    {isDownloadPresent(task) && (
                      <button
                        className="text-button"
                        data-testid={"download-open-" + task.id}
                        onClick={() => onOpenDownloaded(task)}
                      >
                        查看电脑文件
                      </button>
                    )}
                    {task.phase === "downloaded" &&
                      task.localFiles === "missing" && (
                        <button
                          className="text-button"
                          disabled={downloads.busy || !downloads.ready}
                          data-testid={"download-reprepare-" + task.id}
                          onClick={() => onReprepare(task)}
                        >
                          重新准备下载
                        </button>
                      )}
                  </div>
                </div>
                {task.errorCode && (
                  <p className="source-notice">
                    {downloadErrorMessage(task.errorCode)}
                  </p>
                )}
                {!getDownloadScope(accounts, task.source) &&
                  task.allowedActions.some((action) => action !== "pause") && (
                    <p className="quiet">
                      请连接{sourceLabel(task.source)}账号后继续或重试。
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
                {isDownloadPresent(task) && (
                  <p className="quiet">
                    电脑文件已保存并登记，手机名单未改变。
                  </p>
                )}
                {task.phase === "downloaded" &&
                  task.localFiles === "missing" && (
                    <p className="quiet">
                      原保存位置的作品文件已移除，保留历史完成记录。重新下载需要再次确认。
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
              </div>
            </article>
          ))}
        </div>
        {downloads.ready && tasks.length === 0 && (
          <div className="empty-state" data-testid="download-empty">
            <Icon name="download" size={32} />
            <h2>这里暂时没有任务</h2>
            <p>选择来源后输入作品编号，或在来源详情中选择下载到电脑。</p>
          </div>
        )}
      </div>
    </>
  );
}
