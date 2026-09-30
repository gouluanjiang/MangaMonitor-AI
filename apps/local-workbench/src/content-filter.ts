import {
  inheritLanguageTags,
  languageTagKind,
  retainedLanguageTags,
} from "./source-language.ts";

// Whole metadata labels only. Titles, authors and descriptions are never evidence.
const blLabels = new Set([
  "bl",
  "耽美",
  "yaoi",
  "boys love",
  "boys' love",
  "boy's love",
  "boys-love",
  "boys_love",
  "boyslove",
  "ボーイズラブ",
  "男男",
  "bl漫畫",
  "bl漫画",
  "bl漫",
  "bl向",
  "耽美漫畫",
  "耽美漫画",
  "耽美向",
  "耽美花園",
  "耽美花园",
]);
const aiLabels = new Set([
  "ai",
  "aigc",
  "ai漫画",
  "ai漫畫",
  "ai作画",
  "ai作畫",
  "ai绘画",
  "ai繪畫",
  "ai绘图",
  "ai繪圖",
  "ai绘制",
  "ai繪製",
  "ai生成",
  "ai生成漫画",
  "ai生成漫畫",
  "ai生成作品",
  "aiart",
  "aiartwork",
  "aicomic",
  "aicomics",
  "aigenerated",
  "aigeneratedart",
  "aigeneratedcomic",
  "aigeneratedcomics",
  "aiイラスト",
  "aiコミック",
  "aiマンガ",
  "ai絵",
]);
function normalizedLabel(tag: string): string {
  return tag
    .normalize("NFKC")
    .toLowerCase()
    .replace(/[’‘]/gu, "'")
    .replace(/\s+/gu, " ")
    .trim();
}
export function isBlTag(tag: string): boolean {
  return blLabels.has(normalizedLabel(tag));
}
export function isAiTag(tag: string): boolean {
  // Only separators within a complete known label are optional; no substring match.
  return aiLabels.has(normalizedLabel(tag).replace(/[ _-]/gu, ""));
}
export function isBlockedTag(tag: string): boolean {
  return isBlTag(tag) || isAiTag(tag);
}
export function isBlTagged(tags?: readonly string[]): boolean {
  return tags?.some(isBlTag) ?? false;
}
export function isBlockedTagged(tags?: readonly string[]): boolean {
  return tags?.some(isBlockedTag) ?? false;
}
export function isJmFemaleTag(tag: string): boolean {
  return normalizedLabel(tag) === "女性向";
}
export function isJmEnglishCategory(tag: string): boolean {
  return normalizedLabel(tag) === "english manga";
}

/** Compact catalogs keep at most two language labels and one label per blocked kind. */
export function retainedContentTags(tags: readonly string[]): string[] {
  const retained = retainedLanguageTags(tags);
  for (const matches of [
    isBlTag,
    isAiTag,
    isJmFemaleTag,
    isJmEnglishCategory,
  ]) {
    const label = tags.find(matches);
    if (label) retained.push(label.trim());
  }
  return retained;
}

/** A lightweight source response must not erase known explicit evidence. */
export function inheritContentTags(
  incoming: string[],
  previous: readonly string[],
): string[] {
  let tags = inheritLanguageTags(incoming, previous);
  for (const matches of [
    isBlTag,
    isAiTag,
    isJmFemaleTag,
    isJmEnglishCategory,
  ]) {
    const prior = !tags.some(matches) && previous.find(matches);
    if (!prior) continue;
    if (tags === incoming) tags = [...tags];
    if (tags.length >= 128) {
      const removable = tags.findLastIndex(
        (tag) =>
          !languageTagKind(tag) &&
          !isBlockedTag(tag) &&
          !isJmFemaleTag(tag) &&
          !isJmEnglishCategory(tag),
      );
      // A saturated list consisting only of evidence can be compacted safely.
      tags =
        removable < 0
          ? retainedContentTags(tags)
          : tags.toSpliced(removable, 1);
    }
    tags.push(prior.trim());
  }
  return tags;
}

type ContentWork = {
  source: string;
  workId: string;
  tags?: readonly string[];
  categories?: readonly string[];
};
// Only explicit positives live here, for this process. No disk persistence,
// title matching, account details, extra source reads, or cross-site guessing.
const known = new Set<string>();
const listeners = new Set<() => void>();
let revision = 0,
  notificationQueued = false;
const key = (work: ContentWork) => JSON.stringify([work.source, work.workId]);
const excludedByMetadata = (work: ContentWork) =>
  isBlockedTagged(work.tags) ||
  isBlockedTagged(work.categories) ||
  (work.source === "JM" &&
    [...(work.tags ?? []), ...(work.categories ?? [])].some(isJmFemaleTag));
/** The accepted JM author/search scope is narrower than general browsing. */
export const isOutsideJmAuthorScope = (work: ContentWork) =>
  work.source === "JM" &&
  [...(work.tags ?? []), ...(work.categories ?? [])].some(isJmEnglishCategory);
export function rememberContentWork(work: ContentWork): void {
  if (!excludedByMetadata(work) || known.has(key(work))) return;
  known.add(key(work));
  revision++;
  if (!notificationQueued) {
    notificationQueued = true;
    queueMicrotask(() => {
      notificationQueued = false;
      for (const listener of listeners) listener();
    });
  }
}
export function isContentHidden(work: ContentWork): boolean {
  return excludedByMetadata(work) || known.has(key(work));
}
export function subscribeContentFilter(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
export function getContentFilterRevision(): number {
  return revision;
}
