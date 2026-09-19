/** Query eligibility, not a claim that a short pen name is not a real author. */
export function authorQueryError(name: string): string | null {
  const value = name
    .replace(/[\uFF01-\uFF5E]/gu, (char) =>
      String.fromCharCode(char.charCodeAt(0) - 0xfee0),
    )
    .trim()
    .toLowerCase();
  if (
    [
      "n/a",
      "n.a.",
      "unknown",
      "none",
      "null",
      "未知作者",
      "作者不明",
      "作者不详",
      "作者不詳",
    ].includes(value)
  )
    return "AUTHOR_QUERY_PLACEHOLDER";
  return /^[a-z0-9]$/u.test(value) ? "AUTHOR_QUERY_TOO_BROAD" : null;
}

export function authorQueryMessage(code: unknown): string | null {
  if (code === "AUTHOR_QUERY_PLACEHOLDER")
    return "作者名是缺失信息的占位值，本次未发送查询。请使用真实作者署名。";
  if (code === "AUTHOR_QUERY_TOO_BROAD")
    return "单个字母或数字无法限定作者范围，本次未发送查询。请使用完整的“社团（作者）”署名。";
  return null;
}
