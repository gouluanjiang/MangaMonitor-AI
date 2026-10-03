import type { AccountSummary, Source } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { DownloadSnapshot } from "./download-types.ts";
import { isDownloadPresent } from "./download-runtime.ts";
import { fileNeedsReview } from "./library-matching.ts";
import { downloadMetadataMessages } from "./download-metadata-problems.ts";

export interface WorkbenchInfo {
  version: string;
  revision: string | null;
  platform: string;
}
export function validateWorkbenchInfo(value: unknown): WorkbenchInfo {
  const v = value as Partial<WorkbenchInfo> | null;
  if (
    !v ||
    typeof v.version !== "string" ||
    v.version.trim() !== v.version ||
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-rc\.(0|[1-9]\d*))?$/.test(
      v.version,
    ) ||
    !(
      v.revision === null ||
      (typeof v.revision === "string" && /^[a-f0-9]{40}$/i.test(v.revision))
    ) ||
    !["windows", "linux", "macos"].includes(v.platform ?? "")
  )
    throw new Error("VERSION_UNAVAILABLE");
  return { version: v.version, revision: v.revision, platform: v.platform! };
}
export const accountStateLabels: Record<AccountSummary["state"], string> = {
  connected: "已连接",
  disconnected: "未连接",
  expired: "需要重新登录",
  unavailable: "暂不可用",
};
export const libraryPhaseLabels: Record<LibrarySnapshot["phase"], string> = {
  idle: "尚未读取",
  reading: "正在读取",
  paused: "读取已暂停",
  complete: "目录已读完",
  error: "读取未完成",
};
const diagnosticOperations = {
  accounts: "账号连接",
  library: "漫画库读取",
  downloads: "下载队列",
  downloadPreparation: "准备下载",
  preferences: "设置读取或保存",
};
export type DiagnosticOperation = keyof typeof diagnosticOperations;
export interface DiagnosticProblem {
  operation: DiagnosticOperation;
  code: string;
  occurredAt: number;
  source?: Source;
}
// Exact allowlist, never a prefix/regex pass-through of native diagnostic text.
const diagnosticMessages: Readonly<Record<string, string>> = {
  ...downloadMetadataMessages,
  BUSY: "本机资料正忙，请稍后重试。",
  LIBRARY_BUSY: "漫画库正忙，请稍后重试。",
  REVISION_CONFLICT: "资料已被另一项操作更新，请重新读取后重试。",
  STORE_UNAVAILABLE: "本机资料暂时无法读写，请检查磁盘与权限。",
  DOCUMENT_CORRUPT: "保存的资料无法校验，请保留原资料并反馈问题。",
  UNSUPPORTED_SCHEMA: "资料格式不受此版本支持，请使用兼容版本。",
  VALIDATION_FAILED: "资料未通过校验，请保留原资料并反馈问题。",
  LIBRARY_UNAVAILABLE: "漫画库暂时无法读取，请检查目录后重试。",
  LIBRARY_ROOT_CHANGED: "漫画库目录已变化，请重新选择并核对目录。",
  LIBRARY_ROOT_MISSING: "漫画库目录暂时无法找到，请检查目录。",
  LIBRARY_DIRECTORY_INVALID: "漫画库目录不可用，请重新选择有效目录。",
  LIBRARY_IDENTITY_CONFLICT: "漫画库登记身份存在冲突，需要先核对。",
  LIBRARY_LIMIT_REACHED: "漫画库记录已达到上限，需要先整理记录。",
  DOWNLOAD_UNAVAILABLE: "下载队列暂时不可用，请重新读取队列。",
  DOWNLOAD_LIMIT_REACHED: "下载记录已达到上限，请先整理队列。",
  DOWNLOAD_INDEX_FAILED: "文件已保存，但入库登记尚未完成，请到队列核对。",
  DOWNLOAD_SOURCE_CHANGED: "来源内容已变化，请到队列重新核对。",
  DOWNLOAD_ROOT_CHANGED: "下载目录已变化，请核对目录后重试。",
  DOWNLOAD_FILE_CHANGED: "目标文件已变化，请先核对，避免覆盖。",
  DOWNLOAD_WORKER_FAILED: "下载执行意外中断，请到队列查看恢复操作。",
  DOWNLOAD_DISK_FULL: "磁盘可用空间不足，请清理空间后重试。",
  SOURCE_UNAVAILABLE: "来源暂时无法读取，请稍后重试。",
  SOURCE_NETWORK_ERROR: "来源网络请求失败，请检查网络后重试。",
  DOWNLOAD_NETWORK_ERROR: "下载网络请求失败，请检查网络后重试。",
  NETWORK_ERROR: "网络请求失败，请检查网络后重试。",
  REQUEST_TIMEOUT: "请求超时，请稍后重试。",
  SOURCE_RATE_LIMITED: "来源暂时限制请求，请稍后重试。",
  SOURCE_SESSION_EXPIRED: "来源会话已过期，请重新连接账号。",
  SESSION_EXPIRED: "来源会话已过期，请重新连接账号。",
  LOGIN_REQUIRED: "此操作需要先连接来源账号。",
  AUTH_REQUIRED: "此操作需要先连接来源账号。",
  AUTH_EXPIRED: "来源会话已过期，请重新连接账号。",
  UNAUTHORIZED: "来源未接受当前会话，请重新连接账号。",
  UNCLASSIFIED_ERROR: "操作未完成，请到对应页面查看提示并重试。",
};
export function createDiagnosticProblem(
  operation: DiagnosticOperation,
  error: unknown,
  occurredAt = Date.now(),
  source?: Source,
): DiagnosticProblem {
  const code =
    typeof error === "string"
      ? error
      : error &&
          typeof error === "object" &&
          "code" in error &&
          typeof error.code === "string"
        ? error.code
        : error instanceof Error
          ? error.message
          : "";
  return {
    operation,
    code: Object.hasOwn(diagnosticMessages, code) ? code : "UNCLASSIFIED_ERROR",
    occurredAt,
    ...(source === "JM" || source === "Pica" ? { source } : {}),
  };
}
export function diagnosticProblemLines(
  problems: readonly DiagnosticProblem[] = [],
): string[] {
  return problems.slice(0, 20).flatMap((problem) => {
    if (
      !problem ||
      !Object.hasOwn(diagnosticOperations, problem.operation) ||
      !Number.isSafeInteger(problem.occurredAt) ||
      problem.occurredAt <= 0 ||
      problem.occurredAt > 8_640_000_000_000_000
    )
      return [];
    const safe = createDiagnosticProblem(
      problem.operation,
      problem.code,
      problem.occurredAt,
      problem.source,
    );
    return [
      `${diagnosticOperations[safe.operation]}${safe.source ? ` · ${safe.source}` : ""} · ${new Date(safe.occurredAt).toISOString()} · ${safe.code}：${diagnosticMessages[safe.code]}`,
    ];
  });
}
export interface DiagnosticState {
  info: WorkbenchInfo | null;
  accounts: AccountSummary[];
  accountsLoading: boolean;
  accountsFailed: boolean;
  library: LibrarySnapshot;
  libraryFailed: boolean;
  downloads: DownloadSnapshot;
  downloadsReady: boolean;
  downloadsFailed: boolean;
  preferencesReady: boolean;
  preferencesFailed: boolean;
  problems?: DiagnosticProblem[];
}
// Build from a small allowlist; never serialize whole account, file or task DTOs.
export function diagnosticSummary(state: DiagnosticState): string {
  const { info, library, downloads } = state;
  const lines = [
    "MangaMonitor 状态摘要",
    info
      ? `版本：${info.version} · ${info.revision?.slice(0, 7) ?? "本地构建"} · ${info.platform}`
      : "版本：未取得",
    `设置：${state.preferencesFailed ? "读取或保存有问题" : state.preferencesReady ? "已读取" : "尚未读取"}`,
  ];
  for (const source of ["JM", "Pica"] as const) {
    const account = state.accounts.find((a) => a.source === source);
    lines.push(
      `${source} 会话：${state.accountsLoading ? "正在读取" : state.accountsFailed ? "读取未完成" : account ? accountStateLabels[account.state] : "尚未读取"}`,
    );
  }
  lines.push(
    `漫画库：${state.libraryFailed ? "读取未完成" : !library.rootId ? "未选择目录" : libraryPhaseLabels[library.phase]}`,
  );
  lines.push(
    `目录记录：${library.items.length} · 文件待核对：${library.items.filter(fileNeedsReview).length} · 跳过：${library.skipped}`,
  );
  lines.push(
    `队列：${state.downloadsFailed ? "读取或操作有问题" : state.downloadsReady ? "已读取" : "尚未读取"}`,
  );
  lines.push(
    `任务：${downloads.tasks.length} · 已下载且文件存在：${downloads.tasks.filter(isDownloadPresent).length} · 处理中：${downloads.tasks.filter((task) => ["queued", "downloading", "verifying", "saving"].includes(task.phase)).length} · 暂停：${downloads.tasks.filter((task) => task.phase === "paused").length} · 需处理：${downloads.tasks.filter((task) => task.phase === "error" || (task.phase === "downloaded" && !isDownloadPresent(task))).length}`,
  );
  lines.push(
    ...diagnosticProblemLines(state.problems),
    "范围：当前应用记录；会话状态不代表刚完成来源连通性测试。",
    "入库口径：同来源成功下载记录与实际文件；旧漫画及跨站同本不自动匹配。",
  );
  return lines.join("\n");
}
