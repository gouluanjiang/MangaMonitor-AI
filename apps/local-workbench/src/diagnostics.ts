import type { AccountSummary } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { DownloadSnapshot } from "./download-types.ts";
import { isDownloadPresent } from "./download-runtime.ts";
import { fileNeedsReview } from "./library-matching.ts";

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
    !/^\d+\.\d+\.\d+$/.test(v.version) ||
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
    "范围：当前应用记录；会话状态不代表刚完成来源连通性测试。",
    "入库口径：同来源成功下载记录与实际文件；旧漫画及跨站同本不自动匹配。",
  );
  return lines.join("\n");
}
