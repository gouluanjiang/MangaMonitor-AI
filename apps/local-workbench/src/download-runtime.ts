import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import {
  downloadMetadataCode,
  downloadMetadataMessages,
  retainDownloadMetadataProblems,
} from "./download-metadata-problems.ts";
import type { DownloadMetadataProblem } from "./download-metadata-problems.ts";
import {
  downloadInputWorkId,
  downloadSubmissionKey,
} from "./download-input.ts";
import {
  emptyDownloads,
  downloadSelectionLimit,
  downloadPreparationChunk,
} from "./download-types.ts";
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
  DownloadInventorySnapshot,
  DownloadContexts,
  DownloadSelectionInput,
  DownloadSelectionPlan,
  DownloadSubmissionResult,
  DownloadSubmissionFailure,
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
const actions: DownloadAction[] = [
  "pause",
  "resume",
  "retry",
  "abandon",
  "cleanup",
];
const localControl = (action: DownloadAction) =>
  ["pause", "abandon", "cleanup"].includes(action);
const phases: DownloadPhase[] = [
  "queued",
  "downloading",
  "verifying",
  "saving",
  "paused",
  "error",
  "downloaded",
  "abandoned",
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
    ? localControl(action)
    : current.source === task.source &&
      (localControl(action) || Boolean(current.sessionId)));
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
  if (!Array.isArray(raw.tasks) || raw.tasks.length > 1000) return invalid();
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
    if (task.allowedActions.includes("cleanup") && task.phase !== "abandoned")
      return invalid();
    if (
      task.allowedActions.includes("abandon") &&
      !["paused", "error", "queued"].includes(task.phase as string)
    )
      return invalid();
    if (task.allowedActions.includes("resume") && task.phase !== "paused")
      return invalid();
    if (task.allowedActions.includes("retry") && task.phase !== "error")
      return invalid();
    if (
      task.allowedActions.includes("pause") &&
      ["saving", "paused", "error", "downloaded", "abandoned"].includes(
        task.phase as string,
      )
    )
      return invalid();
    return {
      id: rootIdentity(task.id),
      revision: integer(task.revision),
      source: downloadSource(task.source),
      workId: workId(downloadSource(task.source), task.workId),
      title: text(task.title),
      ...(task.tags === undefined
        ? {}
        : {
            tags:
              Array.isArray(task.tags) && task.tags.length <= 1000
                ? task.tags.map((tag) => text(tag))
                : invalid(),
          }),
      ...(task.rootId === undefined
        ? {}
        : { rootId: rootIdentity(task.rootId) }),
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
  if (
    tasks.filter((task) => task.phase === "abandoned").length > 500 ||
    tasks.filter((task) => task.phase !== "abandoned").length > 500
  )
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
export function validateDownloadInventory(
  value: unknown,
): DownloadInventorySnapshot {
  const raw = record(value);
  if (!Array.isArray(raw.items) || raw.items.length > 20500) return invalid();
  const rootId = raw.rootId === null ? null : rootIdentity(raw.rootId);
  const items = raw.items.map((value) => {
    const item = record(value),
      source = downloadSource(item.source);
    if (
      !["present", "missing", "incomplete", "unavailable"].includes(
        String(item.localFiles),
      )
    )
      return invalid();
    return {
      source,
      workId: workId(source, item.workId),
      libraryEntryId: rootIdentity(item.libraryEntryId),
      localFiles:
        item.localFiles as DownloadInventorySnapshot["items"][number]["localFiles"],
    };
  });
  if (
    (rootId === null && items.length > 0) ||
    new Set(items.map((item) => item.source + ":" + item.workId)).size !==
      items.length
  )
    return invalid();
  return {
    revision: integer(raw.revision),
    libraryRevision: integer(raw.libraryRevision),
    rootId,
    items,
  };
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
    inventory: async () =>
      validateDownloadInventory(await call("download_inventory_read")),
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
    prepareBatch: async (context, inputs, retainedBatchIds = []) => {
      if (!Array.isArray(inputs) || inputs.length === 0 || inputs.length > 50)
        return invalid();
      const checked = {
        scope: scope(context.scope),
        rootId: rootIdentity(context.rootId),
        generation: integer(context.generation),
        inputs: inputs.map((input) => text(input.trim(), 2048)),
        retainedBatchIds: retainedBatchIds.map(rootIdentity),
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
    confirmSelection: async (batchIds) => {
      if (
        !batchIds.length ||
        batchIds.length > downloadSelectionLimit ||
        new Set(batchIds).size !== batchIds.length
      )
        return invalid();
      return validateDownloadSnapshot(
        await call("jm_download_selection_confirm", {
          batchIds: batchIds.map(rootIdentity),
        }),
      );
    },
    cancelBatch: async () => {
      await call("jm_download_batch_cancel");
    },
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
          scope: scope(current, localControl(action)),
          taskId: rootIdentity(taskId),
          expectedRevision: integer(expectedRevision),
          action,
        }),
      );
    },
  };
}
export function downloadErrorMessage(cause: unknown): string {
  const metadataCode = downloadMetadataCode(cause);
  if (metadataCode) return downloadMetadataMessages[metadataCode];
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
    return "保存目录或文件暂时无法读取，或原文件身份缺少核对依据，请先核对电脑文件。";
  if (code === "DOWNLOAD_DESTINATION_EXISTS")
    return "该文件名已被其他下载任务或电脑文件占用，请核对冲突后重新准备。现有文件不会被覆盖。";
  if (code.startsWith("DOWNLOAD_INDEX_")) {
    const reason = code.slice("DOWNLOAD_INDEX_".length);
    const reasons: Record<string, string> = {
      LIBRARY_BUSY: "请先完成漫画库目录读取，再重试登记。",
      LIBRARY_LIMIT_REACHED:
        "漫画库记录已达上限，请先整理库记录；直接重试不会释放容量。",
      LIBRARY_IDENTITY_CONFLICT:
        "作品身份与预期不一致，请先核对已有文件，避免反复重试。",
      LIBRARY_FILE_CHANGED: "已保存文件身份发生变化，请先核对文件。",
      DOWNLOAD_OUTPUT_CHANGED: "已保存输出与校验记录不一致，请先核对文件。",
      DOWNLOAD_ROOT_CHANGED: "漫画库目录已改变，请先核对当前目录。",
    };
    if (reasons[reason])
      return "作品保存后的核验或登记未完成。" + reasons[reason];
  }
  if (code === "LIBRARY_BUSY")
    return "电脑目录正在读取或已暂停读取，请完成目录读取后重试当前操作；已有下载进度会保留。";
  if (/BUSY/.test(code)) return "当前任务还在处理，请等待它暂停或完成后再试。";
  if (code === "DOWNLOAD_LIMIT_REACHED")
    return "下载队列已达到 500 条，请先整理完成记录，或暂停并放弃不再需要的任务，再添加任务。";
  if (
    code === "DOWNLOAD_BATCH_INPUT_INVALID" ||
    code === "DOWNLOAD_BATCH_LIMIT"
  )
    return "每行输入一个编号或链接，一次最多选择 500 本；不会截取前 50 本。";
  if (code === "DOWNLOAD_BATCH_DUPLICATE")
    return "本批重复的来源编号，已跳过。";
  if (code === "DOWNLOAD_ABANDONED_LIMIT_REACHED")
    return "已放弃记录已达 500 条，请先在「需要处理」明确清理这些任务的临时文件。最终漫画文件会保留。";
  if (
    code === "DOWNLOAD_CLEANUP_INCOMPLETE" ||
    code === "DOWNLOAD_CLEANUP_REVIEW_REQUIRED"
  )
    return "临时目录含未知或已改变的内容，未完成清理；已放弃记录仍保留，请先核对，程序不会删除最终漫画文件。";
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
  if (code === "DOWNLOAD_WORKER_INTERRUPTED")
    return "下载处理意外中断，队列已停止。已保存进度保留，请重新读取后明确继续或重试。";
  if (/INDEX_/.test(code))
    return "作品已保存，但电脑文件登记尚未完成。重试会先核对已有结果。";
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
    abandoned: "已放弃 · 待清理临时文件",
  })[phase];
export const isDownloadPresent = (task: DownloadTask) =>
  task.phase === "downloaded" && task.localFiles === "present";
export const downloadNeedsAttention = (task: DownloadTask) =>
  task.phase === "error" ||
  task.phase === "abandoned" ||
  (task.phase === "downloaded" && !isDownloadPresent(task));
export const downloadQueueFilters = ["active", "error", "downloaded"] as const;
export type DownloadQueueFilter = (typeof downloadQueueFilters)[number];
export function downloadQueueSummary(tasks: DownloadTask[]) {
  return {
    processing: tasks.filter((task) =>
      ["downloading", "verifying", "saving"].includes(task.phase),
    ).length,
    waiting: tasks.filter((task) => task.phase === "queued").length,
    paused: tasks.filter((task) => task.phase === "paused").length,
    attention: tasks.filter(downloadNeedsAttention).length,
    downloaded: tasks.filter(isDownloadPresent).length,
  };
}
export function downloadCompletedAt(task: DownloadTask): string | null {
  // Downloaded.updatedAt is written only after registration succeeds. File
  // presence checks and relocated-path projection never rewrite that record.
  if (task.phase !== "downloaded" || task.updatedAt <= 0) return null;
  const date = new Date(task.updatedAt);
  return Number.isFinite(date.getTime()) ? date.toISOString() : null;
}
export function downloadAttentionReason(task: DownloadTask): string {
  if (task.phase !== "error") return downloadTaskLabel(task);
  const code = task.errorCode ?? "";
  if (/SESSION|AUTH|ACCOUNT|CREDENTIAL|TOKEN|SOURCE_MISMATCH/.test(code))
    return "需要连接来源账号";
  if (/INDEX_/.test(code)) return "保存后的核验或入库登记未完成";
  if (/VERIFY|MANIFEST|PROOF|INCOMPLETE/.test(code)) return "完整校验未通过";
  if (/ROOT|DIRECTORY|DESTINATION|LIBRARY/.test(code))
    return "漫画库目录需要处理";
  if (
    task.filesTotal !== null &&
    task.filesTotal > 0 &&
    task.filesDone === task.filesTotal
  )
    return "图片已齐，保存或入库未完成";
  return "下载未完成";
}
export interface RecentDownloadBatch {
  taskIds: string[];
  completedTaskIds: string[];
}
export function downloadBatchProgress(
  tasks: DownloadTask[],
  batch: RecentDownloadBatch,
) {
  const ids = new Set(batch.taskIds);
  const members = tasks.filter((task) => ids.has(task.id));
  const completed = new Set(batch.completedTaskIds);
  for (const task of members)
    if (task.phase === "downloaded") completed.add(task.id);
  return {
    total: ids.size,
    completed: [...completed].filter((id) => ids.has(id)).length,
    attention: members.filter(downloadNeedsAttention).length,
  };
}
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
    inputs.length > downloadSelectionLimit ||
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
  tasks
    .filter(
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
          (filter === "active" &&
            !["error", "downloaded", "abandoned"].includes(task.phase))),
    )
    .sort((left, right) =>
      filter === "downloaded"
        ? right.updatedAt - left.updatedAt || left.id.localeCompare(right.id)
        : 0,
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
  failure?: { cause: unknown; occurredAt: number; preparation?: boolean };
  plan: DownloadPlan | null;
  batchPlan: DownloadSelectionPlan | null;
  preparation: { done: number; total: number } | null;
  recentBatch: RecentDownloadBatch | null;
  resuming: { done: number; total: number; stopped: boolean } | null;
  queueNotice: string;
  submittingKeys: string[];
  submissionIssues: DownloadSubmissionFailure[];
  metadataProblems: DownloadMetadataProblem[];
}
export function downloadActionState(
  state: Pick<DownloadState, "snapshot" | "submittingKeys">,
  source: DownloadSource,
  workId: string,
  rootId?: string | null,
): { label: string; disabled: boolean } {
  if (state.submittingKeys.includes(downloadSubmissionKey(source, workId)))
    return { label: "正在加入…", disabled: true };
  const tasks = state.snapshot.tasks.filter(
    (task) =>
      task.source === source &&
      task.workId === workId &&
      (!rootId || !task.rootId || task.rootId === rootId),
  );
  const task =
    tasks.find((item) => item.phase !== "downloaded") ??
    tasks.find(isDownloadPresent);
  if (!task) return { label: "下载到漫画库", disabled: false };
  if (task.phase === "abandoned")
    return { label: "请先清理已放弃记录", disabled: true };
  if (task.phase === "error") return { label: "重试下载", disabled: false };
  if (task.phase === "paused") return { label: "继续下载", disabled: false };
  if (isDownloadPresent(task)) return { label: "已入库", disabled: true };
  return {
    label: task.phase === "queued" ? "已排队" : "下载中",
    disabled: true,
  };
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
    preparation: null,
    recentBatch: null,
    resuming: null,
    queueNotice: "",
    submittingKeys: [],
    submissionIssues: [],
    metadataProblems: [],
  };
  private listeners = new Set<(state: DownloadState) => void>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readPromise: Promise<void> | null = null;
  private readingFiles = false;
  private pendingRecheck = false;
  private readFailures = 0;
  private retryRead = false;
  private epoch = 0;
  private planEpoch = 0;
  private preparedContext: string | null = null;
  private preparedBatchContexts: DownloadContext[] = [];
  private cancellation: Promise<void> = Promise.resolve();
  private submissionTail: Promise<void> = Promise.resolve();
  private pendingSubmissions = new Map<string, number>();
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
    this.state = {
      ...this.state,
      ...next,
      ...(next.error === "" ? { failure: undefined } : {}),
    };
    for (const listener of this.listeners) listener(this.state);
  }
  private fail(cause: unknown, preparation = false) {
    this.publish({
      error: downloadErrorMessage(cause),
      failure: {
        cause,
        occurredAt: Date.now(),
        ...(preparation ? { preparation: true } : {}),
      },
    });
  }
  private recordMetadataProblems(
    source: DownloadSource,
    causes: readonly unknown[],
  ) {
    if (!causes.some((cause) => downloadMetadataCode(cause))) return;
    this.publish({
      metadataProblems: retainDownloadMetadataProblems(
        this.state.metadataProblems,
        source,
        causes,
      ),
    });
  }
  private accept(snapshot: DownloadSnapshot) {
    this.readFailures = 0;
    this.retryRead = false;
    if (snapshot.revision < this.state.snapshot.revision)
      throw new DownloadError("DOWNLOAD_REVISION_STALE");
    const batch = this.state.recentBatch;
    const completed = new Set(batch?.completedTaskIds ?? []);
    if (batch)
      for (const task of snapshot.tasks)
        if (batch.taskIds.includes(task.id) && task.phase === "downloaded")
          completed.add(task.id);
    this.publish({
      snapshot,
      ready: true,
      error: "",
      recentBatch: batch
        ? { ...batch, completedTaskIds: [...completed] }
        : null,
    });
  }
  private rememberBatch(taskIds: string[]) {
    this.publish({
      recentBatch: {
        taskIds,
        completedTaskIds: this.state.snapshot.tasks
          .filter(
            (task) => taskIds.includes(task.id) && task.phase === "downloaded",
          )
          .map((task) => task.id),
      },
    });
  }
  private schedule() {
    clearTimeout(this.timer);
    if (this.pendingRecheck && !this.state.busy) {
      this.timer = setTimeout(() => void this.read(true), 0);
      return;
    }
    if (
      !this.state.busy &&
      ((!this.state.error && this.state.snapshot.tasks.some(activeTask)) ||
        this.retryRead)
    )
      this.timer = setTimeout(
        () => void this.read(false),
        this.retryRead
          ? Math.min(30_000, 1000 * 2 ** Math.min(this.readFailures - 1, 5))
          : 1000,
      );
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
        if (epoch === this.epoch) {
          const code = cause instanceof DownloadError ? cause.code : "";
          this.retryRead =
            /^(BUSY|DOWNLOAD_(UNAVAILABLE|WORKER_BUSY|STORAGE_UNAVAILABLE|READ_FAILED)|STORE_(UNAVAILABLE|READ_FAILED)|READ_FAILED|IO_ERROR)$/.test(
              code,
            );
          this.readFailures++;
          this.fail(cause);
        }
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
  /** The click is the approval. Each submitted work still passes the native
   * prepare/confirm or revision-bound retry gate with its original context. */
  enqueueSelection(
    contexts: DownloadContexts,
    inputs: DownloadSelectionInput[],
    currentContexts: () => DownloadContexts = () => contexts,
  ): Promise<DownloadSubmissionResult> {
    const seenInputs = new Set<string>();
    const requested = inputs
      .filter((item) => {
        const key = downloadSubmissionKey(item.source, item.input);
        if (seenInputs.has(key)) return false;
        seenInputs.add(key);
        return true;
      })
      .map((item) => ({
        source: item.source,
        input: item.input.trim(),
        workId: downloadInputWorkId(item.source, item.input),
      }));
    const failedResult = (code: string): DownloadSubmissionResult => ({
      accepted: [],
      failed: requested.map((item) => ({
        ...item,
        errorCode: code,
        message: downloadErrorMessage(code),
      })),
    });
    if (!requested.length || inputs.length > downloadSelectionLimit)
      return Promise.resolve(failedResult("DOWNLOAD_BATCH_LIMIT"));
    const bound: DownloadContexts = {
      JM: contexts.JM
        ? { ...contexts.JM, scope: { ...contexts.JM.scope } }
        : null,
      Pica: contexts.Pica
        ? { ...contexts.Pica, scope: { ...contexts.Pica.scope } }
        : null,
    };
    const epoch = this.epoch;
    for (const item of requested) {
      const key = downloadSubmissionKey(item.source, item.input);
      this.pendingSubmissions.set(
        key,
        (this.pendingSubmissions.get(key) ?? 0) + 1,
      );
    }
    this.publish({ submittingKeys: [...this.pendingSubmissions.keys()] });
    const finish = (item: DownloadSelectionInput) => {
      if (epoch !== this.epoch) return;
      const key = downloadSubmissionKey(item.source, item.input);
      const count = this.pendingSubmissions.get(key) ?? 0;
      if (count <= 1) this.pendingSubmissions.delete(key);
      else this.pendingSubmissions.set(key, count - 1);
      this.publish({ submittingKeys: [...this.pendingSubmissions.keys()] });
    };
    const checkContext = (source: DownloadSource): DownloadContext => {
      const original = bound[source],
        current = currentContexts()[source];
      if (epoch !== this.epoch)
        throw new DownloadError("DOWNLOAD_REQUEST_CANCELLED");
      if (!original || !current)
        throw new DownloadError("DOWNLOAD_SESSION_REQUIRED");
      if (contextKey(original) !== contextKey(current))
        throw new DownloadError("DOWNLOAD_PLAN_STALE");
      return original;
    };
    const run = async (): Promise<DownloadSubmissionResult> => {
      if (epoch !== this.epoch)
        return failedResult("DOWNLOAD_REQUEST_CANCELLED");
      while (this.state.busy && epoch === this.epoch)
        await new Promise<void>((resolve) => {
          const unsubscribe = this.subscribe((state) => {
            if (!state.busy || epoch !== this.epoch) {
              unsubscribe();
              resolve();
            }
          });
        });
      if (epoch !== this.epoch)
        return failedResult("DOWNLOAD_REQUEST_CANCELLED");
      this.publish({ busy: true, error: "", submissionIssues: [] });
      clearTimeout(this.timer);
      const result: DownloadSubmissionResult = { accepted: [], failed: [] };
      let refreshError: unknown = null;
      try {
        await this.readPromise;
        await this.cancellation;
        if (epoch !== this.epoch)
          return failedResult("DOWNLOAD_REQUEST_CANCELLED");
        try {
          const next = await this.adapter.read(true);
          if (epoch === this.epoch) this.accept(next);
        } catch (cause) {
          refreshError = cause;
        }
        for (const item of requested) {
          if (epoch !== this.epoch)
            return failedResult("DOWNLOAD_REQUEST_CANCELLED");
          try {
            if (refreshError) throw refreshError;
            const context = checkContext(item.source);
            const matching = this.state.snapshot.tasks.filter(
              (task) =>
                task.source === item.source &&
                task.workId === item.workId &&
                (!task.rootId || task.rootId === context.rootId),
            );
            const existing =
              matching.find((task) => task.phase !== "downloaded") ??
              matching.find(isDownloadPresent);
            if (existing) {
              if (isDownloadPresent(existing)) {
                result.accepted.push({
                  ...item,
                  taskId: existing.id,
                  outcome: "present",
                });
                continue;
              }
              if (
                ["queued", "downloading", "verifying", "saving"].includes(
                  existing.phase,
                )
              ) {
                result.accepted.push({
                  ...item,
                  taskId: existing.id,
                  outcome: "existing",
                });
                continue;
              }
              const action = existing.phase === "error" ? "retry" : "resume";
              if (!canControlDownload(existing, action, context.scope))
                throw new DownloadError("DOWNLOAD_WORKER_BUSY");
              checkContext(item.source);
              const next = await this.adapter.control(
                context.scope,
                existing.id,
                existing.revision,
                action,
              );
              if (epoch !== this.epoch)
                return failedResult("DOWNLOAD_REQUEST_CANCELLED");
              if (
                !next.tasks.some(
                  (task) =>
                    task.id === existing.id &&
                    task.source === item.source &&
                    task.workId === item.workId,
                )
              )
                throw new DownloadError("DOWNLOAD_RESPONSE_INVALID");
              this.accept(next);
              result.accepted.push({
                ...item,
                taskId: existing.id,
                outcome: action === "retry" ? "retried" : "resumed",
              });
              continue;
            }
            const plan = await this.adapter
              .prepare(context, item.input)
              .catch((cause) => {
                if (epoch === this.epoch) {
                  checkContext(item.source);
                  this.recordMetadataProblems(item.source, [cause]);
                }
                throw cause;
              });
            checkContext(item.source);
            if (
              plan.source !== item.source ||
              plan.rootId !== context.rootId ||
              plan.generation !== context.generation ||
              (item.workId !== null && plan.workId !== item.workId)
            )
              throw new DownloadError("DOWNLOAD_PLAN_STALE");
            const next = await this.adapter.confirm(plan.planId, plan.revision);
            if (epoch !== this.epoch)
              return failedResult("DOWNLOAD_REQUEST_CANCELLED");
            if (
              !next.tasks.some(
                (task) =>
                  task.id === plan.planId &&
                  task.source === plan.source &&
                  task.workId === plan.workId,
              )
            )
              throw new DownloadError("DOWNLOAD_RESPONSE_INVALID");
            this.accept(next);
            result.accepted.push({
              ...item,
              workId: plan.workId,
              taskId: plan.planId,
              outcome: "queued",
            });
          } catch (cause) {
            const rawCode = (cause as { code?: unknown })?.code;
            const code =
              typeof rawCode === "string" && /^[A-Z0-9_]{1,100}$/.test(rawCode)
                ? rawCode
                : "DOWNLOAD_UNAVAILABLE";
            if (code === "DOWNLOAD_ALREADY_PRESENT" && item.workId) {
              result.accepted.push({
                ...item,
                taskId: null,
                outcome: "present",
              });
            } else
              result.failed.push({
                ...item,
                errorCode: code,
                message: downloadErrorMessage(code),
              });
          } finally {
            finish(item);
          }
        }
        if (epoch === this.epoch) {
          this.rememberBatch([
            ...new Set(
              result.accepted.flatMap((item) =>
                item.taskId ? [item.taskId] : [],
              ),
            ),
          ]);
          // Per-item failures are separate from queue-read health: a bad input
          // must not stop polling successful work from this same batch.
          this.publish({ submissionIssues: result.failed });
        }
        return result;
      } finally {
        if (epoch === this.epoch) {
          this.publish({ busy: false });
          this.schedule();
        }
      }
    };
    const result = this.submissionTail.then(run, run);
    this.submissionTail = result.then(
      () => {},
      () => {},
    );
    return result;
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
      if (epoch === this.epoch && token === this.planEpoch) {
        this.recordMetadataProblems(context.scope.source, [cause]);
        this.fail(cause, true);
      }
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  cancelPlan() {
    const hadBatch =
      this.state.batchPlan !== null || this.state.preparation !== null;
    this.planEpoch++;
    this.preparedContext = null;
    this.preparedBatchContexts = [];
    this.publish({ plan: null, batchPlan: null, preparation: null });
    if (hadBatch && this.adapter.cancelBatch) {
      // New preparation waits for cancellation so a late cancel cannot erase it.
      this.cancellation = this.cancellation
        .then(() => this.adapter.cancelBatch())
        .catch(() => {});
    }
  }
  async prepareBatch(
    context: DownloadContext,
    inputs: string[],
  ): Promise<void> {
    const contexts: DownloadContexts = { JM: null, Pica: null };
    contexts[context.scope.source] = context;
    return this.prepareSelection(
      contexts,
      inputs.map((input) => ({ source: context.scope.source, input })),
    );
  }
  async prepareSelection(
    contexts: DownloadContexts,
    inputs: DownloadSelectionInput[],
  ): Promise<void> {
    if (this.state.busy) return;
    this.cancelPlan();
    const token = this.planEpoch,
      epoch = this.epoch;
    this.publish({
      busy: true,
      error: "",
      preparation: { done: 0, total: inputs.length },
    });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      await this.cancellation;
      if (epoch !== this.epoch || token !== this.planEpoch) return;
      if (!inputs.length || inputs.length > downloadSelectionLimit)
        throw new DownloadError("DOWNLOAD_BATCH_LIMIT");
      const groups = new Map<DownloadSource, string[]>();
      for (const item of inputs) {
        if (
          !contexts[item.source] ||
          !item.input.trim() ||
          item.input.length > 2048
        )
          throw new DownloadError("DOWNLOAD_PLAN_STALE");
        const group = groups.get(item.source) ?? [];
        group.push(item.input.trim());
        groups.set(item.source, group);
      }
      const used = [...groups.keys()].map((source) => contexts[source]!);
      if (
        used.some(
          (context) =>
            context.rootId !== used[0].rootId ||
            context.generation !== used[0].generation,
        )
      )
        throw new DownloadError("DOWNLOAD_PLAN_STALE");
      const batchPlan: DownloadSelectionPlan = {
        batchId: null,
        batchIds: [],
        plans: [],
        issues: [],
      };
      let done = 0;
      for (const [source, values] of groups) {
        const context = contexts[source]!;
        for (
          let offset = 0;
          offset < values.length;
          offset += downloadPreparationChunk
        ) {
          if (epoch !== this.epoch || token !== this.planEpoch) return;
          const chunk = values.slice(offset, offset + downloadPreparationChunk);
          const next = await this.adapter
            .prepareBatch(context, chunk, [...batchPlan.batchIds])
            .catch((cause) => {
              if (epoch === this.epoch && token === this.planEpoch)
                this.recordMetadataProblems(source, [cause]);
              throw cause;
            });
          if (epoch !== this.epoch || token !== this.planEpoch) return;
          if (
            next.plans.length + next.issues.length !== chunk.length ||
            next.plans.some(
              (plan) =>
                plan.source !== source ||
                plan.rootId !== context.rootId ||
                plan.generation !== context.generation ||
                batchPlan.plans.some(
                  (old) =>
                    old.planId === plan.planId ||
                    (old.source === plan.source && old.workId === plan.workId),
                ),
            )
          )
            throw new DownloadError("DOWNLOAD_PLAN_STALE");
          if (next.batchId) {
            if (batchPlan.batchIds.includes(next.batchId))
              throw new DownloadError("DOWNLOAD_PLAN_STALE");
            batchPlan.batchIds.push(next.batchId);
          }
          this.recordMetadataProblems(
            source,
            next.issues.map((issue) => issue.errorCode),
          );
          batchPlan.plans.push(...next.plans);
          batchPlan.issues.push(
            ...next.issues.map((issue) => ({
              ...issue,
              input: `${source} · ${issue.input}`,
            })),
          );
          done += chunk.length;
          this.publish({ preparation: { done, total: inputs.length } });
        }
      }
      batchPlan.batchId = batchPlan.batchIds[0] ?? null;
      if (epoch === this.epoch && token === this.planEpoch) {
        this.preparedBatchContexts = used;
        this.publish({ batchPlan });
      }
    } catch (cause) {
      if (epoch === this.epoch && token === this.planEpoch) {
        this.cancelPlan();
        this.fail(cause, true);
      }
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false, preparation: null });
        this.schedule();
      }
    }
  }
  async confirmBatch(
    context: DownloadContext | DownloadContexts,
  ): Promise<boolean> {
    const batch = this.state.batchPlan;
    if (!batch?.batchId || this.state.busy) return false;
    const contexts: DownloadContexts =
      "scope" in context
        ? { JM: null, Pica: null, [context.scope.source]: context }
        : context;
    if (
      this.preparedBatchContexts.some(
        (previous) =>
          !contexts[previous.scope.source] ||
          contextKey(previous) !== contextKey(contexts[previous.scope.source]!),
      )
    ) {
      this.cancelPlan();
      this.fail("DOWNLOAD_PLAN_STALE");
      return false;
    }
    const epoch = this.epoch;
    this.publish({ busy: true, error: "" });
    clearTimeout(this.timer);
    try {
      await this.readPromise;
      if (epoch !== this.epoch) return false;
      const next =
        batch.batchIds.length === 1
          ? await this.adapter.confirmBatch(batch.batchId)
          : await this.adapter.confirmSelection(batch.batchIds);
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
      this.rememberBatch(batch.plans.map((plan) => plan.planId));
      this.cancelPlan();
      return true;
    } catch (cause) {
      if (epoch === this.epoch) {
        this.cancelPlan();
        this.fail(cause);
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
      if (epoch === this.epoch) this.fail(cause);
      return false;
    } finally {
      if (epoch === this.epoch) {
        this.publish({ busy: false });
        this.schedule();
      }
    }
  }
  pauseAll() {
    this.stopResume();
    return this.changeQueue(async () => {
      const next = await this.adapter.pauseAll();
      this.publish({
        queueNotice: next.tasks.some((task) => task.phase === "saving")
          ? "后续任务已暂停；当前作品的保存、校验和登记将完成。"
          : "队列已暂停，进度已保留。",
      });
      return next;
    });
  }
  stopResume() {
    this.planEpoch++;
    if (this.state.resuming)
      this.publish({ resuming: { ...this.state.resuming, stopped: true } });
  }
  async resumeAll(
    context: DownloadContext,
    tasks: DownloadTask[],
  ): Promise<boolean> {
    if (
      this.state.busy ||
      !tasks.length ||
      tasks.length > 500 ||
      new Set(tasks.map((task) => task.id)).size !== tasks.length ||
      tasks.some(
        (task) =>
          !canControlDownload(task, "resume", context.scope) ||
          (task.rootId && task.rootId !== context.rootId),
      )
    )
      return false;
    const epoch = this.epoch,
      token = ++this.planEpoch;
    this.publish({
      busy: true,
      error: "",
      resuming: { done: 0, total: tasks.length, stopped: false },
    });
    clearTimeout(this.timer);
    let done = 0;
    try {
      await this.readPromise;
      for (let offset = 0; offset < tasks.length; offset += 50) {
        if (epoch !== this.epoch || token !== this.planEpoch) return false;
        const chunk = tasks.slice(offset, offset + 50);
        if (
          chunk.some(
            (expected) =>
              !this.state.snapshot.tasks.some(
                (current) =>
                  current.id === expected.id &&
                  current.revision === expected.revision &&
                  current.rootId === expected.rootId &&
                  canControlDownload(current, "resume", context.scope),
              ),
          )
        )
          throw new DownloadError("DOWNLOAD_TASK_STALE");
        const next = await this.adapter.resumeMany(
          context.scope,
          chunk.map((task) => ({
            taskId: task.id,
            expectedRevision: task.revision,
          })),
        );
        if (epoch !== this.epoch) return false;
        if (
          chunk.some(
            (expected) =>
              !next.tasks.some(
                (current) =>
                  current.id === expected.id &&
                  current.source === expected.source &&
                  current.workId === expected.workId &&
                  current.revision > expected.revision,
              ),
          )
        )
          invalid();
        this.accept(next);
        done += chunk.length;
        this.publish({
          resuming: {
            done,
            total: tasks.length,
            stopped: token !== this.planEpoch,
          },
        });
      }
      return token === this.planEpoch;
    } catch (cause) {
      if (epoch === this.epoch) this.fail(cause);
      return false;
    } finally {
      if (epoch === this.epoch) {
        this.publish({
          busy: false,
          resuming: { done, total: tasks.length, stopped: done < tasks.length },
        });
        this.schedule();
      }
    }
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
      this.fail("DOWNLOAD_PLAN_STALE");
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
      this.rememberBatch([plan.planId]);
      this.cancelPlan();
      return true;
    } catch (cause) {
      if (epoch === this.epoch) {
        this.cancelPlan();
        this.fail(cause);
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
      this.fail(
        current ? "DOWNLOAD_SOURCE_MISMATCH" : "DOWNLOAD_SESSION_REQUIRED",
      );
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
        action === "cleanup"
          ? next.tasks.some((result) => result.id === task.id)
          : !next.tasks.some(
              (result) =>
                result.id === task.id &&
                result.source === task.source &&
                result.workId === task.workId,
            )
      )
        return invalid();
      if (epoch === this.epoch) this.accept(next);
    } catch (cause) {
      if (epoch === this.epoch) this.fail(cause);
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
    this.retryRead = false;
    this.readFailures = 0;
    this.pendingSubmissions.clear();
    this.state = {
      ...this.state,
      reading: false,
      busy: false,
      plan: null,
      batchPlan: null,
      preparation: null,
      submittingKeys: [],
    };
    for (const listener of this.listeners) listener(this.state);
    this.listeners.clear();
  }
}
