import type { Source, SourceItemIssue } from "./source-types.ts";
import { sourceLabel } from "./source-types.ts";
import { useState } from "react";

export function SourceIssues({
  source,
  issues = [],
  count = issues.length,
  pagesComplete = false,
  testId = "source-issues",
}: {
  source: Source;
  issues?: SourceItemIssue[];
  count?: number;
  pagesComplete?: boolean;
  testId?: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const [limit, setLimit] = useState(100);
  if (!count) return null;
  return (
    <details
      className="source-notice"
      data-testid={testId}
      onToggle={(event) => setExpanded(event.currentTarget.open)}
    >
      <summary>
        {pagesComplete ? "分页已读完 · " : ""}来源记录待核对 {count} 条 ·
        查看异常位置
      </summary>
      <p>
        正常作品已保留；以下异常不会新增到作品与入库统计，已有可靠记录仍会保留。异常条目不能直接下载，可重新读取来源后核对。
      </p>
      {expanded &&
        issues.slice(0, limit).map((issue) => (
          <p key={`${issue.page}:${issue.index}`}>
            {sourceLabel(source)} · 第 {issue.page} 页 · 第 {issue.index} 条 ·{" "}
            {issue.workId === null ? "编号缺失" : `编号 ${issue.workId}`} ·{" "}
            {issue.code === "SOURCE_ITEM_METADATA_MISSING"
              ? "作品信息缺失"
              : "作品信息格式异常"}
          </p>
        ))}
      {expanded && issues.length > limit && (
        <button
          className="text-button"
          onClick={() => setLimit((previous) => previous + 100)}
        >
          继续查看异常位置
        </button>
      )}
      {count > issues.length && (
        <p>
          显示前 {issues.length} 条异常位置，共 {count}{" "}
          条；其余异常同样不会新增到作品统计。
        </p>
      )}
    </details>
  );
}
