import test from "node:test";
import assert from "node:assert/strict";
import {
  historyCoverTarget,
  historyEntryVisible,
  historyIdentityKey,
  historyLibraryItem,
} from "../src/history-covers.ts";
import { emptyLibrary } from "../src/library-types.ts";
import { rememberContentWork } from "../src/content-filter.ts";

const rootId = "a".repeat(64);
const entryId = "b".repeat(64);
const item = {
  id: entryId,
  relativePath: "Synthetic.zip",
  fileName: "Synthetic.zip",
  title: "Synthetic",
  authors: [],
  description: null,
  tags: [],
  format: "zip",
  bytes: 50,
  modifiedAt: null,
  pageCount: 1,
  coverAvailable: true,
  state: "indexed",
  errorCode: null,
  sourceRef: null,
  identityEvidence: null,
};
const library = { ...emptyLibrary(), rootId, generation: 4, items: [item] };
const account = (source) => ({
  source,
  sessionId: "current-" + source,
  state: "connected",
});
const entry = (identity) => ({ identity, title: "Synthetic", visitedAt: 100 });

test("history cover identities preserve source and library root boundaries", () => {
  const identities = [
    { kind: "source", source: "JM", workId: "123" },
    { kind: "source", source: "Pica", workId: "123" },
    { kind: "library", rootId, entryId },
    { kind: "library", rootId: "c".repeat(64), entryId },
  ];
  assert.equal(new Set(identities.map(historyIdentityKey)).size, 4);
  assert.equal(
    historyIdentityKey(identities[0]),
    historyIdentityKey({ workId: "123", source: "JM", kind: "source" }),
  );
  assert.equal(historyLibraryItem(identities[2], library), item);
  assert.equal(historyLibraryItem(identities[3], library), undefined);
});

test("source history covers resolve current matching sessions without making visit metadata", () => {
  for (const source of ["JM", "Pica"]) {
    const row = entry({ kind: "source", source, workId: "123" });
    const original = structuredClone(row);
    const result = historyCoverTarget(
      row,
      [account("Pica"), account("JM")],
      library,
    );
    assert.equal(result.kind, "source");
    assert.deepEqual(result.scope, { source, sessionId: "current-" + source });
    assert.deepEqual(result.work.authors, []);
    assert.deepEqual(result.work.tags, []);
    assert.equal(result.work.coverAvailable, false);
    assert.equal(result.work.workId, "123");
    assert.equal(result.work.title, row.title);
    assert.deepEqual(row, original);
    assert.equal(historyCoverTarget(row, [], library).kind, "unavailable");
    assert.equal(
      historyCoverTarget(
        row,
        [{ ...account(source), state: "expired" }],
        library,
      ).kind,
      "unavailable",
    );
    assert.equal(
      historyCoverTarget(
        row,
        [account(source === "JM" ? "Pica" : "JM")],
        library,
      ).kind,
      "unavailable",
    );
  }
});

test("local history uses exact current entry references and keeps stale entries explainable", () => {
  const identity = { kind: "library", rootId, entryId };
  const row = entry(identity);
  const result = historyCoverTarget(row, [], library);
  assert.deepEqual(result, { kind: "library", item });
  assert.equal(
    historyCoverTarget(row, [], { ...library, rootId: "c".repeat(64) }).message,
    "此记录来自其他漫画库目录",
  );
  assert.equal(
    historyCoverTarget(row, [], {
      ...library,
      items: [{ ...item, id: "d".repeat(64) }],
    }).message,
    "当前漫画库中未找到此文件",
  );
  assert.equal(historyEntryVisible(identity, emptyLibrary()), true);
  assert.deepEqual(
    historyCoverTarget(row, [], {
      ...library,
      items: [{ ...item, state: "unreadable", errorCode: "LIBRARY_RECYCLED" }],
    }),
    { kind: "unavailable", message: "文件已移到回收站" },
  );
  assert.equal(
    historyEntryVisible(identity, {
      ...library,
      items: [
        {
          ...item,
          state: "unreadable",
          errorCode: "LIBRARY_RECYCLED",
          coverAvailable: false,
        },
      ],
    }),
    true,
  );
  assert.deepEqual(
    historyCoverTarget(row, [], {
      ...library,
      items: [
        { ...item, state: "unreadable", errorCode: "LIBRARY_FILE_MISSING" },
      ],
    }),
    { kind: "unavailable", message: "此文件暂时无法读取" },
  );
});

test("history cover filtering consumes only existing explicit evidence and never title guesses", () => {
  const local = { kind: "library", rootId, entryId };
  const snapshot = (changes) => ({
    ...library,
    items: [{ ...item, ...changes }],
  });
  assert.equal(
    historyEntryVisible(local, snapshot({ title: "BL AI unknown label" })),
    true,
  );
  assert.equal(
    historyEntryVisible(local, snapshot({ tags: ["耽美花園"] })),
    false,
  );
  assert.equal(
    historyEntryVisible(local, snapshot({ tags: ["AI生成"] })),
    false,
  );
  assert.equal(
    historyEntryVisible(local, snapshot({ tags: ["女性向"] })),
    true,
  );
  assert.equal(
    historyEntryVisible(
      local,
      snapshot({
        tags: ["女性向"],
        sourceRef: { source: "JM", workId: "123" },
      }),
    ),
    false,
  );
  const jm = { kind: "source", source: "JM", workId: "known-history-filter" };
  rememberContentWork({ ...jm, tags: ["BL"] });
  assert.equal(historyEntryVisible(jm, library), false);
  assert.equal(historyEntryVisible({ ...jm, source: "Pica" }, library), true);
  assert.equal(historyEntryVisible(local, snapshot({ sourceRef: jm })), false);
});
