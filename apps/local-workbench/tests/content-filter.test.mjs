import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  isBlTag,
  isBlTagged,
  isAiTag,
  isJmFemaleTag,
  isBlockedTag,
  isBlockedTagged,
  retainedContentTags,
  inheritContentTags,
  rememberContentWork,
  isContentHidden,
  subscribeContentFilter,
  getContentFilterRevision,
} from "../src/content-filter.ts";
import { compactWork } from "../src/source-memory.ts";
import { mergeSourceWorks } from "../src/source-types.ts";
import { appendCatalog } from "../src/source-collection.ts";

const work = (workId, tags, source = "JM") => ({
  source,
  workId,
  tags,
  title: "BL in a title is not a category",
  authors: ["Yaoi is an author name here"],
  description: "Metadata fixture",
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});

test("女性向 is retained explicit JM evidence, not a Pica or title guess", () => {
  const jm = { ...work("female-scope", []), categories: [" 女性向 "] };
  assert.equal(isJmFemaleTag(" 女性向 "), true);
  assert.equal(isJmFemaleTag("非女性向"), false);
  assert.equal(isBlockedTag("女性向"), false);
  assert.equal(isContentHidden(jm), true);
  assert.equal(isContentHidden({ ...jm, source: "Pica" }), false);
  assert.equal(
    isContentHidden({ ...work("female-title", []), title: "女性向" }),
    false,
  );
  const compact = compactWork(jm);
  assert.deepEqual(compact.tags, ["女性向"]);
  assert.equal(
    isContentHidden(mergeSourceWorks([compact], [work(jm.workId, [])])[0]),
    true,
  );
  const full = inheritContentTags(
    Array.from({ length: 128 }, (_, i) => `tag${i}`),
    ["女性向"],
  );
  assert.equal(full.length, 128);
  assert.ok(full.some(isJmFemaleTag));
});

test("exact label matrix is shared with Rust and never matches ordinary words", () => {
  const matrix = JSON.parse(
    readFileSync(
      new URL(
        "../../../crates/workbench-sources/src/content-labels.test.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  for (const [kind, matches] of [
    ["bl", isBlTag],
    ["ai", isAiTag],
  ]) {
    for (const tag of matrix[kind]) {
      assert.equal(matches(tag), true, tag);
      assert.equal(isBlockedTag(tag), true, tag);
    }
  }
  for (const tag of matrix.visible) assert.equal(isBlockedTag(tag), false, tag);
  assert.equal(isBlTagged(undefined), false);
  assert.equal(isBlockedTagged(undefined), false);
  assert.equal(isContentHidden(work("unknown", [])), false);
  assert.equal(
    isContentHidden({
      ...work("ordinary-ai-title", []),
      title: "AI研究",
      authors: ["AI"],
    }),
    false,
  );
});

test("AI and Pica category evidence survive compaction and lightweight refresh", () => {
  const original = {
    ...work("category-only-content", ["中文", "生肉"], "Pica"),
    categories: ["同人", "耽美花園", "AI漫画"],
  };
  assert.equal(isContentHidden(original), true);
  const compact = compactWork(original);
  assert.deepEqual(compact.tags, ["中文", "生肉", "耽美花園", "AI漫画"]);
  assert.deepEqual(compact.categories, ["耽美花園", "AI漫画"]);
  assert.equal(compactWork(compact), compact);
  const merged = mergeSourceWorks(
    [compact],
    [work(original.workId, ["生肉"], "Pica")],
  );
  assert.deepEqual(merged[0].tags, ["生肉", "耽美花園", "AI漫画"]);
  const legacy = mergeSourceWorks(
    [original],
    [work(original.workId, [], "Pica")],
  );
  assert.deepEqual(legacy[0].tags, ["中文", "生肉", "耽美花園", "AI漫画"]);
  const catalog = appendCatalog(null, {
    items: [original],
    page: 1,
    pages: 1,
    total: 1,
    hasMore: false,
    folders: [],
  });
  assert.equal(catalog.items[0].tags.some(isAiTag), true);
  assert.equal(isContentHidden(catalog.items[0]), true);
});

test("128-label refresh retains both explicit blocked kinds without growing the cap", () => {
  const incoming = Array.from({ length: 128 }, (_, n) => `ordinary${n}`);
  const previous = ["中文", "生肉", "耽美花园", "AI作畫"];
  const inherited = inheritContentTags(incoming, previous);
  assert.equal(inherited.length, 128);
  assert.equal(inherited.some(isBlTag), true);
  assert.equal(inherited.some(isAiTag), true);
  assert.equal(incoming.length, 128);
  assert.equal(incoming.some(isBlockedTag), false);
  assert.deepEqual(inheritContentTags(Array(128).fill("中文"), previous), [
    "中文",
    "耽美花园",
    "AI作畫",
  ]);
});

test("category-only AI evidence is shared with queue and lightweight exact-ID records", async () => {
  const item = {
    ...work("category-only-ai-memory", [], "Pica"),
    categories: ["ＡＩ"],
  };
  rememberContentWork(item);
  assert.equal(isContentHidden(work(item.workId, [], "Pica")), true);
  assert.equal(isContentHidden(work(item.workId, [], "JM")), false);
  await Promise.resolve();
});

test("compact caches and lightweight merges preserve BL alongside language conflicts", () => {
  const original = work("content-fixture", [
    "Tag",
    "中文",
    "日文",
    "Yaoi",
    "BL",
  ]);
  assert.deepEqual(retainedContentTags(original.tags), [
    "中文",
    "日文",
    "Yaoi",
  ]);
  const compact = compactWork(original);
  assert.deepEqual(compact.tags, ["中文", "日文", "Yaoi"]);
  assert.equal(compact.description, null);
  assert.equal(compactWork(compact), compact);
  const merged = mergeSourceWorks([compact], [work(original.workId, [])]);
  assert.equal(isBlTagged(merged[0].tags), true);
  assert.deepEqual(inheritContentTags(["生肉"], original.tags), [
    "生肉",
    "Yaoi",
  ]);
  const catalog = appendCatalog(null, {
    items: [original],
    page: 1,
    pages: 1,
    total: 1,
    hasMore: false,
    folders: [],
  });
  assert.equal(isBlTagged(catalog.items[0].tags), true);
  const categories = compactWork({
    ...original,
    categories: ["Tag", "中文", "Yaoi"],
  });
  assert.deepEqual(categories.categories, ["中文", "Yaoi"]);
  assert.equal(compactWork(categories), categories);
});

test("known explicit evidence is shared for an exact source and ID, with one batched notification", async () => {
  let notifications = 0;
  const unsubscribe = subscribeContentFilter(() => notifications++);
  const before = getContentFilterRevision();
  const item = work("shared-content-evidence", ["BL"]);
  rememberContentWork(item);
  rememberContentWork(item);
  assert.equal(getContentFilterRevision(), before + 1);
  assert.equal(isContentHidden(work(item.workId, [])), true);
  assert.equal(isContentHidden(work(item.workId, [], "Pica")), false);
  assert.equal(isContentHidden(work("unrelated-content-id", [])), false);
  await Promise.resolve();
  assert.equal(notifications, 1);
  unsubscribe();
});
