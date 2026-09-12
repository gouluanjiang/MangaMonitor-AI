import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AccountSummary,
  SourceAdapter,
  SourceWork,
} from "./source-types.ts";
import { accountScope, sourceLabel, sourceWorkKey } from "./source-types.ts";
import { parseLibraryReference } from "./library-model.ts";
import { sourceErrorMessage } from "./source-runtime.ts";
import { findSourceMatch } from "./source-matches-model.ts";
import {
  lookupSourceMatchWork,
  sourceMatchesErrorMessage,
} from "./source-matches-runtime.ts";
import type {
  SourceMatchWork,
  SourceMatchesAdapter,
  SourceMatchesSnapshot,
} from "./source-matches-types.ts";
import { emptySourceMatches } from "./source-matches-types.ts";
import "./source-matches.css";

export function useSourceMatches(
  adapter: SourceMatchesAdapter,
  enabled: boolean,
) {
  const [snapshot, setSnapshot] = useState(emptySourceMatches);
  const [ready, setReady] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const current = useRef(snapshot),
    lock = useRef(false),
    epoch = useRef(0);
  const execute = useCallback(
    async (operation: () => Promise<SourceMatchesSnapshot>) => {
      if (!enabled || lock.current) return false;
      lock.current = true;
      setBusy(true);
      setError("");
      const token = epoch.current;
      try {
        const next = await operation();
        if (token !== epoch.current) return false;
        current.current = next;
        setSnapshot(next);
        setReady(true);
        return true;
      } catch (cause) {
        if (token === epoch.current) {
          setError(sourceMatchesErrorMessage(cause));
          setReady(false);
        }
        return false;
      } finally {
        if (token === epoch.current) {
          lock.current = false;
          setBusy(false);
        }
      }
    },
    [enabled],
  );
  const reload = useCallback(
    () => execute(() => adapter.read()),
    [adapter, execute],
  );
  useEffect(() => {
    if (!enabled) return;
    void reload();
    return () => {
      epoch.current++;
      lock.current = false;
    };
  }, [enabled, reload]);
  return {
    snapshot,
    ready,
    busy,
    error,
    reload,
    confirm: (jm: SourceMatchWork, pica: SourceMatchWork) =>
      execute(() => adapter.confirm(current.current.revision, jm, pica)),
    unlink: (pairId: string) =>
      execute(() => adapter.unlink(current.current.revision, pairId)),
  };
}
export type SourceMatchesState = ReturnType<typeof useSourceMatches>;

export function SourceMatchPanel({
  work,
  matches,
  adapter,
  accounts,
}: {
  work: SourceWork;
  matches: SourceMatchesState;
  adapter: SourceAdapter;
  accounts: AccountSummary[];
}) {
  const opposite = work.source === "JM" ? "Pica" : "JM";
  const scope = accountScope(
    accounts.find((account) => account.source === opposite),
  );
  const scopeKey = scope ? scope.source + ":" + scope.sessionId : "";
  const identity = sourceWorkKey(work) + ":" + scopeKey;
  const [input, setInput] = useState(""),
    [lookup, setLookup] = useState<SourceMatchWork | null>(null),
    [reading, setReading] = useState(false),
    [checked, setChecked] = useState(false),
    [removing, setRemoving] = useState(false),
    [notice, setNotice] = useState("");
  const version = useRef(0);
  useEffect(() => {
    version.current++;
    setInput("");
    setLookup(null);
    setReading(false);
    setChecked(false);
    setRemoving(false);
    setNotice("");
    return () => {
      version.current++;
    };
  }, [identity]);
  const pair = findSourceMatch(matches.snapshot.pairs, work);
  const readOpposite = async () => {
    const reference = parseLibraryReference(opposite, input);
    if (!reference || reading) return;
    const token = ++version.current;
    setReading(true);
    setLookup(null);
    setChecked(false);
    setNotice("");
    try {
      const result = await lookupSourceMatchWork(adapter, accounts, reference);
      if (token === version.current) setLookup(result);
    } catch (cause) {
      if (token === version.current) setNotice(sourceErrorMessage(cause));
    } finally {
      if (token === version.current) setReading(false);
    }
  };
  const confirm = async () => {
    if (!lookup || !checked || !matches.ready) return;
    const here = {
      source: work.source,
      workId: work.workId,
      title: work.title,
    };
    const token = version.current;
    const saved = await matches.confirm(
      work.source === "JM" ? here : lookup,
      work.source === "Pica" ? here : lookup,
    );
    if (saved && token === version.current) {
      setLookup(null);
      setChecked(false);
      setNotice("已确认这两个来源指向同一作品。");
    }
  };
  return (
    <section className="source-match-panel" data-testid="source-match-panel">
      <h3>跨来源关联</h3>
      <p className="source-muted">
        确认两个来源为同一作品后，共用已有的已下载、已入库状态。
      </p>
      {!matches.ready || matches.error ? (
        <div className="source-notice" role="status">
          <p>{matches.error || "正在读取关联记录…"}</p>
          <button
            className="button secondary"
            disabled={matches.busy}
            onClick={() => void matches.reload()}
          >
            重新读取关联
          </button>
        </div>
      ) : pair ? (
        <div data-testid="source-match-confirmed">
          <p>
            <strong>JM · {pair.jm.workId}</strong>
            <br />
            {pair.jm.title}
          </p>
          <p>
            <strong>哔咔 · {pair.pica.workId}</strong>
            <br />
            {pair.pica.title}
          </p>
          <p className="source-muted">已手动确认同一作品</p>
          {removing ? (
            <div className="source-actions">
              <span>解除后按各来源原有记录显示状态，文件与手机名单保留。</span>
              <button
                className="button secondary"
                data-testid="source-match-unlink-confirm"
                disabled={matches.busy}
                onClick={() => {
                  void matches.unlink(pair.id).then((saved) => {
                    if (saved) setRemoving(false);
                  });
                }}
              >
                确认解除关联
              </button>
              <button
                className="button secondary"
                disabled={matches.busy}
                onClick={() => setRemoving(false)}
              >
                取消
              </button>
            </div>
          ) : (
            <button
              className="button secondary"
              data-testid="source-match-unlink"
              disabled={matches.busy}
              onClick={() => setRemoving(true)}
            >
              解除关联
            </button>
          )}
        </div>
      ) : (
        <div>
          <label className="source-match-input">
            {sourceLabel(opposite)} 作品编号
            <input
              data-testid="source-match-id"
              value={input}
              maxLength={40}
              placeholder={opposite === "JM" ? "JM 编号" : "24 位作品 ID"}
              disabled={matches.busy}
              onChange={(event) => {
                version.current++;
                setInput(event.target.value);
                setLookup(null);
                setReading(false);
                setChecked(false);
                setNotice("");
              }}
            />
          </label>
          <button
            className="button secondary"
            data-testid="source-match-lookup"
            disabled={
              reading ||
              matches.busy ||
              !scope ||
              !parseLibraryReference(opposite, input)
            }
            onClick={() => void readOpposite()}
          >
            {reading ? "正在读取作品…" : "读取并核对作品"}
          </button>
          {!scope && (
            <p className="source-notice">
              请先登录{sourceLabel(opposite)}账号，以便读取另一部作品。
            </p>
          )}
          {lookup && (
            <div
              className="source-match-preview"
              data-testid="source-match-preview"
            >
              <p>
                <strong>
                  {sourceLabel(work.source)} · {work.workId}
                </strong>
                <br />
                {work.title}
              </p>
              <p>
                <strong>
                  {sourceLabel(lookup.source)} · {lookup.workId}
                </strong>
                <br />
                {lookup.title}
              </p>
              <label className="source-match-checkbox">
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={matches.busy}
                  onChange={(event) => setChecked(event.target.checked)}
                />
                我已核对内容与版本，确认是同一作品
              </label>
              <button
                className="button primary"
                data-testid="source-match-confirm"
                disabled={!checked || matches.busy}
                onClick={() => void confirm()}
              >
                确认关联
              </button>
            </div>
          )}
        </div>
      )}
      {notice && (
        <p className="source-notice" role="status">
          {notice}
        </p>
      )}
    </section>
  );
}
