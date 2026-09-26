import type { SourceWork } from "./source-types.ts";

export function AuthorCreditNote({ work }: { work: SourceWork }) {
  if (!work.authorCreditReview) return null;
  return (
    <small
      className="source-muted"
      data-testid="author-credit-reviewed"
      title={`已按本作品核对署名。来源原署名：${work.authorCreditReview.originalAuthors.join("、")}`}
    >
      已核对署名
    </small>
  );
}
