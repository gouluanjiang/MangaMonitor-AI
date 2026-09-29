import test from "node:test";
import assert from "node:assert/strict";
import {
  classifySourceLanguage,
  hasLanguageTags,
  inheritLanguageTags,
  retainedLanguageTags,
} from "../src/source-language.ts";
import { compactWork } from "../src/source-memory.ts";
import { mergeSourceWorks } from "../src/source-types.ts";
import { appendCatalog } from "../src/source-collection.ts";

test("explicit language and untranslated tags classify conservatively without substring guessing", () => {
  for (const tag of [
    "中文",
    "汉化",
    "漢化",
    "简体中文",
    "繁體中文",
    "中国語",
    " CHINESE ",
  ])
    assert.equal(classifySourceLanguage([tag]).kind, "chinese");
  for (const tag of ["日文", "日語", "日本語", " Japanese ", "生肉"])
    assert.equal(classifySourceLanguage([tag]).kind, "untranslated");
  for (const tags of [
    [],
    ["日漫"],
    ["原创"],
    ["合成汉化组"],
    ["日文作者"],
    ["英語 ENG"],
    ["未汉化"],
    ["英文", "成人"],
  ]) {
    assert.equal(classifySourceLanguage(tags).kind, "unknown");
    assert.equal(hasLanguageTags(tags), false);
  }
  const conflict = ["中文", "生肉"];
  assert.equal(classifySourceLanguage(conflict).label, "未知");
  assert.match(classifySourceLanguage(conflict).explanation, /冲突/);
  assert.equal(hasLanguageTags(conflict), true);
  assert.match(classifySourceLanguage(["生肉"]).explanation, /不一定是日语/);
});

const work = (source, id, tags) => ({
  source,
  workId: id,
  tags,
  title: "日本語のタイトル [中文翻译]",
  authors: ["汉化组"],
  description: "language words in prose are not evidence",
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});

test("catalog compaction keeps at most two language labels including conflicts without title inference", () => {
  const original = work("JM", "123", [
    "奇幻",
    " 中文 ",
    "漢化",
    "生肉",
    "日文",
  ]);
  const compact = compactWork(original);
  assert.deepEqual(compact.tags, ["中文", "生肉"]);
  assert.equal(compact.description, null);
  assert.deepEqual(original.tags, ["奇幻", " 中文 ", "漢化", "生肉", "日文"]);
  assert.equal(compactWork(compact), compact);
  assert.equal(classifySourceLanguage(compact.tags).kind, "unknown");
  const unknown = compactWork(work("JM", "124", []));
  assert.equal(classifySourceLanguage(unknown.tags).kind, "unknown");
  const catalog = appendCatalog(null, {
    items: [original],
    page: 1,
    total: 1,
    pages: 1,
    hasMore: false,
    folders: [],
  });
  assert.deepEqual(catalog.items[0].tags, ["中文", "生肉"]);
  assert.deepEqual(retainedLanguageTags(["日文", "中文", "生肉", "漢化"]), [
    "日文",
    "中文",
  ]);
});

test("language enrichment keeps explicit fresh evidence and never inherits across source or work identity", () => {
  const previous = work("JM", "123", ["中文"]);
  const compact = work("JM", "123", ["奇幻"]);
  assert.deepEqual(mergeSourceWorks([previous], [compact])[0].tags, [
    "奇幻",
    "中文",
  ]);
  assert.deepEqual(
    mergeSourceWorks([previous], [work("JM", "123", ["生肉"])])[0].tags,
    ["生肉"],
  );
  assert.deepEqual(
    mergeSourceWorks([previous], [work("JM", "123", ["中文", "生肉"])])[0].tags,
    ["中文", "生肉"],
  );
  const other = mergeSourceWorks(
    [previous],
    [work("Pica", "123", []), work("JM", "124", [])],
  );
  assert.deepEqual(
    other.map((value) => value.tags),
    [["中文"], [], []],
  );
  const sixtyFour = Array(64).fill("genre");
  assert.equal(inheritLanguageTags(sixtyFour, ["中文", "生肉"]).length, 66);
  assert.equal(
    classifySourceLanguage(inheritLanguageTags(sixtyFour, ["中文", "生肉"]))
      .kind,
    "unknown",
  );
  assert.equal(inheritLanguageTags(sixtyFour, ["genre"]), sixtyFour);
  const invalid = Array(65).fill("genre");
  assert.equal(inheritLanguageTags(invalid, ["中文", "生肉"]), invalid);
});
