import {
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { SpecialFollowsContext } from "./useSpecialFollows.tsx";
import { specialRunMessage } from "./special-runtime.ts";
import type { CompletionPanelProps } from "./CompletionPanel.tsx";
import { VirtualSourceGrid } from "./VirtualSourceGrid.tsx";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import { CoverInteraction, sourceReaderRequest } from "./reader-access.tsx";
import { SourceCover } from "./SourceWorkbench.tsx";
import { SourceLanguageBadge } from "./SourceLanguageBadge.tsx";
import { AuthorLinks } from "./AuthorLinks.tsx";
import { DownloadWorkButton } from "./DownloadWorkButton.tsx";
import { sourceLabel, sourceWorkKey } from "./source-types.ts";
import {
  isContentHidden,
  subscribeContentFilter,
  getContentFilterRevision,
} from "./content-filter.ts";
import { createInventoryMatcher, inventoryLabel } from "./inventory-model.ts";
import { useBrowseSession } from "./useBrowseSession.ts";

type Props = Pick<
  CompletionPanelProps,
  | "active"
  | "sourceAdapter"
  | "library"
  | "inventorySnapshot"
  | "inventoryReady"
  | "density"
  | "onOpenWork"
  | "onDownload"
  | "onOpenAccounts"
>;
export function SpecialFollowsPanel(props: Props) {
  const state = useContext(SpecialFollowsContext)!;
  const [onlyUnread, setOnlyUnread] = useState(true);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLElement>(null),
    grid = useRef<SourceGridHandle>(null);
  const snapshot = state.snapshot;
  const contentRevision = useSyncExternalStore(
    subscribeContentFilter,
    getContentFilterRevision,
  );
  const inventory = useMemo(
    () =>
      createInventoryMatcher(
        props.library,
        props.inventorySnapshot,
        props.inventoryReady,
      ),
    [props.library, props.inventorySnapshot, props.inventoryReady],
  );
  const rows = useMemo(
    () =>
      (snapshot?.updates ?? [])
        .filter(
          (row) =>
            !isContentHidden(row.work) &&
            (!onlyUnread || row.readAt === null) &&
            (!query.trim() ||
              [row.work.title, ...row.authors].some((text) =>
                text
                  .toLocaleLowerCase()
                  .includes(query.trim().toLocaleLowerCase()),
              )),
        )
        .sort(
          (a, b) =>
            b.discoveredAt - a.discoveredAt ||
            sourceWorkKey(a.work).localeCompare(sourceWorkKey(b.work)),
        ),
    [snapshot, onlyUnread, query, contentRevision],
  );
  const accountKey = JSON.stringify(snapshot?.scopes ?? []);
  useBrowseSession({
    scope: `special:${accountKey}:${onlyUnread}:${query}`,
    active: !!props.active,
    root,
    grid,
    itemKeys: rows.map((row) => sourceWorkKey(row.work)),
  });
  useEffect(() => {
    if (props.active && state.connected) void state.refresh();
  }, [props.active, state.connected]);
  if (!props.active) return null;
  const running = ["waiting", "checking"].includes(state.run?.phase ?? "");
  return (
    <section
      ref={root}
      className="completion-panel source-workbench"
      data-testid="special-panel"
    >
      <h1>特别关注</h1>
      <p>
        首次建立作品基线；以后每次启动在后台检查。进入本页不会自动标为已读。
      </p>
      {!state.connected ? (
        <div className="source-empty">
          <p>请连接 JM 和哔咔后查看当前账号的特别关注。</p>
          <button onClick={props.onOpenAccounts}>连接账号</button>
        </div>
      ) : (
        <>
          <div className="source-actions">
            <button
              disabled={running || state.busy}
              onClick={() => void state.start()}
            >
              检查特别关注
            </button>
            {running && (
              <button onClick={() => void state.cancel()}>停止本次检查</button>
            )}
            <button onClick={() => void state.refresh()}>刷新状态</button>
            <button
              disabled={!state.unread}
              onClick={() => void state.markRead()}
            >
              全部标为已读
            </button>
          </div>
          <p role="status">{specialRunMessage(state.run)}</p>
          {state.error && <p role="alert">{state.error}</p>}
          <p data-testid="special-counts">
            未读作品 {state.unread} 部 ·
            按来源与作品编号计数，同一作品的多位作者归属合并展示。
          </p>
          {!!snapshot?.authors.some(
            (row) => row.enabled && row.baselinesComplete < 2,
          ) && (
            <details open>
              <summary>部分作者基线尚未完成，旧作暂不计为新更新</summary>
              {snapshot.authors
                .filter((row) => row.enabled && row.baselinesComplete < 2)
                .map((row) => (
                  <p key={row.author}>
                    {row.author} · 已建立 {row.baselinesComplete} / 2 个来源基线
                    {row.errorCodes.length
                      ? ` · ${row.errorCodes.join("、")}`
                      : ""}
                  </p>
                ))}
            </details>
          )}
          <div className="source-actions">
            <button
              aria-pressed={onlyUnread}
              onClick={() => setOnlyUnread(true)}
            >
              只看未读
            </button>
            <button
              aria-pressed={!onlyUnread}
              onClick={() => setOnlyUnread(false)}
            >
              全部特别关注更新
            </button>
            <input
              aria-label="筛选特别关注更新"
              placeholder="筛选作品或作者…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          {!rows.length && (
            <p className="source-empty">
              当前筛选下没有作品。首次基线中的旧作品不会列为新更新。
            </p>
          )}
          <VirtualSourceGrid
            ref={grid}
            items={rows}
            density={props.density}
            itemKey={(row) => sourceWorkKey(row.work)}
            renderItem={(row) => {
              const work = row.work,
                scope = snapshot!.scopes.find(
                  (scope) => scope.source === work.source,
                )!,
                stock = inventory(work);
              return (
                <article
                  className="source-card"
                  data-testid={`special-work-${sourceWorkKey(work)}`}
                >
                  <CoverInteraction
                    className="source-card-open"
                    title={work.title}
                    request={sourceReaderRequest(scope, work)}
                    onDetails={() => props.onOpenWork(work)}
                  >
                    <div className="source-language-cover">
                      <SourceCover
                        adapter={props.sourceAdapter}
                        scope={scope}
                        work={work}
                      />
                      <SourceLanguageBadge
                        tags={work.tags}
                        work={work}
                        scope={scope}
                      />
                    </div>
                  </CoverInteraction>
                  <h3>
                    <button
                      className="text-button"
                      onClick={() => props.onOpenWork(work)}
                    >
                      {work.title}
                    </button>
                  </h3>
                  <AuthorLinks
                    authors={row.authors}
                    fallback="作者信息未提供"
                  />
                  <p>
                    {sourceLabel(work.source)} · {inventoryLabel(stock)} ·{" "}
                    {row.readAt === null ? "未读" : "已读"}
                  </p>
                  <DownloadWorkButton
                    work={work}
                    owned={stock.kind === "owned"}
                    ready={!!props.library.rootId}
                    onClick={() => props.onDownload(work)}
                  />
                </article>
              );
            }}
          />
        </>
      )}
    </section>
  );
}
