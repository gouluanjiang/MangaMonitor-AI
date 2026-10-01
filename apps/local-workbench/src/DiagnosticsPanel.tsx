import { useEffect, useRef, useState } from "react";
import { invokeDesktop } from "./runtime.ts";
import {
  accountStateLabels,
  diagnosticSummary,
  diagnosticProblemLines,
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
  const [copyReceipt, setCopyReceipt] = useState<{
    text: string;
    success: boolean;
    sequence: number;
  } | null>(null);
  const [copying, setCopying] = useState(false);
  const copySequence = useRef(0);
  const mounted = useRef(true);
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
  const problemLines = diagnosticProblemLines(state.problems);
  const displayedReport =
    copyReceipt && !copyReceipt.success ? copyReceipt.text : report;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (!copyReceipt) return;
    if (!copyReceipt.success) {
      reportField.current?.focus();
      reportField.current?.select();
      return;
    }
    const timer = setTimeout(() => setCopyReceipt(null), 6000);
    return () => clearTimeout(timer);
  }, [copyReceipt]);
  async function copyReport() {
    if (copying) return;
    setCopying(true);
    const text = report;
    const sequence = ++copySequence.current;
    try {
      await navigator.clipboard.writeText(text);
      if (mounted.current) setCopyReceipt({ text, success: true, sequence });
    } catch {
      if (mounted.current) setCopyReceipt({ text, success: false, sequence });
    } finally {
      if (mounted.current) setCopying(false);
    }
  }
  return (
    <section
      className="settings-card diagnostics-card"
      aria-labelledby="diagnostics-title"
      data-testid="diagnostics-panel"
    >
      <h2 id="diagnostics-title">网络与诊断</h2>
      <p className="settings-copy">
        查看本次运行的状态，前往对应页面处理问题。作品信息校验失败会保留最近 20
        条诊断；退出程序后清空。
      </p>
      <p className="settings-help" data-testid="diagnostics-version">
        {info
          ? `MangaMonitor ${info.version} · ${info.revision?.slice(0, 7) ?? "本地构建"}`
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
      {problemLines.length > 0 && (
        <div className="settings-help" data-testid="diagnostics-problems">
          <h3>最近的操作问题</h3>
          <ul>
            {problemLines.map((line, index) => (
              <li key={index}>{line}</li>
            ))}
          </ul>
        </div>
      )}
      <h3>反馈问题</h3>
      <p className="settings-help">
        可复制下方包含版本、状态、数量及受控错误原因和时间的摘要，不含账号、路径或作品信息。
      </p>
      <textarea
        ref={reportField}
        className="diagnostic-summary"
        data-testid="diagnostic-summary"
        aria-label="诊断摘要"
        readOnly
        value={displayedReport}
        rows={10}
      />
      <button
        className="button secondary"
        disabled={copying}
        onClick={() => void copyReport()}
      >
        复制诊断摘要
      </button>
      {copyReceipt && (
        <p role="status">
          {copyReceipt.success
            ? "诊断摘要已复制。"
            : "无法自动复制，请复制下方已选中的文字。"}
        </p>
      )}
      {copyReceipt && copyReceipt.text !== report && (
        <p className="settings-help">
          {copyReceipt.success
            ? "状态已有变化，刚才复制的是点击时的摘要。"
            : "下方保留本次复制的摘要，当前状态已有变化。"}
          {!copyReceipt.success && (
            <button
              className="text-button"
              onClick={() => setCopyReceipt(null)}
            >
              显示最新摘要
            </button>
          )}
        </p>
      )}
    </section>
  );
}
