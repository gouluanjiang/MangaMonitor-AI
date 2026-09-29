import { useEffect, useRef, useState } from "react";
import type { AccountSummary, Source, SourceAdapter } from "./source-types.ts";
import { sourceLabel, sources } from "./source-types.ts";
import { SourceError, sourceErrorMessage } from "./source-runtime.ts";
import "./source-workbench.css";

export interface AccountSettingsProps {
  adapter: SourceAdapter;
  accounts: AccountSummary[];
  onAccountsChange(updates: AccountSummary[]): void;
  onOpenFavorites(source: Source): void;
  loadingAccounts?: boolean;
}
const accountStateLabels: Record<AccountSummary["state"], string> = {
  connected: "已连接",
  disconnected: "未连接",
  expired: "需要重新登录",
  unavailable: "暂不可用",
};
export function AccountSettings({
  adapter,
  accounts,
  onAccountsChange,
  onOpenFavorites,
  loadingAccounts = false,
}: AccountSettingsProps) {
  const [loginSource, setLoginSource] = useState<Source | null>(null);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(false);
  const [rememberLogin, setRememberLogin] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const lock = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const currentAccounts = useRef(accounts);
  currentAccounts.current = accounts;
  useEffect(() => {
    if (loginSource && dialog.current && !dialog.current.open)
      dialog.current.showModal();
  }, [loginSource]);
  const unavailable = !adapter.available;
  function close() {
    if (lock.current) return;
    setPassword("");
    setUsername("");
    setRemember(false);
    setRememberLogin(false);
    setLoginSource(null);
    setError("");
  }
  function open(source: Source) {
    if (lock.current || loadingAccounts || unavailable) return;
    setLoginSource(source);
    setUsername("");
    setPassword("");
    const account = currentAccounts.current.find(
      (item) => item.source === source,
    );
    const savedLogin = source === "JM" && account?.rememberLogin === true;
    setRemember(savedLogin || account?.remembered === true);
    setRememberLogin(savedLogin);
    setError("");
    setNotice("");
  }
  async function refresh() {
    if (lock.current || loadingAccounts) return;
    lock.current = true;
    setPending(true);
    setError("");
    setNotice("");
    try {
      onAccountsChange(await adapter.accounts(true));
    } catch (cause) {
      setError(sourceErrorMessage(cause));
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  async function login() {
    if (
      lock.current ||
      loadingAccounts ||
      !loginSource ||
      !username.trim() ||
      !password
    )
      return;
    const input = {
      source: loginSource,
      username: username.trim(),
      password,
      remember,
      rememberLogin: loginSource === "JM" && remember && rememberLogin,
    };
    // Clear the displayed secret before starting IPC. It is never placed in persistent state.
    setPassword("");
    lock.current = true;
    setPending(true);
    setError("");
    setNotice("");
    try {
      const account = await adapter.login(input);
      onAccountsChange([account]);
      setLoginSource(null);
      setUsername("");
      setNotice(
        sourceLabel(account.source) + " 已连接。可前往在线收藏读取作品。",
      );
    } catch (cause) {
      setError(sourceErrorMessage(cause));
    } finally {
      input.password = "";
      lock.current = false;
      setPending(false);
    }
  }
  async function logout(account: AccountSummary) {
    const scope = { source: account.source, sessionId: account.sessionId };
    if (lock.current || loadingAccounts) return;
    lock.current = true;
    setPending(true);
    setError("");
    setNotice("");
    try {
      onAccountsChange([await adapter.logout(scope)]);
      setNotice(
        sourceLabel(account.source) + " 已退出登录。电脑漫画与下载记录保留。",
      );
    } catch (cause) {
      setError(sourceErrorMessage(cause));
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  return (
    <section
      className="source-accounts"
      data-testid="source-account-settings"
      aria-busy={pending || loadingAccounts}
    >
      <h2>账号连接</h2>
      <p className="settings-copy">JM 与哔咔分别连接和管理收藏。</p>
      {loadingAccounts && <p role="status">正在恢复账号会话…</p>}
      {unavailable && (
        <p className="source-notice" role="status">
          请在桌面应用中连接账号。浏览器预览不会登录或读取真实来源。
        </p>
      )}
      {sources.map((source) => {
        const account = accounts.find((item) => item.source === source);
        const connected = account?.state === "connected";
        return (
          <div
            className="source-account-block"
            key={source}
            data-testid={"account-" + source}
          >
            <div className="source-account-info">
              <div className="source-account-title">
                <h3>{sourceLabel(source)}</h3>
                <span
                  className={connected ? "source-connected" : "source-muted"}
                >
                  {loadingAccounts
                    ? "正在恢复"
                    : account
                      ? account.state === "expired" && account.remembered
                        ? "连接需恢复"
                        : accountStateLabels[account.state]
                      : "尚未读取"}
                </span>
              </div>
              {connected && (
                <p>
                  {account.displayName ?? account.accountId ?? "已连接账号"}
                </p>
              )}
              <p className="source-muted">
                {connected
                  ? account.rememberLogin
                    ? "已保存登录信息，打开应用或重新读取账号状态时可恢复连接。"
                    : account.remembered
                      ? "已保存会话"
                      : "仅本次应用会话"
                  : account?.rememberLogin
                    ? "已保存登录信息，可先重新读取账号状态尝试恢复连接。"
                    : account?.remembered
                      ? "已保存会话，可先重新读取账号状态尝试恢复连接。"
                      : "连接账号后可读取和更新网站收藏。"}
              </p>
              {!loadingAccounts && account?.errorCode && (
                <p className="source-warning">
                  {account.remembered &&
                  /SESSION|LOGIN_REQUIRED|AUTH_REQUIRED|AUTH_EXPIRED|UNAUTHORIZED/.test(
                    account.errorCode,
                  )
                    ? "保存的连接暂未恢复，可先重新读取账号状态；若仍无法恢复，再重新登录。"
                    : sourceErrorMessage(new SourceError(account.errorCode))}
                </p>
              )}
            </div>
            <div className="source-actions">
              {connected && (
                <button
                  type="button"
                  className="button secondary"
                  disabled={pending || loadingAccounts}
                  data-testid={"account-favorites-" + source}
                  onClick={() => onOpenFavorites(source)}
                >
                  查看收藏
                </button>
              )}
              <button
                type="button"
                className={connected ? "button secondary" : "button primary"}
                disabled={pending || loadingAccounts || unavailable}
                data-testid={"account-connect-" + source}
                onClick={() => open(source)}
              >
                {connected ? "重新登录" : "连接" + sourceLabel(source) + "账号"}
              </button>
              {account &&
                (connected ||
                  account.remembered ||
                  account.rememberLogin ||
                  account.state === "expired") && (
                  <button
                    type="button"
                    className="button secondary"
                    disabled={pending || loadingAccounts}
                    data-testid={"account-logout-" + source}
                    onClick={() => void logout(account)}
                  >
                    {connected
                      ? "退出登录"
                      : account.rememberLogin
                        ? "忘记保存的登录"
                        : "忘记保存的会话"}
                  </button>
                )}
            </div>
          </div>
        );
      })}
      <p className="settings-help">
        “记住会话”仅将登录会话保存在系统安全存储，密码不写入普通配置或日志。
      </p>
      <p className="settings-help">
        JM 可另选“记住登录”，密码仅保存在 Windows
        系统凭据库。退出登录或忘记保存的登录会清除会话与登录信息。
      </p>
      <p className="settings-help">
        收藏与作者更新由你手动检查。账号操作独立完成，不需要点击其他设置页的保存按钮。
      </p>
      {error && !loginSource && (
        <p className="source-warning" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" data-testid="account-operation-status">
          {notice}
        </p>
      )}
      <button
        type="button"
        className="text-button"
        disabled={pending || loadingAccounts}
        data-testid="accounts-reload"
        onClick={() => void refresh()}
      >
        重新读取账号状态
      </button>
      {loginSource && (
        <dialog
          ref={dialog}
          className="dialog source-login-dialog"
          data-testid="account-login-dialog"
          aria-label={"连接" + sourceLabel(loginSource) + "账号"}
          aria-busy={pending}
          onCancel={(event) => {
            event.preventDefault();
            close();
          }}
        >
          <div className="dialog-heading">
            <h2>连接{sourceLabel(loginSource)}账号</h2>
            <button
              type="button"
              className="text-button"
              disabled={pending}
              aria-label="关闭登录窗口"
              onClick={close}
            >
              关闭
            </button>
          </div>
          <form
            autoComplete="off"
            onSubmit={(event) => {
              event.preventDefault();
              void login();
            }}
          >
            <label className="source-field">
              账号
              <input
                autoFocus
                value={username}
                disabled={pending}
                data-testid="account-username"
                autoComplete="username"
                onChange={(event) => setUsername(event.target.value)}
              />
            </label>
            <label className="source-field">
              密码
              <input
                type="password"
                value={password}
                disabled={pending}
                data-testid="account-password"
                autoComplete="current-password"
                onChange={(event) => setPassword(event.target.value)}
              />
            </label>
            <label className="source-check">
              <input
                type="checkbox"
                checked={remember}
                disabled={pending}
                data-testid="account-remember"
                onChange={(event) => {
                  setRemember(event.target.checked);
                  if (!event.target.checked) setRememberLogin(false);
                }}
              />
              记住会话（仅选此项不保存密码）
            </label>
            {loginSource === "JM" && (
              <>
                <label className="source-check">
                  <input
                    type="checkbox"
                    checked={rememberLogin}
                    disabled={pending}
                    data-testid="account-remember-login"
                    onChange={(event) => {
                      setRememberLogin(event.target.checked);
                      if (event.target.checked) setRemember(true);
                    }}
                  />
                  记住登录（会话过期后自动重新登录）
                </label>
                <p className="settings-help">
                  启用后会同时记住会话，密码仅保存在 Windows
                  系统凭据库，不写入普通配置或日志。打开应用或重新读取账号状态时，仅明确过期才自动重新登录一次，网络错误不会反复登录，也不会重做收藏、下载或扫描操作。
                </p>
              </>
            )}
            {error && (
              <p role="alert" className="source-warning">
                {error}
              </p>
            )}
            {pending && <p role="status">正在连接…</p>}
            <div className="dialog-actions">
              <button
                type="button"
                className="button secondary"
                disabled={pending}
                onClick={close}
              >
                取消
              </button>
              <button
                type="submit"
                className="button primary"
                data-testid="account-login-submit"
                disabled={pending || !username.trim() || !password}
              >
                {pending ? "正在连接…" : "连接账号"}
              </button>
            </div>
          </form>
        </dialog>
      )}
    </section>
  );
}
