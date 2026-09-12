import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { emptyDownloads } from "./download-types.ts";
import type {
  DownloadAction,
  DownloadAdapter,
  DownloadContext,
  DownloadPhase,
  DownloadPlan,
  DownloadScope,
  DownloadSource,
  DownloadSnapshot,
  DownloadTask,
  DownloadBatchPlan,
  DownloadTaskRevision,
} from "./download-types.ts";
import type { AccountSummary } from "./source-types.ts";
export class DownloadError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.name = "DownloadError";
    this.code = code;
  }
}
const invalid = (): never => {
  throw new DownloadError("DOWNLOAD_RESPONSE_INVALID");
};
const record = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : invalid();
const text = (value: unknown, maximum = 16384): string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= maximum &&
  !/[\x00-\x08\x0b\x0c\x0e-\x1f]/.test(value)
    ? value
    : invalid();
const integer = (value: unknown): number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : invalid();
const opaque = (value: unknown): string =>
  typeof value === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value)
    ? value
    : invalid();
const rootIdentity = (value: unknown): string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value) ? value : invalid();
const downloadSource = (value: unknown): DownloadSource =>
  value === "JM" || value === "Pica" ? value : invalid();
const workId = (source: DownloadSource, value: unknown): string =>
  typeof value === "string" &&
  (source === "JM" ? /^[1-9]\d{0,19}$/ : /^[a-f0-9]{24}$/).test(value)
    ? value
    : invalid();
const actions: DownloadAction[] = ["pause", "resume", "retry"];
const phases: DownloadPhase[] = [
  "queued",
  "downloading",
  "verifying",
  "saving",
  "paused",
  "error",
  "downloaded",
];
function scope(value: DownloadScope, allowEmpty = false): DownloadScope {
  return {
    source: downloadSource(value.source),
    sessionId:
      allowEmpty && value.sessionId === "" ? "" : opaque(value.sessionId),
  };
}
export function getDownloadScope(
  accounts: AccountSummary[],
  source: DownloadSource,
): DownloadScope | null {
  const account = accounts.find((account) => account.source === source);
  return account?.state === "connected" && account.sessionId
    ? { source, sessionId: account.sessionId }
    : null;
}
export const canControlDownload = (
  task: DownloadTask,
  action: DownloadAction,
  current: DownloadScope | null,
) =>
  task.allowedActions.includes(action) &&
  (current === null
    ? action === "pause"
    : current.source === task.source &&
      (action === "pause" || Boolean(current.sessionId)));
export function validateDownloadPlan(value: unknown): DownloadPlan {
  const raw = record(value);
  if (!Array.isArray(raw.authors) || raw.authors.length > 1000)
    return invalid();
  return {
    planId: rootIdentity(raw.planId),
    revision: integer(raw.revision),
    source: downloadSource(raw.source),
    workId: workId(downloadSource(raw.source), raw.workId),
    title: text(raw.title),
    authors: raw.authors.map((value) => text(value)),
    destinationDisplay: text(raw.destinationDisplay),
    rootId: rootIdentity(raw.rootId),
    generation: integer(raw.generation),
  };
}
export function validateDownloadSnapshot(value: unknown): DownloadSnapshot {
  const raw = record(value);
  if (!Array.isArray(raw.tasks) || raw.tasks.length > 500) return invalid();
  const tasks = raw.tasks.map((value) => {
    const task = record(value);
    if (integer(task.revision) === 0) return invalid();
    if (
      !phases.includes(task.phase as DownloadPhase) ||
      !Array.isArray(task.allowedActions) ||
      task.allowedActions.some((action) => !actions.includes(action)) ||
      new Set(task.allowedActions).size !== task.allowedActions.length
    )
      return invalid();
    if (
      task.phase === "downloaded"
        ? !["present", "missing", "incomplete", "unavailable"].includes(
            task.localFiles as string,
          )
        : task.localFiles !== null
    )
      return invalid();
    const filesDone = integer(task.filesDone),
      filesTotal = task.filesTotal === null ? null : integer(task.filesTotal),
      libraryEntryId =
        task.libraryEntryId === null ? null : rootIdentity(task.libraryEntryId);
    const errorCode =
      task.errorCode === null
        ? null
        : typeof task.errorCode === "string" &&
            /^[A-Z0-9_]{1,100}$/.test(task.errorCode)
          ? task.errorCode
          : invalid();
    if (filesTotal !== null && filesDone > filesTotal) return invalid();
    if (
      task.phase === "downloaded" &&
      (libraryEntryId === null ||
        filesTotal === null ||
        filesTotal === 0 ||
        filesDone !== filesTotal ||
        errorCode !== null ||
        task.allowedActions.length !== 0)
    )
      return invalid();
    if (task.phase !== "downloaded" && libraryEntryId !== null)
      return invalid();
    if (task.allowedActions.includes("resume") && task.phase !== "paused")
      return invalid();
    if (task.allowedActions.includes("retry") && task.phase !== "error")
      return invalid();
    if (
      task.allowedActions.includes("pause") &&
      ["saving", "paused", "error", "downloaded"].includes(task.phase as string)
    )
      return invalid();
    return {
      id: rootIdentity(task.id),
      revision: integer(task.revision),
      source: downloadSource(task.source),
      workId: workId(downloadSource(task.source), task.workId),
      title: text(task.title),
      phase: task.phase,
      filesDone,
      filesTotal,
      bytesDone: integer(task.bytesDone),
      errorCode,
      allowedActions: task.allowedActions,
      libraryEntryId,
      localFiles: task.localFiles,
      updatedAt: integer(task.updatedAt),
      destinationDisplay: text(task.destinationDisplay),
    } as DownloadTask;
  });
  if (new Set(tasks.map((task) => task.id)).size !== tasks.length)
    return invalid();
  return { revision: integer(raw.revision), tasks };
}
export function validateDownloadBatchPlan(value: unknown): DownloadBatchPlan {
  const raw = record(value);
  if (
    !Array.isArray(raw.plans) ||
    !Array.isArray(raw.issues) ||
    raw.plans.length + raw.issues.length > 50 ||
    raw.plans.length + raw.issues.length === 0
  )
    return invalid();
  const plans = raw.plans.map(validateDownloadPlan);
  if (
    new Set(plans.map((plan) => plan.planId)).size !== plans.length ||
    new Set(plans.map((plan) => plan.source + ":" + plan.workId)).size !==
      plans.length
  )
    return invalid();
  const batchId = raw.batchId === null ? null : rootIdentity(raw.batchId);
  if (plans.length > 0 !== (batchId !== null)) return invalid();
  const issues = raw.issues.map((value) => {
    const issue = record(value);
    if (
      typeof issue.errorCode !== "string" ||
      !/^[A-Z0-9_]{1,100}$/.test(issue.errorCode)
    )
      return invalid();
    return { input: text(issue.input, 2048), errorCode: issue.errorCode };
  });
  return { batchId, plans, issues };
}
function taskRevisions(tasks: DownloadTaskRevision[]): DownloadTaskRevision[] {
  if (
    !Array.isArray(tasks) ||
    tasks.length === 0 ||
    tasks.length > 50 ||
    new Set(tasks.map((task) => task.taskId)).size !== tasks.length
  )
    return invalid();
  return tasks.map((task) => {
    if (integer(task.expectedRevision) === 0) return invalid();
    return {
      taskId: rootIdentity(task.taskId),
      expectedRevision: task.expectedRevision,
    };
  });
}
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createDownloadAdapter(
  options: { native?: boolean; invoke?: Invoke } = {},
): DownloadAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  async function call(
    command: string,
    args: Record<string, unknown> = {},
  ): Promise<unknown> {
    if (!(options.native ?? isDesktopRuntime()))
      throw new DownloadError("DESKTOP_REQUIRED");
    try {
      return await invoke(command, args);
    } catch (cause) {
      const code = (cause as { code?: unknown })?.code;
      throw new DownloadError(
        typeof code === "string" && /^[A-Z0-9_]{1,100}$/.test(code)
          ? code
          : "DOWNLOAD_UNAVAILABLE",
      );
    }
  }
  return {
    read: async (recheckFiles = true) => {
      if (typeof recheckFiles !== "boolean") return invalid();
      return validateDownloadSnapshot(
        await call("jm_download_read", { recheckFiles }),
      );
    },
    prepare: async (context, input) => {
      const checked = {
        scope: scope(context.scope),
        rootId: rootIdentity(context.rootId),
        generation: integer(context.generation),
        input: text(input.trim(), 2048),
      };
      const plan = validateDownloadPlan(
        await call("jm_download_prepare", checked),
      );
      if (plan.source !== checked.scope.source)
        throw new DownloadError("DOWNLOAD_SOURCE_MISMATCH");
      if (
        plan.rootId !== checked.rootId ||
        plan.generation !== checked.generation
      )
        throw new DownloadError("DOWNLOAD_PLAN_STALE");
      return plan;
    },
    confirm: async (planId, expectedRevision) =>
      validateDownloadSnapshot(
        await call("jm_download_confirm", {
          planId: rootIdentity(planId),
          expectedRevision: integer(expectedRevision),
        }),
      ),
    prepareBatch: async (context, inputs) => {
      if (!Array.isArray(inputs) || inputs.length === 0 || inputs.length > 50)
        return invalid();
      const checked = {
        scope: scope(context.scope),
        rootId: rootIdentity(context.rootId),
        generation: integer(context.generation),
        inputs: inputs.map((input) => text(input.trim(), 2048)),
      };
      const batch = validateDownloadBatchPlan(
        await call("jm_download_batch_prepare", checked),
      );
      if (batch.plans.some((plan) => plan.source !== checked.scope.source))
        throw new DownloadError("DOWNLOAD_SOURCE_MISMATCH");
      if (
        batch.plans.some(
          (plan) =>
            plan.rootId !== checked.rootId ||
            plan.generation !== checked.generation,
        )
      )
        throw new DownloadError("DOWNLOAD_PLAN_STALE");
      return batch;
    },
    confirmBatch: async (batchId) =>
      validateDownloadSnapshot(
        await call("jm_download_batch_confirm", {
          batchId: rootIdentity(batchId),
        }),
      ),
    pauseAll: async () =>
      validateDownloadSnapshot(await call("jm_download_pause_all")),
    resumeMany: async (current, tasks) =>
      validateDownloadSnapshot(
        await call("jm_download_resume_many", {
          scope: scope(current),
          tasks: taskRevisions(tasks),
        }),
      ),
    removeHistory: async (tasks) =>
      validateDownloadSnapshot(
        await call("jm_download_history_remove", {
          tasks: taskRevisions(tasks),
        }),
      ),
    control: async (current, taskId, expectedRevision, action) => {
      if (!actions.includes(action)) return invalid();
      return validateDownloadSnapshot(
        await call("jm_download_control", {
          scope: scope(current, action === "pause"),
          taskId: rootIdentity(taskId),
          expectedRevision: integer(expectedRevision),
          action,
        }),
      );
    },
  };
}
export function downloadErrorMessage(cause: unknown): string {
  const code =
    typeof cause === "string"
      ? cause
      : ((cause as { code?: string })?.code ?? "DOWNLOAD_UNAVAILABLE");
  if (/SESSION|AUTH|ACCOUNT|CREDENTIAL|TOKEN/.test(code))
    return "来源会话已改变或需要重新登录。请连接对应来源的账号后再试。";
  if (code === "DOWNLOAD_SOURCE_MISMATCH")
    return "任务来源与当前账号不一致，请使用对应来源的账号。";
  if (code === "DOWNLOAD_MEDIA_TIMEOUT")
    return "图片服务器响应超时，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_NETWORK_ERROR")
    return "暂时无法连接图片服务器，已保留进度，请检查网络后重试。";
  if (code === "DOWNLOAD_MEDIA_ADDRESS_UNSUPPORTED")
    return "来源返回了暂不支持的图片地址，已保留进度，请反馈此问题。";
  if (code === "DOWNLOAD_MEDIA_REDIRECT_FAILED")
    return "图片服务器的跳转地址异常，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_ACCESS_DENIED")
    return "图片服务器拒绝访问，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_UNAVAILABLE")
    return "来源图片暂时不可用，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_RATE_LIMITED")
    return "图片服务器暂时限制请求，请稍候再重试，已有进度会保留。";
  if (code === "DOWNLOAD_MEDIA_SERVER_ERROR")
    return "图片服务器暂时出错，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_EMPTY")
    return "图片服务器返回了空内容，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_MEDIA_TOO_LARGE")
    return "来源图片超过当前支持的大小，已保留进度，请反馈此问题。";
  if (code === "DOWNLOAD_MEDIA_IMAGE_INVALID")
    return "来源返回的图片无法解码或格式不符，已保留进度，请稍后重试。";
  if (code === "DOWNLOAD_LOCAL_FILES_INCOMPLETE")
    return "原目录或文件已变化，请先核对电脑文件。";
  if (code === "DOWNLOAD_LOCAL_FILES_UNAVAILABLE")
    return "保存目录或文件暂时无法读取，请检查目录后重新准备。";
  if (code === "LIBRARY_BUSY")
    return "电脑目录正在读取或已暂停读取，请先完成目录读取再准备下载。";
  if (/BUSY/.test(code)) return "当前任务还在处理，请等待它暂停或完成后再试。";
  if (code === "DOWNLOAD_LIMIT_REACHED")
    return "下载记录已达到 500 条，请先整理已完成的历史记录，再添加任务。";
  if (
    code === "DOWNLOAD_BATCH_INPUT_INVALID" ||
    code === "DOWNLOAD_BATCH_LIMIT"
  )
    return "每行输入一个编号或链接，每批最多 50 本。";
  if (code === "DOWNLOAD_BATCH_DUPLICATE")
    return "本批重复的来源编号，已跳过。";
  if (code === "DOWNLOAD_HISTORY_NOT_COMPLETED")
    return "只能整理已完成的下载记录，未完成任务的进度会保留。";
  if (code === "DOWNLOAD_HISTORY_LIMIT_REACHED")
    return "保留的作品身份记录已达到上限，暂时无法继续整理历史。";
  if (code === "DOWNLOAD_PLAN_LIMIT_REACHED")
    return "待确认计划过多，请关闭确认单后重新准备。";
  if (code === "DOWNLOAD_RESUME_REQUIRED")
    return "任务记录已恢复，请点击继续后执行。";
  if (/DOCUMENT_INVALID|INVALID_DOCUMENT|RESPONSE_INVALID/.test(code))
    return "下载状态无法确认，已显示内容保留。请重新读取队列。";
  if (/INDEX_/.test(code))
    return "作品已保存，但电脑文件登记尚未完成。重试会先核对已有结果。";
  if (code === "DOWNLOAD_METADATA_INVALID")
    return "来源作品信息暂时无法确认，请重新读取作品后再试。";
  if (code === "DOWNLOAD_SOURCE_INCOMPLETE")
    return "来源目录暂未读取完整，当前下载还不能完成。已保留进度，请稍后重试。";
  if (/STAGING_CHANGED|SOURCE_CHANGED|STAGING_CONFLICT/.test(code))
    return "已有进度或来源内容发生变化，请核对当前任务后重试。";
  if (/EXISTS|DUPLICATE|ALREADY/.test(code))
    return "电脑已存在该作品或同名目标，请核对电脑文件。现有文件保持不变。";
  if (/ROOT|DIRECTORY|DESTINATION|LIBRARY/.test(code))
    return "电脑目录暂时无法使用，请重新选择或读取目录后重试。";
  if (/STALE|REVISION|PLAN|CONFLICT/.test(code))
    return "计划或任务已改变，请重新读取并确认当前操作。";
  if (
    /INVALID_INPUT|UNSUPPORTED_URL|INVALID_URL|INVALID_ID|WORK_ID_INVALID/.test(
      code,
    )
  )
    return "请输入所选来源的有效编号或受支持的作品链接。";
  if (/INCOMPLETE|VERIFY|MANIFEST|PROOF/.test(code))
    return "作品尚未通过完整校验，已保留进度，可按任务提示重试。";
  if (code === "DESKTOP_REQUIRED") return "请在桌面应用中下载作品。";
  return "下载暂时未能完成。已显示内容会保留，请重新读取或按任务提示重试。";
}
export const downloadPhaseLabel = (phase: DownloadPhase) =>
  ({
    queued: "等待下载",
    downloading: "正在下载",
    verifying: "校验图片",
    saving: "保存到电脑",
    paused: "已暂停",
    error: "需要处理",
    downloaded: "已下载",
  })[phase];
export const isDownloadPresent = (task: DownloadTask) =>
  task.phase === "downloaded" && task.localFiles === "present";
export const downloadNeedsAttention = (task: DownloadTask) =>
  task.phase === "error" ||
  (task.phase === "downloaded" && !isDownloadPresent(task));
export const downloadTaskLabel = (task: DownloadTask) =>
  task.phase === "downloaded" && !isDownloadPresent(task)
    ? ({
        missing: "文件已移除",
        incomplete: "文件已变化",
        unavailable: "目录不可用",
      }[task.localFiles as "missing" | "incomplete" | "unavailable"] ??
      "文件状态待核对")
    : downloadPhaseLabel(task.phase);
export function parseDownloadInputs(input: string): string[] {
  const inputs = input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (
    inputs.length === 0 ||
    inputs.length > 50 ||
    inputs.some((line) => line.length > 2048)
  )
    throw new DownloadError("DOWNLOAD_BATCH_INPUT_INVALID");
  return inputs;
}
export const filterDownloadTasks = (
  tasks: DownloadTask[],
  filter: string,
  query = "",
  source: DownloadSource | "all" = "all",
) =>
  tasks.filter(
    (task) =>
      (source === "all" || task.source === source) &&
      [task.title, task.workId, sourceLabelForSearch(task.source)]
        .join(" ")
        .normalize("NFKC")
        .toLocaleLowerCase()
        .includes(query.trim().normalize("NFKC").toLocaleLowerCase()) &&
      (filter === "all" ||
        (filter === "history" && task.phase === "downloaded") ||
        (filter === "downloaded" && isDownloadPresent(task)) ||
        (filter === "error" && downloadNeedsAttention(task)) ||
        (filter === "active" && !["error", "downloaded"].includes(task.phase))),
  );
const sourceLabelForSearch = (source: DownloadSource) =>
  source === "Pica" ? "Pica 哔咔" : "JM";
const activeTask = (task: DownloadTask) =>
  ["queued", "downloading", "verifying", "saving"].includes(task.phase) ||
  (["paused", "error"].includes(task.phase) &&
    task.allowedActions.length === 0);
const contextKey = (value: DownloadContext) =>
  JSON.stringify([
    value.scope.source,
    value.scope.sessionId,
    value.rootId,
    value.generation,
  ]);
export interface DownloadState {
  snapshot: DownloadSnapshot;
  ready: boolean;
  reading: boolean;
  busy: boolean;
  error: string;
  plan: DownloadPlan | null;
  batchPlan: DownloadBatchPlan | null;
}
/** Read polling is single-flight and never starts or resumes persisted work. */
export class DownloadController {
  readonly adapter: DownloadAdapter;
  private state: DownloadState = {
    snapshot: emptyDownloads(),
    ready: false,
    reading: false,
    busy: false,
    error: "",
    plan: null,
    batchPlan: null,
  };
  private listeners = new Set<(state: DownloadState) => void>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readPromise: Promise<void> | null = null;
  private readingFiles = false;
  private pendingRecheck = false;
  private epoch = 0;
  private planEpoch = 0;
  private preparedContext: string | null = null;
  constructor(adapter: DownloadAdapter) {
    this.adapter = adapter;
  }
  getState() {
    return this.state;
  }
  subscribe(listener: (state: DownloadState) => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private publish(next: Partial<DownloadState>) {
    this.state = { ...this.state, ...next };
    for (const listener of this.listeners) listener(this.state);
  }
  private accept(snapshot: DownloadSnapshot) {
    if (snapshot.revision < this.state.snapshot.revision)
      throw new DownloadError("DOWNLOAD_REVISION_STALE");
    this.publish({ snapshot, ready: true, error: "" });
  }
  private schedule() {
    clearTimeout(this.timer);
    if (this.pendingRecheck && !this.state.busy) {
      this.timer = setTimeout(() => void this.read(true), 0);
      return;
    }
    if (
      !this.state.error &&
      !this.state.busy &&
      this.state.snapshot.tasks.some(activeTask)
    )
      this.timer = setTimeout(() => void this.read(false), 1000);
  }
  read(recheckFiles = true): Promise<void> {
    if (this.readPromise) {
      if (recheckFiles && !this.readingFiles) this.pendingRecheck = true;
      return this.readPromise;
    }
    if (this.state.busy) {
      if (recheckFiles) this.pendingRecheck = true;
      return Promise.resolve();
    }
    clearTimeout(this.timer);
    if (recheckFiles) this.pendingRecheck = false;
    const epoch = this.epoch;
    this.publish({ reading: true });
    this.readPromise = (async () => {
      try {
        let checkFiles = recheckFiles;
        do {
          this.readingFiles = checkFiles;
          const next = await this.adapter.read(checkFiles);
          if (epoch !== this.epoch) return;
          this.accept(next);
          if (!this.pendingRecheck || this.state.busy) break;
          this.pendingRecheck = false;
          checkFiles = true;
        } while (true);
      } catch (cause) {
        if (epoch === this.epoch)
          this.publish({ error: downloadErrorMessage(cause) });
      } finally {
        if (epoch === this.epoch) {
          this.readPromise = null;
          this.readingFiles = false;
          this.publish({ reading: false });
          this.schedule();
        }
      }
    })();
    return this.readPromise;
  }
  async prepare(context: DownloadContext, input: string): Promise<void> {
    if (this.state.busy) return;
    this.cancelPlan();
    const token = this.planEpoch,
      epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch || token !== this.planEpoch) return;
      const plan = await this.adapter.prepare(context, input);
      if (plan.source !== context.scope.source)
        throw new DownloadError("DOWNLOAD_SOURCE_MISMATCH");
      if (epoch === this.epoch && token === this.planEpoch) {
        this.preparedContext = contextKey(context);
        this.publish({ plan });
      }
    } catch (cause) {
      if (epoch === this.epoch && token === this.planEpoch)
        this.publish({ error: downloadErrorMessage(cause) });
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  cancelPlan() {
    this.planEpoch++;
    this.preparedContext = null;
    this.publish({ plan: null, batchPlan: null });
  }
  async prepareBatch(
    context: DownloadContext,
    inputs: string[],
  ): Promise<void> {
    if (this.state.busy) return;
    this.cancelPlan();
    const token = this.planEpoch,
      epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch || token !== this.planEpoch) return;
      const batchPlan = await this.adapter.prepareBatch(context, inputs);
      if (
        batchPlan.plans.some(
          (plan) =>
            plan.source !== context.scope.source ||
            plan.rootId !== context.rootId ||
            plan.generation !== context.generation,
        )
      )
        throw new DownloadError("DOWNLOAD_PLAN_STALE");
      if (epoch === this.epoch && token === this.planEpoch) {
        this.preparedContext = contextKey(context);
        this.publish({ batchPlan });
      }
    } catch (cause) {
      if (epoch === this.epoch && token === this.planEpoch)
        this.publish({ error: downloadErrorMessage(cause) });
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  async confirmBatch(context: DownloadContext): Promise<boolean> {
    const batch = this.state.batchPlan;
    if (!batch?.batchId || this.state.busy) return false;
    if (this.preparedContext !== contextKey(context)) {
      this.cancelPlan();
      this.publish({ error: downloadErrorMessage("DOWNLOAD_PLAN_STALE") });
      return false;
    }
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return false;
      const next = await this.adapter.confirmBatch(batch.batchId);
      if (epoch !== this.epoch) return false;
      if (
        !batch.plans.every((plan) =>
          next.tasks.some(
            (task) =>
              task.id === plan.planId &&
              task.source === plan.source &&
              task.workId === plan.workId,
          ),
        )
      )
        return invalid();
      this.accept(next);
      this.cancelPlan();
      return true;
    } catch (cause) {
      if (epoch === this.epoch) {
        this.cancelPlan();
        this.publish({ error: downloadErrorMessage(cause) });
      }
      return false;
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  private async changeQueue(
    operation: () => Promise<DownloadSnapshot>,
  ): Promise<boolean> {
    if (this.state.busy) return false;
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return false;
      const next = await operation();
      if (epoch !== this.epoch) return false;
      this.accept(next);
      return true;
    } catch (cause) {
      if (epoch === this.epoch)
        this.publish({ error: downloadErrorMessage(cause) });
      return false;
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  pauseAll() {
    return this.changeQueue(() => this.adapter.pauseAll());
  }
  resumeMany(current: DownloadScope, tasks: DownloadTask[]) {
    if (
      !tasks.length ||
      tasks.some((task) => !canControlDownload(task, "resume", current))
    )
      return Promise.resolve(false);
    return this.changeQueue(() =>
      this.adapter.resumeMany(
        current,
        tasks.map((task) => ({
          taskId: task.id,
          expectedRevision: task.revision,
        })),
      ),
    );
  }
  removeHistory(tasks: DownloadTask[]) {
    if (!tasks.length || tasks.some((task) => task.phase !== "downloaded"))
      return Promise.resolve(false);
    return this.changeQueue(async () => {
      const next = await this.adapter.removeHistory(
        tasks.map((task) => ({
          taskId: task.id,
          expectedRevision: task.revision,
        })),
      );
      if (
        next.tasks.some((task) =>
          tasks.some((removed) => removed.id === task.id),
        )
      )
        return invalid();
      return next;
    });
  }
  async confirm(context: DownloadContext): Promise<boolean> {
    const plan = this.state.plan;
    if (!plan || this.state.busy) return false;
    if (this.preparedContext !== contextKey(context)) {
      this.cancelPlan();
      this.publish({ error: downloadErrorMessage("DOWNLOAD_PLAN_STALE") });
      return false;
    }
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return false;
      const next = await this.adapter.confirm(plan.planId, plan.revision);
      if (epoch !== this.epoch) return false;
      if (
        !next.tasks.some(
          (task) =>
            task.id === plan.planId &&
            task.source === plan.source &&
            task.workId === plan.workId,
        )
      )
        return invalid();
      this.accept(next);
      this.cancelPlan();
      return true;
    } catch (cause) {
      if (epoch === this.epoch) {
        this.cancelPlan();
        this.publish({ error: downloadErrorMessage(cause) });
      }
      return false;
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  async control(
    current: DownloadScope | null,
    task: DownloadTask,
    action: DownloadAction,
  ): Promise<void> {
    if (this.state.busy || !task.allowedActions.includes(action)) return;
    if (!canControlDownload(task, action, current)) {
      this.publish({
        error: downloadErrorMessage(
          current ? "DOWNLOAD_SOURCE_MISMATCH" : "DOWNLOAD_SESSION_REQUIRED",
        ),
      });
      return;
    }
    const taskScope = current ?? { source: task.source, sessionId: "" };
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return;
      const next = await this.adapter.control(
        taskScope,
        task.id,
        task.revision,
        action,
      );
      if (
        !next.tasks.some(
          (result) =>
            result.id === task.id &&
            result.source === task.source &&
            result.workId === task.workId,
        )
      )
        return invalid();
      if (epoch === this.epoch) this.accept(next);
    } catch (cause) {
      if (epoch === this.epoch)
        this.publish({ error: downloadErrorMessage(cause) });
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  dispose() {
    this.epoch++;
    this.planEpoch++;
    clearTimeout(this.timer);
    this.readPromise = null;
    this.readingFiles = false;
    this.pendingRecheck = false;
    this.listeners.clear();
    this.state = {
      ...this.state,
      reading: false,
      busy: false,
      plan: null,
      batchPlan: null,
    };
  }
}
