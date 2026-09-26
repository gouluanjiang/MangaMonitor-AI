export type SourceLanguageKind = "chinese" | "untranslated" | "unknown";

// Exact language/version labels only. Do not infer from titles, creators,
// ordinary categories, or a translation group's name containing these words.
const chinese = new Set([
  "中文",
  "汉化",
  "漢化",
  "简体中文",
  "簡體中文",
  "繁体中文",
  "繁體中文",
  "中国語",
  "中國語",
  "chinese",
]);
const untranslated = new Set([
  "日文",
  "日语",
  "日語",
  "日本語",
  "japanese",
  "生肉",
]);

export function languageTagKind(
  tag: string,
): Exclude<SourceLanguageKind, "unknown"> | null {
  const value = tag.trim().toLowerCase();
  return chinese.has(value)
    ? "chinese"
    : untranslated.has(value)
      ? "untranslated"
      : null;
}

export function hasLanguageTags(tags: readonly string[]): boolean {
  return tags.some((tag) => languageTagKind(tag) !== null);
}

/** At most one original label per class; preserve both sides of a conflict. */
export function retainedLanguageTags(tags: readonly string[]): string[] {
  const seen = new Set<SourceLanguageKind>();
  return tags.flatMap((tag) => {
    const kind = languageTagKind(tag);
    if (!kind || seen.has(kind)) return [];
    seen.add(kind);
    return [tag.trim()];
  });
}

/** Fresh explicit evidence wins; a lightweight response can reuse known labels. */
export function inheritLanguageTags(
  incoming: string[],
  previous: readonly string[],
): string[] {
  if (hasLanguageTags(incoming)) return incoming;
  const retained = retainedLanguageTags(previous);
  // Raw source tags are capped at 64 and enrichment adds at most two. If an
  // invalid/legacy projection has no room, never keep only one conflict side.
  return retained.length && incoming.length + retained.length <= 66
    ? [...incoming, ...retained]
    : incoming;
}

export function classifySourceLanguage(tags: readonly string[]): {
  kind: SourceLanguageKind;
  label: string;
  explanation: string;
} {
  const kinds = new Set(tags.map(languageTagKind));
  const chinese = kinds.has("chinese");
  const raw = kinds.has("untranslated");
  if (chinese && raw)
    return {
      kind: "unknown",
      label: "未知",
      explanation: "中文／汉化与日文／生肉标签冲突，暂不判断。",
    };
  if (chinese)
    return {
      kind: "chinese",
      label: "已汉化",
      explanation:
        "标签明确标注中文或汉化；沿用“已汉化”显示，也可能是中文原创。",
    };
  if (raw)
    return {
      kind: "untranslated",
      label: "生肉",
      explanation: "标签明确标注日文或生肉；生肉表示未翻译版本，不一定是日语。",
    };
  return {
    kind: "unknown",
    label: "未知",
    explanation: "未取得明确的中文、日文或生肉标签，不根据标题猜测。",
  };
}
