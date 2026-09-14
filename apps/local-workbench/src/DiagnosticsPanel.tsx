import { useEffect, useRef, useState } from "react";
import { invokeDesktop } from "./runtime.ts";
import {
  accountStateLabels,
  diagnosticSummary,
  libraryPhaseLabels,
  validateWorkbenchInfo,
} from "./diagnostics.ts";
import type { DiagnosticState, WorkbenchInfo } from "./diagnostics.ts";
import type { SettingsPage } from "./settings-navigation.ts";

export function DiagnosticsPanel({
  state,
  onOpenSettings,
  onOpenQueue,
  onReloadAccounts,
}: {
  state: Omit<DiagnosticState, "info">;
  onOpenSettings(page: SettingsPage): void;
  onOpenQueue(): void;
  onReloadAccounts(): Promise<void>;
}) {
  const [info, setInfo] = useState<WorkbenchInfo | null>(null);
  const [infoFailed, setInfoFailed] = useState(false);
  const [retry, setRetry] = useState(0);
  const [copyNotice, setCopyNotice] = useState("");
  const reportField = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    let active = true;
    setInfoFailed(false);
    void invokeDesktop("workbench_info")
      .then(validateWorkbenchInfo)
      .then((value) => {
        if (active) setInfo(value);
      })
      .catch(() => {
        if (active) setInfoFailed(true);
      });
    return () => {
      active = false;
    };
  }, [retry]);
  const report = diagnosticSummary({ ...state, info });
  useEffect(() => setCopyNotice(""), [report]);
  async function copyReport() {
    try {
      await navigator.clipboard.writeText(report);
      setCopyNotice("诊断摘要已复制。");
    } catch {
      reportField.current?.select();
      setCopyNotice("无法自动复制，请复制下方已选中的文字。");
    }
  }
  return (
    <section
      className="settings-card"
      aria-labelledby="diagnostics-title"
      data-testid="diagnostics-panel"
    >
      <h2 id="diagnostics-title">网络与诊断</h2>
      <p className="settings-copy">
        查看本次运行的状态，前往对应页面处理问题。
      </p>
      <p className="settings-help" data-testid="diagnostics-version">
        {info
          ? `MangaMonitor Dev ${info.version} · ${info.revision?.slice(0, 7) ?? "本地构建"}`
          : infoFailed
            ? "版本信息暂时无法读取。"
            : "正在读取版本…"}
        {infoFailed && (
          <button
            className="text-button"
            onClick={() => setRetry((value) => value + 1)}
          >
            重新读取版本
          </button>
        )}
      </p>
      <h3>账号与来源</h3>
      <dl className="settings-facts">
        {(["JM", "Pica"] as const).map((source) => {
          const account = state.accounts.find(
            (entry) => entry.source === source,
          );
          return (
            <div key={source}>
              <dt>{source === "Pica" ? "哔咔" : source}</dt>
              <dd>
                {state.accountsLoading
                  ? "正在读取"
                  : state.accountsFailed
                    ? "读取未完成"
                    : account
                      ? accountStateLabels[account.state]
                      : "尚未读取"}
              </dd>
            </div>
          );
        })}
      </dl>
      <p className="settings-help">
        这里显示账号会话状态。实际来源能否读取，以收藏、搜索或下载时的结果为准。
      </p>
      <div className="source-actions">
        <button
          className="button secondary"
          disabled={state.accountsLoading}
          onClick={() => void onReloadAccounts()}
        >
          重新读取账号状态
        </button>
        <button
          className="text-button"
          onClick={() => onOpenSettings("accounts")}
        >
          管理账号
        </button>
      </div>
      <h3>漫画库与下载</h3>
      <dl className="settings-facts">
        <div>
          <dt>漫画库</dt>
          <dd>
            {state.libraryFailed
              ? "读取未完成"
              : !state.library.rootId
                ? "未选择目录"
                : libraryPhaseLabels[state.library.phase]}{" "}
            · {state.library.items.length} 部
          </dd>
        </div>
        <div>
          <dt>下载队列</dt>
          <dd>
            {state.downloadsFailed
              ? "需要处理"
              : state.downloadsReady
                ? `${state.downloads.tasks.length} 条任务`
                : "尚未读取"}
          </dd>
        </div>
        <div>
          <dt>外观与设置</dt>
          <dd>
            {state.preferencesFailed
              ? "读取或保存有问题"
              : state.preferencesReady
                ? "已读取"
                : "尚未读取"}
          </dd>
        </div>
      </dl>
      <div className="source-actions">
        <button
          className="button secondary"
          onClick={() => onOpenSettings("library")}
        >
          漫画库设置
        </button>
        <button className="button secondary" onClick={onOpenQueue}>
          查看下载队列
        </button>
      </div>
      <h3>反馈问题</h3>
      <p className="settings-help">
        可复制下方仅含版本、状态和数量的摘要，便于反馈。
      </p>
      <textarea
        ref={reportField}
        className="diagnostic-summary"
        data-testid="diagnostic-summary"
        aria-label="诊断摘要"
        readOnly
        value={report}
        rows={10}
      />
      <button className="button secondary" onClick={() => void copyReport()}>
        复制诊断摘要
      </button>
      {copyNotice && <p role="status">{copyNotice}</p>}
    </section>
  );
}
