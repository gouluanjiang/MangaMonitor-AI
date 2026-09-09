import test from "node:test";
import assert from "node:assert/strict";
import {
  createSourceAdapter,
  SourceError,
  sourceErrorMessage,
  validateSourceWork,
  validateCatalogSnapshot,
} from "../src/source-runtime.ts";
import {
  mergeSourceWorks,
  sourceWorkKey,
  toWorkReference,
} from "../src/source-types.ts";

const scope = { source: "JM", sessionId: "synthetic-session" };

test("catalog IPC preserves scope/reverse and rejects malformed terminal snapshots", async () => {
  const snapshot = {
    items: [work()],
    page: 1,
    total: 1,
    pages: 1,
    hasMore: false,
    folders: [],
    complete: true,
    updatedAt: 10,
    firstPageIds: ["123"],
  };
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      assert.equal(command, "source_catalog");
      assert.equal(args.reverse, true);
      assert.equal(args.sessionId, scope.sessionId);
      return { ...scope, snapshot, completeSnapshot: snapshot };
    },
  });
  assert.equal(
    (
      await adapter.catalog(scope, {
        action: "read",
        folderId: null,
        reverse: true,
      })
    ).snapshot.items.length,
    1,
  );
  for (const invalid of [
    { ...snapshot, complete: false },
    { ...snapshot, total: null, pages: null, hasMore: null },
    { ...snapshot, items: [], total: null, firstPageIds: [] },
    { ...snapshot, pages: 2 },
    { ...snapshot, firstPageIds: [] },
  ])
    assert.throws(() => validateCatalogSnapshot(invalid, scope), SourceError);
});
const work = (source = "JM", id = "123") => ({
  source,
  workId: id,
  title: "合成验收作品",
  authors: [],
  description: null,
  tags: [],
  favorite: null,
  chapterCount: null,
  pageCount: null,
  coverAvailable: false,
});
const account = (source = "JM") => ({
  source,
  sessionId: "synthetic-session",
  accountId: "synthetic-account",
  displayName: "合成验收账号",
  state: "connected",
  remembered: false,
  errorCode: null,
});
const page = (overrides = {}) => ({
  ...scope,
  items: [work()],
  page: 1,
  total: null,
  pages: null,
  hasMore: null,
  folders: [],
  ...overrides,
});
const query = { kind: "favorites", query: "", folderId: null, page: 1 };

test("browser sources are unavailable and never invoke a native or synthetic login", async () => {
  let invoked = 0;
  const adapter = createSourceAdapter({
    native: false,
    invoke: async () => {
      invoked++;
    },
  });
  assert.equal(adapter.mode, "unavailable");
  assert.equal(adapter.available, false);
  assert.deepEqual(
    (await adapter.accounts()).map((item) => [item.state, item.errorCode]),
    [
      ["unavailable", "DESKTOP_REQUIRED"],
      ["unavailable", "DESKTOP_REQUIRED"],
    ],
  );
  await assert.rejects(adapter.query(scope, query), {
    code: "DESKTOP_REQUIRED",
  });
  await assert.rejects(
    adapter.login({
      source: "JM",
      username: "synthetic",
      password: "fixture-only",
      remember: false,
    }),
    { code: "DESKTOP_REQUIRED" },
  );
  assert.equal(invoked, 0);
});

test("unknown source metadata remains null and unexpected native fields are not forwarded", () => {
  const result = validateSourceWork({
    ...work(),
    coverUrl: "https://private.invalid/cover",
    password: "not-a-real-secret",
  });
  assert.equal(result.pageCount, null);
  assert.equal(result.chapterCount, null);
  assert.equal(result.favorite, null);
  assert.deepEqual(result.authors, []);
  assert.equal("coverUrl" in result, false);
  assert.equal("password" in result, false);
});

test("page validation retains incomplete pagination and rejects duplicate identities", async () => {
  const adapter = createSourceAdapter({
    native: true,
    invoke: async () => page({ items: [work()] }),
  });
  const result = await adapter.query(scope, query);
  assert.equal(result.items.length, 1);
  assert.equal(result.total, null);
  assert.equal(result.pages, null);
  assert.equal(result.hasMore, null);
  assert.equal(result.items[0].pageCount, null);
  const duplicate = createSourceAdapter({
    native: true,
    invoke: async () => page({ items: [work(), work()] }),
  });
  await assert.rejects(duplicate.query(scope, query), {
    code: "CATALOG_CHANGED",
  });
});

test("query results from another source, session or page are rejected", async () => {
  for (const response of [
    page({ source: "Pica" }),
    page({ sessionId: "previous-session" }),
    page({ items: [work("Pica")] }),
    page({ page: 2 }),
  ]) {
    const adapter = createSourceAdapter({
      native: true,
      invoke: async () => response,
    });
    await assert.rejects(adapter.query(scope, query), SourceError);
  }
});

test("all account-bound requests preserve source and opaque session while Pica rejects folder operations", async () => {
  const calls = [];
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "source_accounts")
        return [account("JM"), account("Pica")];
      if (command === "source_query") return page();
      if (command === "source_cover")
        return { ...scope, workId: "123", dataUrl: null };
      if (command === "source_following")
        return { ...scope, revision: 0, works: [], authors: [] };
      throw new Error("unexpected synthetic command");
    },
  });
  await adapter.accounts(true);
  assert.deepEqual(calls[0], {
    command: "source_accounts",
    args: { refresh: true },
  });
  await adapter.query(scope, query);
  await adapter.cover(scope, "123");
  await adapter.following(scope);
  for (const call of calls.slice(1)) {
    assert.equal(call.args.source, scope.source);
    assert.equal(call.args.sessionId, scope.sessionId);
  }
  await assert.rejects(
    adapter.query(
      { source: "Pica", sessionId: "pica-session" },
      { ...query, folderId: "folder" },
    ),
    { code: "INVALID_INPUT" },
  );
  assert.equal(calls.length, 4);
});

test("favorite writes require matching identity and read-back confirmation of the desired state", async () => {
  const correct = {
    ...scope,
    workId: "123",
    favorite: true,
    changed: false,
    verified: true,
  };
  for (const result of [
    { ...correct, verified: false },
    { ...correct, favorite: false },
    { ...correct, workId: "456" },
    { ...correct, sessionId: "old" },
  ]) {
    const adapter = createSourceAdapter({
      native: true,
      invoke: async () => result,
    });
    await assert.rejects(adapter.favorite(scope, "123", true), SourceError);
  }
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      assert.equal(command, "source_favorite");
      assert.deepEqual(args, { ...scope, workId: "123", desired: true });
      return correct;
    },
  });
  assert.equal((await adapter.favorite(scope, "123", true)).verified, true);
});

test("only data images or null leave the native cover adapter", async () => {
  for (const dataUrl of [
    "https://private.invalid/cover",
    "file:///private.png",
    "data:image/svg+xml;base64,PHN2Zz4=",
  ]) {
    const adapter = createSourceAdapter({
      native: true,
      invoke: async () => ({ ...scope, workId: "123", dataUrl }),
    });
    await assert.rejects(adapter.cover(scope, "123"), {
      code: "INVALID_RESPONSE",
    });
  }
  const adapter = createSourceAdapter({
    native: true,
    invoke: async () => ({
      ...scope,
      workId: "123",
      dataUrl: "data:image/png;base64,aGVsbG8=",
    }),
  });
  assert.match(await adapter.cover(scope, "123"), /^data:image\/png/);
});

test("same work IDs from two sources stay separate when merged and passed to booklists", () => {
  const merged = mergeSourceWorks(
    [work("JM")],
    [work("Pica"), { ...work("JM"), title: "合成更新" }],
  );
  assert.equal(merged.length, 2);
  assert.deepEqual(merged.map(sourceWorkKey), ["JM:123", "Pica:123"]);
  assert.deepEqual(merged.map(toWorkReference), [
    { source: "JM", workId: "123" },
    { source: "Pica", workId: "123" },
  ]);
  assert.equal(merged[0].title, "合成更新");
});

test("following writes carry the reviewed revision and do not silently retry conflicts", async () => {
  let attempts = 0;
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      attempts++;
      assert.equal(command, "source_follow");
      assert.deepEqual(args, {
        ...scope,
        kind: "work",
        value: "123",
        desired: true,
        expectedRevision: 4,
      });
      throw {
        code: "REVISION_CONFLICT",
        message: "private details must be discarded",
      };
    },
  });
  await assert.rejects(
    adapter.follow(scope, {
      kind: "work",
      value: "123",
      desired: true,
      expectedRevision: 4,
    }),
    { code: "REVISION_CONFLICT" },
  );
  assert.equal(attempts, 1);
});

test("login only accepts its requested source and errors never retain raw credential-bearing text", async () => {
  const input = {
    source: "JM",
    username: "synthetic-user",
    password: "fixture-only",
    remember: true,
  };
  const wrong = createSourceAdapter({
    native: true,
    invoke: async () => account("Pica"),
  });
  await assert.rejects(wrong.login(input), { code: "INVALID_RESPONSE" });
  const failing = createSourceAdapter({
    native: true,
    invoke: async () => {
      throw {
        code: "LOGIN_REJECTED",
        message: "synthetic-user fixture-only https://private.invalid",
      };
    },
  });
  await assert.rejects(failing.login(input), (error) => {
    assert.equal(error.message, "LOGIN_REJECTED");
    assert.doesNotMatch(
      sourceErrorMessage(error),
      /synthetic-user|fixture-only|private/,
    );
    return true;
  });
  assert.match(
    sourceErrorMessage(new SourceError("FAVORITE_OUTCOME_UNKNOWN")),
    /先重新读取/,
  );
  assert.match(
    sourceErrorMessage(new SourceError("SOURCE_ACCESS_DENIED")),
    /会话未被清除/,
  );
});

test("remembered invalid sessions can be forgotten without inventing a usable session ID", async () => {
  const adapter = createSourceAdapter({
    native: true,
    invoke: async (command, args) => {
      assert.equal(command, "source_logout");
      assert.deepEqual(args, { source: "JM", sessionId: null });
      return {
        ...account(),
        sessionId: null,
        accountId: null,
        displayName: null,
        state: "disconnected",
        remembered: false,
      };
    },
  });
  const result = await adapter.logout({ source: "JM", sessionId: null });
  assert.equal(result.state, "disconnected");
  assert.equal(result.remembered, false);
  await assert.rejects(
    adapter.query({ source: "JM", sessionId: null }, query),
    { code: "LOGIN_REQUIRED" },
  );
});
