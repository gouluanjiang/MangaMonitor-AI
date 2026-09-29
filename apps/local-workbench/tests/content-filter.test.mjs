import test from "node:test";
import assert from "node:assert/strict";
import {
  isBlTag,
  isBlTagged,
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

test("BL uses whole metadata labels with case and Unicode width normalization", () => {
  for (const tag of [
    "BL",
    " ＢＬ ",
    "YaOi",
    "耽美",
    "ＢＬ漫畫",
    "Boys’ Love",
    "Boys Love",
    "ボーイズラブ",
  ])
    assert.equal(isBlTag(tag), true, tag);
  for (const tag of [
    "",
    "非BL",
    "非ＢＬ",
    "BLではない",
    "GL",
    "百合",
    "black",
    "blonde",
    "bl artist",
  ])
    assert.equal(isBlTag(tag), false, tag);
  assert.equal(isBlTagged(undefined), false);
  assert.equal(isContentHidden(work("unknown", [])), false);
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
