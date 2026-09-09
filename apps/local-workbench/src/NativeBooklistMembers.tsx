import { useEffect, useRef, useState } from "react";
import type { WorkReference } from "./booklists.ts";
import { SourceWorkGrid } from "./SourceWorkbench.tsx";
import { sourceErrorMessage } from "./source-runtime.ts";
import { accountScope, sourceWorkKey } from "./source-types.ts";
import type {
  AccountSummary,
  SourceAdapter,
  SourceScope,
  SourceWork,
} from "./source-types.ts";

export function NativeBooklistMembers({
  members,
  accounts,
  adapter,
  cache,
  onWorksChanged,
  onAccountsChange,
  density,
  onOpenWork,
  onAddToBooklists,
  onRemove,
  removeDisabled,
  query,
  sourceFilter,
}: {
  members: WorkReference[];
  accounts: AccountSummary[];
  adapter: SourceAdapter;
  cache: Record<string, { scope: SourceScope; work: SourceWork }>;
  onWorksChanged(scope: SourceScope, works: SourceWork[]): void;
  onAccountsChange(accounts: AccountSummary[]): void;
  density: 5 | 7 | 9;
  onOpenWork(ref: WorkReference): void;
  onAddToBooklists(refs: WorkReference[]): Promise<boolean>;
  onRemove(ref: WorkReference): Promise<boolean>;
  removeDisabled: boolean;
  query: string;
  sourceFilter: string;
}) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [limit, setLimit] = useState(20);
  const lock = useRef(false);
  const generation = useRef(0);
  const current = useRef(accounts);
  current.current = accounts;
  const identity = accounts
    .map(
      (account) =>
        account.source + ":" + account.sessionId + ":" + account.state,
    )
    .join("|");
  useEffect(() => {
    generation.current++;
    setError("");
    return () => {
      generation.current++;
    };
  }, [identity]);
  const scopes = accounts
    .map(accountScope)
    .filter((scope): scope is SourceScope => scope !== null);
  const loaded = members.flatMap((member) => {
    const entry = cache[sourceWorkKey(member)];
    return entry &&
      scopes.some(
        (scope) =>
          scope.source === entry.scope.source &&
          scope.sessionId === entry.scope.sessionId,
      )
      ? [entry.work]
      : [];
  });
  const missing = members.filter(
    (member) =>
      !loaded.some((work) => sourceWorkKey(work) === sourceWorkKey(member)),
  );
  const loadable = missing.filter((member) =>
    scopes.some((scope) => scope.source === member.source),
  );
  const visible = loaded.filter(
    (work) =>
      (sourceFilter === "all" || work.source === sourceFilter) &&
      (work.title + " " + work.authors.join(" ") + " " + work.workId)
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  async function read() {
    if (lock.current) return;
    lock.current = true;
    setLoading(true);
    setError("");
    const epoch = generation.current;
    let failed = 0;
    try {
      // One deliberate click reads at most twenty details, sequentially. No full scan.
      for (const member of loadable.slice(0, 20)) {
        if (generation.current !== epoch) break;
        const scope = accountScope(
          current.current.find((account) => account.source === member.source),
        );
        if (!scope) continue;
        try {
          const result = await adapter.query(scope, {
            kind: "detail",
            query: member.workId,
            folderId: null,
            page: 1,
          });
          if (generation.current !== epoch) break;
          onWorksChanged(scope, result.items);
        } catch (cause) {
          failed++;
          if (generation.current === epoch) setError(sourceErrorMessage(cause));
        }
      }
      if (failed && generation.current === epoch) {
        try {
          const latest = await adapter.accounts();
          if (generation.current === epoch) onAccountsChange(latest);
        } catch {
          /* The actionable request error is retained. */
        }
      }
    } finally {
      lock.current = false;
      setLoading(false);
    }
  }
  return (
    <section
      className="native-booklist-members"
      data-testid="native-booklist-members"
      aria-busy={loading}
    >
      <h3>来源作品（{members.length}）</h3>
      <p className="source-muted">
        已读取 {loaded.length}{" "}
        部。书单关联保存在本机，作品资料按当前连接账号读取。
      </p>
      {error && (
        <p role="alert" className="source-notice">
          {error} 书单关联已保留。
        </p>
      )}
      {loadable.length > 0 && (
        <button
          className="button secondary"
          disabled={loading}
          onClick={() => void read()}
        >
          {loading ? "正在读取资料…" : "读取作品资料（最多 20 部）"}
        </button>
      )}
      {visible.length > 0 && (
        <SourceWorkGrid
          works={visible}
          scopes={scopes}
          adapter={adapter}
          density={density}
          onOpenWork={onOpenWork}
          onAddToBooklists={onAddToBooklists}
        />
      )}
      {missing.length > 0 && (
        <div className="unavailable-members" data-testid="unavailable-members">
          <h3>作品资料尚未读取（{missing.length}）</h3>
          <p>连接对应来源账号后可查看详情。读取失败不会移除书单关联。</p>
          {missing.slice(0, limit).map((member) => (
            <div key={sourceWorkKey(member)}>
              <span>
                {member.source} · {member.workId}
              </span>
              <button
                className="text-button"
                onClick={() => onOpenWork(member)}
              >
                查看详情
              </button>
              <button
                className="text-button"
                disabled={removeDisabled}
                onClick={() => void onRemove(member)}
              >
                移出书单
              </button>
            </div>
          ))}
          {missing.length > limit && (
            <button
              className="text-button"
              onClick={() => setLimit((previous) => previous + 20)}
            >
              显示更多关联
            </button>
          )}
        </div>
      )}
    </section>
  );
}
