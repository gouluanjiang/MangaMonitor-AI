import {
  inheritLanguageTags,
  retainedLanguageTags,
} from "./source-language.ts";

// Whole metadata labels only. A title/author containing "bl" is not evidence.
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
]);
export function isBlTag(tag: string): boolean {
  return blLabels.has(
    tag
      .normalize("NFKC")
      .toLowerCase()
      .replace(/[’‘]/gu, "'")
      .replace(/\s+/gu, " ")
      .trim(),
  );
}
export function isBlTagged(tags?: readonly string[]): boolean {
  return tags?.some(isBlTag) ?? false;
}

/** Compact catalogs keep at most two language labels and one BL label. */
export function retainedContentTags(tags: readonly string[]): string[] {
  const retained = retainedLanguageTags(tags);
  const bl = tags.find(isBlTag);
  if (bl) retained.push(bl.trim());
  return retained;
}

/** A lightweight source response must not erase known explicit evidence. */
export function inheritContentTags(
  incoming: string[],
  previous: readonly string[],
): string[] {
  const tags = inheritLanguageTags(incoming, previous);
  const priorBl = !isBlTagged(tags) && previous.find(isBlTag);
  return priorBl && tags.length < 128 ? [...tags, priorBl.trim()] : tags;
}

type ContentWork = {
  source: string;
  workId: string;
  tags?: readonly string[];
};
// Only explicit positives live here, for this process. No disk persistence,
// title matching, account details, extra source reads, or cross-site guessing.
const known = new Set<string>();
const listeners = new Set<() => void>();
let revision = 0,
  notificationQueued = false;
const key = (work: ContentWork) => JSON.stringify([work.source, work.workId]);
export function rememberContentWork(work: ContentWork): void {
  if (!isBlTagged(work.tags) || known.has(key(work))) return;
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
  return isBlTagged(work.tags) || known.has(key(work));
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
