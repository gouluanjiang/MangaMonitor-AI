import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { SourceWork } from "./source-types.ts";
import type { ContentCheckPool, ContentCheckState } from "./content-check.ts";
import { isContentHidden } from "./content-filter.ts";
import { sourceErrorMessage } from "./source-runtime.ts";

/** Unknown metadata never mounts a cover, reader action or download action. */
export function ContentCheckedCard({
  pool,
  work,
  active,
  children,
}: {
  pool: ContentCheckPool;
  work: SourceWork;
  active: boolean;
  children(work: SourceWork): ReactNode;
}) {
  // A new scope/version remounts before paint: no frame may reuse an old verdict.
  return (
    <CheckedCard
      key={JSON.stringify([
        pool.scope.source,
        pool.scope.sessionId,
        work.workId,
        work.sourceUpdatedAt,
      ])}
      pool={pool}
      work={work}
      active={active}
    >
      {children}
    </CheckedCard>
  );
}
function CheckedCard({
  pool,
  work,
  active,
  children,
}: {
  pool: ContentCheckPool;
  work: SourceWork;
  active: boolean;
  children(work: SourceWork): ReactNode;
}) {
  const node = useRef<HTMLSpanElement>(null);
  const [near, setNear] = useState(false);
  const [verdict, setVerdict] = useState<{
    pool: ContentCheckPool;
    state: ContentCheckState;
  }>(() => ({ pool, state: pool.state(work) }));
  const state = verdict.pool === pool ? verdict.state : pool.state(work);
  const expired = state.phase === "ready" && !pool.verified(work);
  useEffect(() => {
    const element = node.current?.closest("article");
    if (!active || !element) {
      setNear(false);
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => setNear(entry.isIntersecting),
      {
        root: element.closest("main"),
        rootMargin: "160px 0px",
      },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [active, pool, work.workId]);
  useEffect(() => {
    setVerdict({ pool, state: pool.state(work) });
    if (active && near)
      return pool.watch(work, (state) => setVerdict({ pool, state }));
  }, [pool, active, near, work.workId, work.sourceUpdatedAt, expired]);
  const ready =
    state.phase === "ready" && !expired && !isContentHidden(state.work);
  return (
    <>
      <span ref={node} hidden />
      {ready ? (
        children(state.work)
      ) : (
        <>
          <div className="source-card-cover content-check-cover" role="status">
            {state.phase === "error"
              ? "标签核验失败"
              : state.phase === "ready"
                ? "已按内容偏好隐藏"
                : "正在核验内容标签…"}
          </div>
          <h3 className="content-check-title">核验通过后显示作品</h3>
          <p className="author-links" aria-hidden="true">
            &nbsp;
          </p>
          <p className="source-card-state">
            {state.phase === "error" ? "标签核验失败" : "内容偏好核验"}
          </p>
          <p className="source-card-date">
            {state.phase === "error"
              ? "可重试，不跳过检查"
              : "仅核验即将浏览的作品"}
          </p>
          <button
            className="text-button"
            disabled={state.phase !== "error"}
            title={
              state.phase === "error"
                ? sourceErrorMessage(state.error)
                : undefined
            }
            onClick={() => pool.retry(work)}
          >
            {state.phase === "error" ? "重试核验" : "等待核验"}
          </button>
        </>
      )}
    </>
  );
}
