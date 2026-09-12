import test from "node:test";
import assert from "node:assert/strict";
import {
  createCompletionAdapter,
  validateCompletionView,
  validateCompletionSettings,
} from "../src/completion-runtime.ts";

// Synthetic metadata only. The native eligibility engine is tested in Rust;
// these tests exercise the renderer boundary and explicit command intent.
const scopes = [
  { source: "JM", sessionId: "synthetic-JM-1" },
  { source: "Pica", sessionId: "synthetic-Pica-1" },
];
const reference = { source: "Pica", workId: "0123456789abcdef01234567" };
const groupId = "a".repeat(64);
const evidenceHash = "b".repeat(64);
const clone = (value) => structuredClone(value);
const settings = { revision: 1, families: [], languages: [] };
function view() {
  const work = {
    ...reference,
    title: "Old translation [Chinese]",
    authors: ["Synthetic Writer"],
    description: null,
    tags: ["Chinese"],
    favorite: null,
    chapterCount: 1,
    pageCount: 20,
    coverAvailable: false,
  };
  return {
    discovery: {
      scopes: clone(scopes),
      revision: 4,
      run: null,
      authors: scopes.map((scope) => ({
        source: scope.source,
        author: "Synthetic Writer",
        state: "partial",
        lastAttemptAt: 100,
        lastCompleteAt: null,
        observedCount: 1,
        pagesRead: 1,
        errorCode: "SOURCE_UNAVAILABLE",
      })),
      records: [
        {
          work,
          matchedAuthors: ["Synthetic Writer"],
          authorVerified: true,
          observedAt: 1,
          scanId: "c".repeat(64),
        },
      ],
    },
    completeness: {
      revision: 1,
      phoneRevision: 2,
      libraryRevision: 3,
      matchesRevision: 0,
      discoveryRevision: 4,
      evidenceHash,
      groups: [
        {
          groupId,
          title: work.title,
          authors: work.authors,
          status: "translation_available",
          reasons: [],
          sources: [
            {
              reference: clone(reference),
              title: work.title,
              language: "chinese",
              authorVerified: true,
            },
          ],
          phone: [
            {
              member: { kind: "phone", name: "Original [Japanese]" },
              name: "Original [Japanese]",
              language: "japanese",
            },
          ],
          computer: [],
          eligible: {
            groupId,
            reference: clone(reference),
            kind: "translation",
            evidenceHash,
          },
        },
      ],
    },
    automatic: {
      runId: null,
      phase: "idle",
      queued: 0,
      skipped: 0,
      errorCode: null,
    },
  };
}

test("old omissions, partial source ranges and separate phone/PC language state survive the read-only boundary", () => {
  const fixture = view(),
    original = clone(fixture);
  assert.deepEqual(validateCompletionView(fixture, scopes), fixture);
  assert.deepEqual(fixture, original);
  for (const status of [
    "missing",
    "downloaded",
    "owned_chinese",
    "waiting_translation",
    "translation_downloaded",
    "review_required",
    "unknown",
  ]) {
    const next = view();
    next.completeness.groups[0].status = status;
    next.completeness.groups[0].eligible = null;
    assert.equal(
      validateCompletionView(next, scopes).completeness.groups[0].status,
      status,
    );
    assert.equal(next.discovery.records[0].observedAt, 1);
  }
  const longTitle = view();
  longTitle.discovery.records[0].work.title = "作".repeat(1500);
  longTitle.completeness.groups[0].sources[0].title = "作".repeat(1500);
  longTitle.completeness.groups[0].title = "作".repeat(1500);
  assert.equal(
    validateCompletionView(longTitle, scopes).completeness.groups[0].sources[0]
      .title.length,
    1500,
  );
});

test("account drift, malformed IDs, duplicated observations and mixed discovery generations are rejected", () => {
  const changed = clone(scopes);
  changed[1].sessionId = "synthetic-Pica-2";
  assert.throws(() => validateCompletionView(view(), changed), /STALE_SESSION/);
  const mutations = [
    (v) => {
      v.discovery.records.push(clone(v.discovery.records[0]));
    },
    (v) => {
      v.completeness.discoveryRevision++;
    },
    (v) => {
      v.completeness.groups[0].sources[0].reference.workId = "../private";
    },
    (v) => {
      v.completeness.phoneRevision = Number.MAX_SAFE_INTEGER + 1;
    },
    (v) => {
      v.discovery.authors[0].author = "bad\nname";
    },
    (v) => {
      v.completeness.groups[0].status = "automatically_owned";
    },
  ];
  for (const mutate of mutations) {
    const invalid = view();
    mutate(invalid);
    assert.throws(
      () => validateCompletionView(invalid, scopes),
      /COMPLETENESS_INVALID|INVALID_RESPONSE/,
    );
  }
});

test("eligible translation identity must agree with the enclosing group and evidence generation", () => {
  for (const mutate of [
    (v) => {
      v.completeness.groups[0].eligible.groupId = "d".repeat(64);
    },
    (v) => {
      v.completeness.groups[0].eligible.evidenceHash = "e".repeat(64);
    },
    (v) => {
      v.completeness.groups[0].eligible.reference = {
        source: "JM",
        workId: "999",
      };
    },
    (v) => {
      v.completeness.groups[0].sources[0].authorVerified = false;
    },
    (v) => {
      v.completeness.groups[0].sources[0].language = "unknown";
    },
  ]) {
    const invalid = view();
    mutate(invalid);
    assert.throws(
      () => validateCompletionView(invalid, scopes),
      /COMPLETENESS_INVALID/,
    );
  }
});

test("reading and correction commands never silently start scanning or authorize downloads", async () => {
  const calls = [];
  const adapter = createCompletionAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return command === "completeness_cancel"
        ? undefined
        : command === "completeness_read" || command === "completeness_start"
          ? view()
          : clone(settings);
    },
  });
  await adapter.read(scopes);
  await adapter.read(scopes, true);
  await adapter.settings();
  await adapter.family(1, [
    { kind: "source", reference },
    { kind: "phone", name: "Original [Japanese].zip" },
  ]);
  await adapter.language(
    2,
    { kind: "phone", name: "Original [Japanese]" },
    "japanese",
  );
  await adapter.language(
    3,
    { kind: "phone", name: "Original [Japanese]" },
    null,
  );
  await adapter.unlink(4, groupId);
  assert.equal(
    calls.filter((call) => /start|download/.test(call.command)).length,
    0,
  );
  assert.deepEqual(calls[1], {
    command: "completeness_read",
    args: { scopes, recheckFiles: true },
  });
  assert.deepEqual(calls[3].args, {
    revision: 1,
    members: [
      { kind: "source", reference },
      { kind: "phone", name: "Original [Japanese].zip" },
    ],
  });
  await adapter.start(scopes, [], true, "f".repeat(64), 3);
  assert.deepEqual(calls.at(-1), {
    command: "completeness_start",
    args: {
      scopes,
      authors: [],
      automatic: true,
      rootId: "f".repeat(64),
      generation: 3,
    },
  });
  await adapter.start(scopes, ["Synthetic Writer"], false, null, 0);
  assert.equal(calls.at(-1).args.automatic, false);
  await adapter.cancel("c".repeat(64));
  assert.deepEqual(calls.at(-1), {
    command: "completeness_cancel",
    args: { runId: "c".repeat(64) },
  });
});

test("manual family and per-member language settings retain references and no file authority", () => {
  const family = {
    revision: 1,
    families: [
      {
        id: groupId,
        members: [
          { kind: "source", reference },
          { kind: "phone", name: "Original [Japanese]" },
        ],
      },
    ],
    languages: [
      {
        member: { kind: "computer", itemId: "f".repeat(64) },
        language: "chinese",
      },
    ],
  };
  assert.deepEqual(validateCompletionSettings(family), family);
  assert.throws(
    () =>
      validateCompletionSettings({
        ...family,
        languages: [
          { member: family.languages[0].member, language: "auto_chinese" },
        ],
      }),
    /COMPLETENESS_INVALID/,
  );
});

test("browser-only adapters refuse IPC and private native error details never escape", async () => {
  await assert.rejects(
    createCompletionAdapter({
      native: false,
      invoke: async () => assert.fail("must not invoke"),
    }).read(scopes),
    /DESKTOP_REQUIRED/,
  );
  const native = createCompletionAdapter({
    native: true,
    invoke: async () => {
      throw { code: "not a code", message: "PRIVATE PATH TOKEN" };
    },
  });
  await assert.rejects(
    native.read(scopes),
    (error) =>
      error.code === "COMPLETENESS_UNAVAILABLE" &&
      !String(error).includes("PRIVATE"),
  );
});
