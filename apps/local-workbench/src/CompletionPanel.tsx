import { useEffect, useMemo, useRef, useState } from "react";
import type {
  AccountSummary,
  SourceAdapter,
  SourceScope,
  SourceWork,
} from "./source-types.ts";
import { accountScope, sourceWorkKey } from "./source-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import type { PhoneLibrarySnapshot } from "./phone-library-types.ts";
import type { WorkReference } from "./booklists.ts";
import type {
  CompletionAdapter,
  CompletionGroup,
  CompletionLanguage,
  CompletionMember,
  CompletionSettings,
  CompletionStatus,
  CompletionView,
} from "./completion-types.ts";
import {
  completionError,
  createCompletionAdapter,
} from "./completion-runtime.ts";
import { SourceCover } from "./SourceWorkbench.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import "./completion.css";

const nativeAdapter = createCompletionAdapter();
export const completionLabels: Record<CompletionStatus, string> = {
  missing: "待补入",
  downloaded: "已下载 · 待传手机",
  owned_chinese: "汉化已入库",
  waiting_translation: "日文已入库 · 等待汉化",
  translation_available: "发现汉化 · 待下载",
  translation_downloaded: "汉化已下载 · 待替换",
  review_required: "需要核对",
  unknown: "状态待确认",
};
const languages: Record<CompletionLanguage, string> = {
  chinese: "汉化 / 中文",
  japanese: "日文",
  other: "其他语言",
  unknown: "语言未确认",
};
const memberKey = (member: CompletionMember) => JSON.stringify(member);
interface Props {
  accounts: AccountSummary[];
  sourceAdapter: SourceAdapter;
  adapter?: CompletionAdapter;
  library: LibrarySnapshot;
  phone: PhoneLibrarySnapshot;
  density: 5 | 7 | 9;
  onOpenWork(reference: WorkReference): void;
  onDownload(work: SourceWork): void;
  onOpenLibrary(): void;
  onOpenAccounts(): void;
}
export function CompletionPanel({
  accounts,
  sourceAdapter,
  adapter = nativeAdapter,
  library,
  phone,
  density,
  onOpenWork,
  onDownload,
  onOpenLibrary,
  onOpenAccounts,
}: Props) {
  const scopes = accounts
    .map(accountScope)
    .filter((s): s is SourceScope => s !== null);
  const scopeKey = JSON.stringify(scopes);
  const currentKey = useRef(scopeKey);
  currentKey.current = scopeKey;
  const [viewState, setViewState] = useState<{
    key: string;
    value: CompletionView;
  } | null>(null);
  const view = viewState?.key === scopeKey ? viewState.value : null;
  const [settings, setSettings] = useState<CompletionSettings | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [query, setQuery] = useState(""),
    [filter, setFilter] = useState<CompletionStatus | "pending" | "all">(
      "pending",
    );
  const [automatic, setAutomatic] = useState(true),
    [author, setAuthor] = useState("");
  const [reviewId, setReviewId] = useState<string | null>(null),
    [selected, setSelected] = useState<CompletionMember[]>([]);
  const [phoneQuery, setPhoneQuery] = useState("");
  const alive = useRef(true),
    operation = useRef(false),
    pollEpoch = useRef(0);
  const connected = scopes.length === 2;
  const scanRunning = view?.discovery.run?.phase === "checking";
  const autoRunning =
    view?.automatic.phase === "waiting" ||
    view?.automatic.phase === "enqueueing";
  const running = !!(scanRunning || autoRunning);
  const apply = (value: CompletionView, key = scopeKey) => {
    if (alive.current && currentKey.current === key)
      setViewState({ key, value });
  };
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    let stopped = false,
      timer: ReturnType<typeof setTimeout> | undefined;
    setViewState(null);
    setSettings(null);
    setReviewId(null);
    setSelected([]);
    setError("");
    if (!connected) return;
    const read = async () => {
      const epoch = pollEpoch.current;
      try {
        const next = await adapter.read(scopes);
        if (stopped) return;
        if (epoch === pollEpoch.current && !operation.current)
          apply(next, scopeKey);
      } catch (cause) {
        if (!stopped && epoch === pollEpoch.current && !operation.current)
          setError(completionError(cause));
      }
      if (!stopped) timer = setTimeout(read, 4000);
    };
    void read();
    void adapter
      .settings()
      .then((next) => {
        if (!stopped) setSettings(next);
      })
      .catch(() => {});
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
    // A changed verified session invalidates all visible data and pending replies.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adapter, scopeKey, connected]);
  async function perform(operationFn: () => Promise<void>) {
    if (operation.current) return;
    operation.current = true;
    pollEpoch.current++;
    setBusy(true);
    setError("");
    const key = scopeKey;
    try {
      await operationFn();
    } catch (cause) {
      if (alive.current && currentKey.current === key)
        setError(completionError(cause));
    } finally {
      operation.current = false;
      if (alive.current) setBusy(false);
    }
  }
  async function refresh(recheckFiles = false) {
    const key = scopeKey;
    const next = await adapter.read(scopes, recheckFiles);
    apply(next, key);
    const nextSettings = await adapter.settings();
    if (alive.current && currentKey.current === key) setSettings(nextSettings);
  }
  const groups = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    return (view?.completeness.groups ?? []).filter(
      (group) =>
        (filter === "all" ||
          (filter === "pending"
            ? group.status !== "owned_chinese"
            : group.status === filter)) &&
        (!term ||
          [
            group.title,
            ...group.authors,
            ...group.sources.map((s) => s.reference.workId),
          ].some((s) => s.toLocaleLowerCase().includes(term))),
    );
  }, [view, filter, query]);
  const works = useMemo(
    () =>
      new Map(
        view?.discovery.records.map((r) => [sourceWorkKey(r.work), r.work]) ??
          [],
      ),
    [view],
  );
  const authors = [
    ...new Set(view?.discovery.authors.map((a) => a.author) ?? []),
  ];
  const review = view?.completeness.groups.find((g) => g.groupId === reviewId);
  const phoneNames = useMemo(
    () => [
      ...new Set([
        ...phone.importedNames,
        ...phone.manualEntries.map((e) => e.name),
      ]),
    ],
    [phone],
  );
  const members = review
    ? [
        ...review.sources.map((s) => ({
          member: {
            kind: "source",
            reference: s.reference,
          } as CompletionMember,
          name:
            s.reference.source + " · " + s.reference.workId + " · " + s.title,
          language: s.language,
        })),
        ...review.phone,
        ...review.computer,
      ]
    : [];
  function choose(member: CompletionMember, checked: boolean) {
    setSelected((previous) =>
      checked
        ? [
            ...previous.filter((m) => memberKey(m) !== memberKey(member)),
            member,
          ]
        : previous.filter((m) => memberKey(m) !== memberKey(member)),
    );
  }
  function workFor(group: CompletionGroup) {
    const ref =
      group.sources.find((s) => s.language === "chinese")?.reference ??
      group.sources[0]?.reference;
    return ref ? works.get(ref.source + ":" + ref.workId) : undefined;
  }
  async function correction(action: () => Promise<CompletionSettings>) {
    const key = scopeKey;
    const next = await action();
    if (currentKey.current !== key) return;
    setSettings(next);
    setSelected([]);
    await refresh();
  }
  return (
    <section className="completion-panel" data-testid="completion-panel">
      <header>
        <h1>作者作品补全</h1>
        <p>
          检查关注作者在 JM 和哔咔的作品，找出手机漫画库中的遗漏与待替换汉化。
        </p>
      </header>
      {!connected ? (
        <p>
          请连接 JM 和哔咔账号后检查。
          <button onClick={onOpenAccounts}>连接账号</button>
        </p>
      ) : (
        <>
          <div className="completion-controls">
            <label>
              检查范围
              <select
                aria-label="检查作者"
                value={author}
                onChange={(e) => setAuthor(e.target.value)}
                disabled={busy || running}
              >
                <option value="">全部关注作者</option>
                {authors.map((a) => (
                  <option key={a}>{a}</option>
                ))}
              </select>
            </label>
            <label>
              <input
                type="checkbox"
                checked={automatic}
                disabled={busy || running}
                onChange={(e) => setAutomatic(e.target.checked)}
              />
              发现对应汉化后自动下载到电脑
            </label>
            <button
              className="primary-button"
              disabled={busy || running}
              data-testid="completion-start"
              onClick={() =>
                void perform(async () => {
                  const key = scopeKey;
                  apply(
                    await adapter.start(
                      scopes,
                      author ? [author] : [],
                      automatic,
                      library.rootId,
                      library.generation,
                    ),
                    key,
                  );
                })
              }
            >
              检查作品补全
            </button>
            {running && (
              <button
                disabled={busy}
                onClick={() =>
                  void perform(async () => {
                    if (view?.discovery.run)
                      await adapter.cancel(view.discovery.run.id);
                    await refresh();
                  })
                }
              >
                停止本次检查与自动下载
              </button>
            )}
            <button
              disabled={busy || running}
              onClick={() => void perform(() => refresh(true))}
            >
              刷新入库状态
            </button>
          </div>
          <p className="source-muted">
            汉化将保存到：{library.rootPath ?? "尚未选择电脑漫画目录"}{" "}
            <button className="text-button" onClick={onOpenLibrary}>
              查看电脑目录
            </button>
          </p>
          <p className="source-muted">
            下载完成后，由你传入手机并手动标记或导入最新
            TXT；电脑副本继续保留。普通遗漏作品由你选择下载。
          </p>
          {error && <p role="alert">{error}</p>}
          {view?.discovery.run && (
            <div role="status" className="completion-progress">
              {scanRunning
                ? `正在检查 ${view.discovery.run.currentAuthor ?? "关注名单"} · ${view.discovery.run.currentSource ?? ""} · 第 ${view.discovery.run.currentPage} 页`
                : `本次检查${view.discovery.run.phase === "complete" ? "完成" : view.discovery.run.phase === "cancelled" ? "已停止" : "有未读完的范围"}`}{" "}
              · 已完成 {view.discovery.run.completedScopes} /{" "}
              {view.discovery.run.totalScopes} 个作者来源范围
            </div>
          )}
          {view && view.automatic.phase !== "idle" && (
            <p role="status">
              汉化自动下载：
              {
                (
                  {
                    waiting: "等待本次目录核对完成",
                    enqueueing: "正在加入下载队列",
                    complete: "本次入队完成",
                    cancelled: "已停止",
                    error: "未能完成",
                    idle: "未开启",
                  } as const
                )[view.automatic.phase]
              }{" "}
              · 已加入 {view.automatic.queued} 本
              {view.automatic.skipped > 0
                ? ` · ${view.automatic.skipped} 本请在队列或补全名单核对`
                : ""}{" "}
              {view.automatic.errorCode && (
                <span>（{view.automatic.errorCode}）</span>
              )}
            </p>
          )}
          {view && (
            <details>
              <summary>
                检查范围与未完成项目（{view.discovery.authors.length}）
              </summary>
              <div className="completion-ranges">
                {view.discovery.authors.map((a) => (
                  <p key={a.author + ":" + a.source}>
                    {a.author} · {a.source} ·{" "}
                    {a.state === "complete"
                      ? "已读完"
                      : a.state === "checking"
                        ? "读取中"
                        : "尚未读完"}{" "}
                    · {a.observedCount} 本 / {a.pagesRead} 页{" "}
                    {a.errorCode && `（${a.errorCode}）`}
                  </p>
                ))}
              </div>
            </details>
          )}
          <div className="completion-controls">
            <input
              aria-label="筛选补全作品"
              placeholder="筛选作品、作者或编号"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select
              aria-label="补全状态"
              value={filter}
              onChange={(e) => setFilter(e.target.value as typeof filter)}
            >
              <option value="pending">待补全与待核对</option>
              <option value="all">全部作品</option>
              {Object.entries(completionLabels).map(([k, label]) => (
                <option key={k} value={k}>
                  {label}
                </option>
              ))}
            </select>
            <span>{groups.length} 部作品</span>
          </div>
          {!view?.discovery.records.length && (
            <p>点击“检查作品补全”开始读取。首次检查也会列出旧作遗漏。</p>
          )}
          <VirtualSourceGrid
            items={groups}
            density={density}
            itemKey={(g) => g.groupId}
            renderItem={(group) => {
              const work = workFor(group),
                scope = work
                  ? scopes.find((s) => s.source === work.source)
                  : undefined;
              return (
                <article
                  className="source-card completion-card"
                  data-testid={"completion-group-" + group.groupId}
                >
                  {work && scope && (
                    <button
                      className="source-cover-button"
                      aria-label={"查看《" + group.title + "》详情"}
                      onClick={() =>
                        onOpenWork({ source: work.source, workId: work.workId })
                      }
                    >
                      <SourceCover
                        adapter={sourceAdapter}
                        scope={scope}
                        work={work}
                      />
                    </button>
                  )}
                  <h3 title={group.title}>{group.title}</h3>
                  <p>{group.authors.join("、")}</p>
                  <p className="completion-status">
                    {completionLabels[group.status]}
                  </p>
                  <p>
                    {[
                      ...new Set(group.sources.map((s) => s.reference.source)),
                    ].join(" · ")}
                  </p>
                  <div className="completion-card-actions">
                    {group.status !== "owned_chinese" && work && (
                      <button onClick={() => onDownload(work)}>
                        {group.status === "downloaded" ||
                        group.status === "translation_downloaded"
                          ? "查看 / 重新下载"
                          : "准备下载"}
                      </button>
                    )}
                    <button
                      onClick={() => {
                        setReviewId(group.groupId);
                        setSelected([]);
                        setPhoneQuery("");
                      }}
                    >
                      核对版本
                    </button>
                  </div>
                </article>
              );
            }}
          />
          {review && (
            <section
              className="completion-review"
              role="dialog"
              aria-label="核对作品版本"
            >
              <div className="completion-controls">
                <h2>核对版本 · {review.title}</h2>
                <button onClick={() => setReviewId(null)}>关闭核对</button>
              </div>
              <p>
                勾选同一作品的日文、汉化或另一来源版本后确认关联。不同卷、合集与单话请分别核对。
              </p>
              {members.map((entry, index) => (
                <div
                  className="completion-member"
                  key={memberKey(entry.member) + index}
                >
                  <label>
                    <input
                      type="checkbox"
                      checked={selected.some(
                        (m) => memberKey(m) === memberKey(entry.member),
                      )}
                      onChange={(e) => choose(entry.member, e.target.checked)}
                    />
                    {entry.name}
                  </label>
                  <select
                    aria-label={"语言：" + entry.name}
                    value={entry.language}
                    disabled={busy || running || !settings}
                    onChange={(e) =>
                      void perform(() =>
                        correction(() =>
                          adapter.language(
                            settings!.revision,
                            entry.member,
                            e.target.value as CompletionLanguage,
                          ),
                        ),
                      )
                    }
                  >
                    {Object.entries(languages).map(([k, label]) => (
                      <option key={k} value={k}>
                        {label}
                      </option>
                    ))}
                  </select>
                  <button
                    disabled={busy || running || !settings}
                    onClick={() =>
                      void perform(() =>
                        correction(() =>
                          adapter.language(
                            settings!.revision,
                            entry.member,
                            null,
                          ),
                        ),
                      )
                    }
                  >
                    恢复来源判断
                  </button>
                </div>
              ))}
              <label>
                从手机 TXT 名单找对应作品
                <input
                  aria-label="搜索手机名单"
                  value={phoneQuery}
                  onChange={(e) => setPhoneQuery(e.target.value)}
                  placeholder="输入作品名"
                />
              </label>
              {phoneQuery.trim() && (
                <div className="completion-phone-results">
                  {phoneNames
                    .filter((n) =>
                      n
                        .toLocaleLowerCase()
                        .includes(phoneQuery.trim().toLocaleLowerCase()),
                    )
                    .slice(0, 30)
                    .map((name) => {
                      const m: CompletionMember = { kind: "phone", name };
                      return (
                        <label key={name}>
                          <input
                            type="checkbox"
                            checked={selected.some(
                              (s) => memberKey(s) === memberKey(m),
                            )}
                            onChange={(e) => choose(m, e.target.checked)}
                          />
                          {name}
                        </label>
                      );
                    })}
                </div>
              )}
              <details>
                <summary>关联另一部已检查的来源作品</summary>
                <input
                  aria-label="输入另一来源作品编号"
                  placeholder="作品编号"
                  onChange={(e) => setPhoneQuery(e.target.value)}
                />
                {phoneQuery.trim() &&
                  [...works.values()]
                    .filter(
                      (w) => w.workId === phoneQuery.trim().replace(/^JM/i, ""),
                    )
                    .slice(0, 10)
                    .map((w) => {
                      const m: CompletionMember = {
                        kind: "source",
                        reference: { source: w.source, workId: w.workId },
                      };
                      return (
                        <label key={sourceWorkKey(w)}>
                          <input
                            type="checkbox"
                            checked={selected.some(
                              (s) => memberKey(s) === memberKey(m),
                            )}
                            onChange={(e) => choose(m, e.target.checked)}
                          />
                          {w.source} · {w.title}
                        </label>
                      );
                    })}
              </details>
              <button
                disabled={
                  busy ||
                  running ||
                  !settings ||
                  selected.length < 2 ||
                  selected.length > 50
                }
                onClick={() =>
                  void perform(() =>
                    correction(() =>
                      adapter.family(settings!.revision, selected),
                    ),
                  )
                }
              >
                确认所选版本属于同一作品
              </button>
              {settings?.families
                .filter((f) =>
                  f.members.some((m) =>
                    members.some((e) => memberKey(e.member) === memberKey(m)),
                  ),
                )
                .map((f) => (
                  <p key={f.id}>
                    已确认 {f.members.length} 个版本 / 名单条目{" "}
                    <button
                      disabled={busy || running}
                      onClick={() =>
                        void perform(() =>
                          correction(() =>
                            adapter.unlink(settings.revision, f.id),
                          ),
                        )
                      }
                    >
                      解除这组关联
                    </button>
                  </p>
                ))}
              <p className="source-muted">
                确认关联与语言后，再检查一次即可应用汉化自动下载规则。
              </p>
            </section>
          )}
        </>
      )}
    </section>
  );
}
