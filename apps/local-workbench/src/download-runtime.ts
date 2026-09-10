import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { emptyDownloads } from "./download-types.ts";
import type {
  DownloadAction,
  DownloadAdapter,
  DownloadContext,
  DownloadPhase,
  DownloadPlan,
  DownloadScope,
  DownloadSnapshot,
  DownloadTask,
} from "./download-types.ts";
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
const jmId = (value: unknown): string =>
  typeof value === "string" && /^[1-9]\d{0,19}$/.test(value)
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
function scope(value: DownloadScope): DownloadScope {
  if (value.source !== "JM") return invalid();
  return { source: "JM", sessionId: opaque(value.sessionId) };
}
export function validateDownloadPlan(value: unknown): DownloadPlan {
  const raw = record(value);
  if (
    raw.source !== "JM" ||
    !Array.isArray(raw.authors) ||
    raw.authors.length > 1000
  )
    return invalid();
  return {
    planId: rootIdentity(raw.planId),
    revision: integer(raw.revision),
    source: "JM",
    workId: jmId(raw.workId),
    title: text(raw.title),
    authors: raw.authors.map((value) => text(value)),
    destinationDisplay: text(raw.destinationDisplay),
    rootId: rootIdentity(raw.rootId),
    generation: integer(raw.generation),
  };
}
export function validateDownloadSnapshot(value: unknown): DownloadSnapshot {
  const raw = record(value);
  if (!Array.isArray(raw.tasks) || raw.tasks.length > 50) return invalid();
  const tasks = raw.tasks.map((value) => {
    const task = record(value);
    if (integer(task.revision) === 0) return invalid();
    if (
      task.source !== "JM" ||
      !phases.includes(task.phase as DownloadPhase) ||
      !Array.isArray(task.allowedActions) ||
      task.allowedActions.some((action) => !actions.includes(action)) ||
      new Set(task.allowedActions).size !== task.allowedActions.length
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
      source: "JM",
      workId: jmId(task.workId),
      title: text(task.title),
      phase: task.phase,
      filesDone,
      filesTotal,
      bytesDone: integer(task.bytesDone),
      errorCode,
      allowedActions: task.allowedActions,
      libraryEntryId,
      updatedAt: integer(task.updatedAt),
      destinationDisplay: text(task.destinationDisplay),
    } as DownloadTask;
  });
  if (new Set(tasks.map((task) => task.id)).size !== tasks.length)
    return invalid();
  return { revision: integer(raw.revision), tasks };
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
    read: async () => validateDownloadSnapshot(await call("jm_download_read")),
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
    control: async (current, taskId, expectedRevision, action) => {
      if (!actions.includes(action)) return invalid();
      return validateDownloadSnapshot(
        await call("jm_download_control", {
          scope: scope(current),
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
  if (/SESSION|AUTH|ACCOUNT|CREDENTIAL/.test(code))
    return "JM 会话已改变或需要重新登录。请连接 JM 后再试。";
  if (code === "LIBRARY_BUSY")
    return "电脑目录正在读取或已暂停读取，请先完成目录读取再准备下载。";
  if (/BUSY/.test(code)) return "当前任务还在处理，请等待它暂停或完成后再试。";
  if (code === "DOWNLOAD_LIMIT_REACHED")
    return "当前下载记录已达到本批支持的上限，暂时无法添加新任务。";
  if (/DOCUMENT_INVALID|INVALID_DOCUMENT|RESPONSE_INVALID/.test(code))
    return "下载状态无法确认，已显示内容保留。请重新读取队列。";
  if (/INDEX_/.test(code))
    return "作品已保存，但电脑文件登记尚未完成。重试会先核对已有结果。";
  if (code === "DOWNLOAD_METADATA_INVALID")
    return "来源作品信息暂时无法确认，请重新读取作品后再试。";
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
    return "请输入有效的 JM 编号或受支持的作品链接。";
  if (/INCOMPLETE|VERIFY|MANIFEST|PROOF/.test(code))
    return "作品尚未通过完整校验，已保留进度，可按任务提示重试。";
  if (code === "DESKTOP_REQUIRED") return "请在桌面应用中下载 JM 作品。";
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
  };
  private listeners = new Set<(state: DownloadState) => void>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readPromise: Promise<void> | null = null;
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
    if (
      !this.state.error &&
      !this.state.busy &&
      this.state.snapshot.tasks.some(activeTask)
    )
      this.timer = setTimeout(() => void this.read(), 1000);
  }
  read(): Promise<void> {
    if (this.readPromise) return this.readPromise;
    if (this.state.busy) return Promise.resolve();
    clearTimeout(this.timer);
    const epoch = this.epoch;
    this.publish({ reading: true });
    this.readPromise = (async () => {
      try {
        const next = await this.adapter.read();
        if (epoch === this.epoch) this.accept(next);
      } catch (cause) {
        if (epoch === this.epoch)
          this.publish({ error: downloadErrorMessage(cause) });
      } finally {
        if (epoch === this.epoch) {
          this.readPromise = null;
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
    this.publish({ plan: null });
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
          (task) => task.id === plan.planId && task.workId === plan.workId,
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
    current: DownloadScope,
    task: DownloadTask,
    action: DownloadAction,
  ): Promise<void> {
    if (this.state.busy || !task.allowedActions.includes(action)) return;
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return;
      const next = await this.adapter.control(
        current,
        task.id,
        task.revision,
        action,
      );
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
    this.listeners.clear();
    this.state = { ...this.state, reading: false, busy: false, plan: null };
  }
}
