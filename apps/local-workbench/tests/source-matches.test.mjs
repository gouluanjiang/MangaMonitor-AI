import test from "node:test";
import assert from "node:assert/strict";
import {
  createSourceMatchesAdapter,
  lookupSourceMatchWork,
  validateSourceMatchesSnapshot,
} from "../src/source-matches-runtime.ts";
import { createLibraryMatcher } from "../src/library-model.ts";
import {
  createInventoryMatcher,
  createPhoneItemMatcher,
} from "../src/phone-library-model.ts";
import { emptyPhoneLibrary } from "../src/phone-library-types.ts";

// Synthetic identities and snapshots only. No real accounts, files or requests.
const picaId = "0123456789abcdef01234567";
const otherPica = "fedcba9876543210fedcba98";
const jm = { source: "JM", workId: "123", title: "合成作品 [版本甲]" };
const pica = { source: "Pica", workId: picaId, title: "另一来源的合成标题" };
const pair = {
  id: "a".repeat(64),
  jm,
  pica,
  confirmedAt: 1800000000000,
  evidence: "manual",
};
const snapshot = { revision: 1, pairs: [pair] };
const clone = (value) => structuredClone(value);
const item = {
  id: "b".repeat(64),
  relativePath: "合成电脑作品.zip",
  fileName: "合成电脑作品.zip",
  format: "zip",
  title: "合成电脑作品",
  authors: [],
  description: null,
  tags: [],
  bytes: 1024,
  modifiedAt: 1800000000000,
  pageCount: 2,
  coverAvailable: false,
  state: "indexed",
  errorCode: null,
  sourceRef: { source: "JM", workId: "123" },
  identityEvidence: "manual",
};
const library = (items = []) => ({
  revision: 1,
  rootId: "c".repeat(64),
  rootPath: "C:\\Synthetic library",
  generation: 1,
  phase: "complete",
  freshness: "live",
  items,
  visited: items.length,
  skipped: 0,
  updatedAt: 1800000000000,
  errorCode: null,
});
const phoneWithReference = (reference = jm) => ({
  ...emptyPhoneLibrary(),
  revision: 1,
  manualEntries: [
    {
      id: "d".repeat(64),
      name: reference.title,
      reference: { source: reference.source, workId: reference.workId },
      markedAt: 1800000000000,
    },
  ],
});

test("manual match snapshots reject conflicts on either source and non-manual evidence", () => {
  assert.deepEqual(validateSourceMatchesSnapshot(clone(snapshot)), snapshot);
  for (const invalid of [
    { ...snapshot, revision: -1 },
    { ...snapshot, pairs: [pair, pair] },
    {
      ...snapshot,
      pairs: [
        pair,
        { ...pair, id: "f".repeat(64), pica: { ...pica, workId: otherPica } },
      ],
    },
    {
      ...snapshot,
      pairs: [
        pair,
        { ...pair, id: "f".repeat(64), jm: { ...jm, workId: "456" } },
      ],
    },
    { ...snapshot, pairs: [{ ...pair, evidence: "filename" }] },
    { ...snapshot, pairs: [{ ...pair, jm: { ...jm, workId: "0123" } }] },
    { ...snapshot, pairs: [{ ...pair, pica: { ...pica, source: "JM" } }] },
    { ...snapshot, pairs: [{ ...pair, jm: { ...jm, title: "\u0085" } }] },
  ])
    assert.throws(
      () => validateSourceMatchesSnapshot(invalid),
      /SOURCE_MATCH_INVALID/,
    );
});

test("match adapter uses scoped commands and the exact revision without path or document writes", async () => {
  const calls = [];
  const adapter = createSourceMatchesAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return clone(snapshot);
    },
  });
  await adapter.read();
  await adapter.confirm(7, jm, pica);
  await adapter.unlink(8, pair.id);
  assert.deepEqual(calls, [
    { command: "source_matches_read", args: {} },
    { command: "source_matches_confirm", args: { revision: 7, jm, pica } },
    {
      command: "source_matches_unlink",
      args: { revision: 8, pairId: pair.id },
    },
  ]);
  assert.throws(
    () => adapter.confirm(8, { ...jm, workId: "../123" }, pica),
    /SOURCE_MATCH_INVALID/,
  );
  assert.throws(() => adapter.unlink(8, "C:\\private"), /SOURCE_MATCH_INVALID/);
  await assert.rejects(
    createSourceMatchesAdapter({
      native: false,
      invoke: async () => assert.fail("no browser IPC"),
    }).read(),
    /DESKTOP_REQUIRED/,
  );
  assert.equal(calls.length, 3);
});

test("opposite detail lookup previews only the requested source ID under the current session", async () => {
  const accounts = [
    { source: "Pica", sessionId: "synthetic-session", state: "connected" },
  ];
  const calls = [];
  const result = {
    source: "Pica",
    sessionId: "synthetic-session",
    items: [pica],
  };
  const adapter = {
    query: async (scope, query) => {
      calls.push({ scope, query });
      return result;
    },
  };
  assert.deepEqual(await lookupSourceMatchWork(adapter, accounts, pica), pica);
  assert.deepEqual(calls[0], {
    scope: { source: "Pica", sessionId: "synthetic-session" },
    query: { kind: "detail", query: picaId, folderId: null, page: 1 },
  });
  for (const invalid of [
    { ...result, sessionId: "previous-session" },
    { ...result, items: [{ ...pica, workId: otherPica }] },
    { ...result, items: [pica, pica] },
  ])
    await assert.rejects(
      lookupSourceMatchWork({ query: async () => invalid }, accounts, pica),
      /STALE_SESSION|INVALID_RESPONSE/,
    );
  await assert.rejects(
    lookupSourceMatchWork(adapter, [], pica),
    /LOGIN_REQUIRED/,
  );
  assert.equal(calls.length, 1);
});

test("confirmed alias projects existing PC evidence and unlink removes it without rewriting references", () => {
  const pc = library([clone(item)]),
    original = clone(pc);
  assert.equal(
    createInventoryMatcher(pc, emptyPhoneLibrary())(pica).kind,
    "missing",
  );
  const linked = createInventoryMatcher(pc, emptyPhoneLibrary(), true, [pair]);
  assert.equal(linked(pica).kind, "downloaded");
  assert.equal(linked(jm).kind, "downloaded");
  assert.equal(linked(pica).items[0], pc.items[0]);
  assert.equal(
    createInventoryMatcher(pc, emptyPhoneLibrary(), true, [])(pica).kind,
    "missing",
  );
  assert.deepEqual(pc, original);
  assert.equal(
    createInventoryMatcher(library(), emptyPhoneLibrary(), true, [pair])(pica)
      .kind,
    "missing",
    "a pair alone is not file or phone presence",
  );
});

test("phone-only explicit identity propagates to its counterpart with phone precedence and no title inference", () => {
  const phone = phoneWithReference(),
    original = clone(phone);
  const linked = createInventoryMatcher(undefined, phone, true, [pair]);
  assert.equal(linked(pica).kind, "owned");
  assert.equal(linked(jm).kind, "owned");
  assert.equal(
    linked({ ...pica, workId: otherPica, title: jm.title }).kind,
    "missing",
  );
  assert.equal(
    createInventoryMatcher(library([item]), phone, true, [pair])(pica).kind,
    "owned",
  );
  assert.equal(
    createInventoryMatcher(undefined, phone, true, [])(pica).kind,
    "missing",
  );
  assert.deepEqual(phone, original);
});

test("paired PC items inherit actual phone filename evidence and reverse manual references", () => {
  const phone = {
    ...emptyPhoneLibrary(),
    revision: 1,
    importedNames: [item.fileName],
    importedAt: 1800000000000,
    importFileName: "synthetic.txt",
  };
  assert.equal(
    createInventoryMatcher(library([item]), phone, true, [pair])(pica).kind,
    "owned",
  );
  assert.equal(
    createPhoneItemMatcher(phoneWithReference(pica), [pair])(item),
    "owned",
  );
  assert.equal(
    createPhoneItemMatcher(phoneWithReference(pica), [])(item),
    "downloaded",
  );
  assert.equal(
    createInventoryMatcher(library([item]), phone, false, [pair])(pica).kind,
    "unknown",
  );
});

test("titles stay candidates and an unread match store cannot assert a work is absent", () => {
  const pc = library([{ ...item, sourceRef: null, title: pica.title }]);
  assert.equal(createLibraryMatcher(pc, [pair])(pica).kind, "candidate");
  assert.equal(
    createInventoryMatcher(pc, emptyPhoneLibrary(), true, [pair])(pica).kind,
    "candidate",
  );
  assert.equal(
    createInventoryMatcher(
      library([item]),
      emptyPhoneLibrary(),
      true,
      [pair],
      false,
    )(pica).kind,
    "unknown",
  );
  assert.equal(
    createInventoryMatcher(
      undefined,
      phoneWithReference(),
      true,
      [pair],
      false,
    )(pica).kind,
    "unknown",
  );
  assert.equal(
    createInventoryMatcher(
      undefined,
      phoneWithReference(),
      true,
      [pair],
      false,
    )(jm).kind,
    "owned",
    "direct phone proof remains valid",
  );
});
