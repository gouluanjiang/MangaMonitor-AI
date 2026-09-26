import test from "node:test";
import assert from "node:assert/strict";
import {
  initialBooklists,
  validateBooklists,
  createBooklist,
  renameBooklist,
  addBooklistMembers,
  removeBooklistMembers,
  archiveBooklist,
  restoreBooklist,
} from "../src/booklists.ts";

const now = 1_800_000_000_000;
const jm = { source: "JM", workId: "same-source-number" };
const pica = { source: "Pica", workId: "same-source-number" };
const unknown = { source: "JM", workId: "not-in-current-catalog" };

function freeze(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

function create(document = initialBooklists(), id = "list-a", name = "旅行") {
  return createBooklist(document, { id, name, now });
}

function refs(count) {
  return Array.from({ length: count }, (_, index) => ({
    source: index % 2 === 0 ? "JM" : "Pica",
    workId: "work-" + index,
  }));
}

function documentWithCounts(counts) {
  return {
    version: 1,
    lists: counts.map((count, index) => ({
      id: "list-" + index,
      name: "书单 " + index,
      createdAt: now,
      updatedAt: now,
      archived: false,
      members: refs(count),
    })),
  };
}

test("creation trims names and respects Unicode character limits without mutating inputs", () => {
  const empty = freeze(initialBooklists());
  const created = create(empty, "list-a", "  星海旅行  ");
  assert.equal(created.lists[0].name, "星海旅行");
  assert.equal(create(empty, "trimmed", "\t 星海 \n").lists[0].name, "星海");
  assert.deepEqual(empty, { version: 1, lists: [] });
  assert.equal(
    create(empty, "emoji", "🌙".repeat(80)).lists[0].name,
    "🌙".repeat(80),
  );
  for (const name of [
    "",
    "   ",
    "🌙".repeat(81),
    "一".repeat(81),
    "前\n后",
    "名\t称",
    "名\u0085称",
  ]) {
    assert.throws(() => create(empty, "invalid", name), /名称/);
  }
  const other = initialBooklists();
  other.lists.push(created.lists[0]);
  assert.deepEqual(initialBooklists(), { version: 1, lists: [] });
});

test("unarchived names are exact and case-sensitive; archived names do not block creation", () => {
  let document = create(initialBooklists(), "upper", "Travel");
  document = create(document, "lower", "travel");
  assert.equal(document.lists.length, 2);
  assert.throws(() => create(document, "duplicate", " Travel "), /同名/);
  assert.throws(
    () => renameBooklist(document, "lower", "Travel", now + 1),
    /同名/,
  );
  document = archiveBooklist(document, "upper", now + 1);
  document = create(document, "replacement", "Travel");
  assert.throws(() => restoreBooklist(document, "upper", now + 2), /同名/);
  document = renameBooklist(document, "upper", "Earlier Travel", now + 3);
  document = restoreBooklist(document, "upper", now + 4);
  assert.equal(
    document.lists.find((list) => list.id === "upper").archived,
    false,
  );
  assert.equal(
    document.lists.find((list) => list.id === "upper").name,
    "Earlier Travel",
  );
});

test("references deduplicate by exact source and id while preserving unknown works", () => {
  const document = freeze(
    addBooklistMembers(create(), "list-a", [unknown], now + 1),
  );
  const incoming = freeze([jm, jm, pica, unknown]);
  const next = addBooklistMembers(document, "list-a", incoming, now + 2);
  assert.deepEqual(next.lists[0].members, [unknown, jm, pica]);
  assert.deepEqual(document.lists[0].members, [unknown]);
  assert.deepEqual(incoming, [jm, jm, pica, unknown]);
  assert.equal(
    addBooklistMembers(next, "list-a", [jm, pica, unknown], now + 3),
    next,
  );
  assert.deepEqual(validateBooklists(JSON.parse(JSON.stringify(next))), next);
});

test("removing a member affects only the exact association in the chosen list", () => {
  let document = create();
  document = create(document, "list-b", "另一个书单");
  document = addBooklistMembers(
    document,
    "list-a",
    [jm, pica, unknown],
    now + 1,
  );
  document = addBooklistMembers(document, "list-b", [jm], now + 1);
  freeze(document);
  const removed = removeBooklistMembers(document, "list-a", [jm, jm], now + 2);
  assert.deepEqual(removed.lists[0].members, [pica, unknown]);
  assert.deepEqual(removed.lists[1], document.lists[1]);
  assert.deepEqual(document.lists[0].members, [jm, pica, unknown]);
  assert.equal(
    removeBooklistMembers(removed, "list-a", [jm], now + 3),
    removed,
  );
});

test("archive and restore retain all references and do not permit silent edits of archived members", () => {
  const original = freeze(
    addBooklistMembers(create(), "list-a", [jm, unknown], now + 1),
  );
  const archived = archiveBooklist(original, "list-a", now + 2);
  assert.equal(archived.lists[0].archived, true);
  assert.deepEqual(archived.lists[0].members, original.lists[0].members);
  assert.throws(
    () => addBooklistMembers(archived, "list-a", [pica], now + 3),
    /恢复/,
  );
  assert.throws(
    () => removeBooklistMembers(archived, "list-a", [jm], now + 3),
    /恢复/,
  );
  assert.equal(archiveBooklist(archived, "list-a", now + 3), archived);
  const restored = restoreBooklist(archived, "list-a", now + 4);
  assert.equal(restored.lists[0].archived, false);
  assert.deepEqual(restored.lists[0].members, [jm, unknown]);
  assert.equal(restored.lists[0].createdAt, now);
  assert.equal(restored.lists[0].updatedAt, now + 4);
  assert.equal(restoreBooklist(restored, "list-a", now + 5), restored);
});

test("timestamp validation rejects unsafe values and clock rollback cannot regress updatedAt", () => {
  for (const value of [
    -1,
    0.5,
    NaN,
    Infinity,
    Number.MAX_SAFE_INTEGER + 1,
    "1800000000000",
  ]) {
    assert.throws(
      () =>
        createBooklist(initialBooklists(), {
          id: "a",
          name: "时间",
          now: value,
        }),
      /时间/,
    );
  }
  const document = create();
  assert.equal(
    renameBooklist(document, "list-a", "新名称", now - 100).lists[0].updatedAt,
    now,
  );
  assert.equal(
    createBooklist(initialBooklists(), {
      id: "max",
      name: "最大安全时间",
      now: Number.MAX_SAFE_INTEGER,
    }).lists[0].createdAt,
    Number.MAX_SAFE_INTEGER,
  );
  const bad = structuredClone(document);
  bad.lists[0].updatedAt = now - 1;
  assert.throws(() => validateBooklists(bad), /早于/);
});

test("list ids and source references enforce their independent bounded alphabets", () => {
  assert.equal(
    create(initialBooklists(), "a".repeat(80)).lists[0].id.length,
    80,
  );
  for (const id of ["", "a".repeat(81), "space id", "../list", "名字"]) {
    assert.throws(() => create(initialBooklists(), id), /编号/);
  }
  const document = create();
  assert.throws(() => create(document, "list-a", "另一个名称"), /编号重复/);
  assert.equal(
    addBooklistMembers(
      document,
      "list-a",
      [
        {
          source: "JM",
          workId: "a".repeat(160),
        },
      ],
      now,
    ).lists[0].members[0].workId.length,
    160,
  );
  for (const reference of [
    { source: "jm", workId: "1" },
    { source: "Other", workId: "1" },
    { source: "JM", workId: "" },
    { source: "JM", workId: "a".repeat(161) },
    { source: "Pica", workId: "../1" },
    { source: "JM", workId: "1", account: "private" },
  ]) {
    assert.throws(
      () => addBooklistMembers(document, "list-a", [reference], now),
      /引用/,
    );
  }
});

test("100-list limit includes archived lists", () => {
  let document = documentWithCounts(Array(100).fill(0));
  document.lists[0].archived = true;
  freeze(document);
  assert.equal(validateBooklists(document), document);
  assert.throws(() => create(document, "over-limit", "更多"), /100/);
  const restored = restoreBooklist(document, "list-0", now + 1);
  assert.equal(restored.lists.length, 100);
});

test("2000-member boundary counts unique references and overflow leaves the original unchanged", () => {
  const original = freeze(create());
  const full = addBooklistMembers(original, "list-a", refs(2000), now + 1);
  assert.equal(full.lists[0].members.length, 2000);
  assert.equal(
    addBooklistMembers(full, "list-a", [full.lists[0].members[0]], now + 2),
    full,
  );
  const before = JSON.stringify(full);
  assert.throws(
    () => addBooklistMembers(full, "list-a", [unknown], now + 2),
    /2000/,
  );
  assert.equal(JSON.stringify(full), before);
  assert.equal(original.lists[0].members.length, 0);
});

test("20000-member total includes retained archive associations", () => {
  const document = freeze(documentWithCounts([...Array(10).fill(2000), 0]));
  assert.equal(validateBooklists(document), document);
  assert.throws(
    () => addBooklistMembers(document, "list-10", [unknown], now + 1),
    /20000/,
  );
  const archived = archiveBooklist(document, "list-0", now + 1);
  assert.throws(
    () => addBooklistMembers(archived, "list-10", [unknown], now + 2),
    /20000/,
  );
  const freed = removeBooklistMembers(
    document,
    "list-0",
    [document.lists[0].members[0]],
    now + 1,
  );
  const added = addBooklistMembers(freed, "list-10", [unknown], now + 2);
  assert.equal(
    added.lists.reduce((count, list) => count + list.members.length, 0),
    20000,
  );
});

test("invalid persisted documents are rejected rather than repaired or emptied", () => {
  const document = addBooklistMembers(create(), "list-a", [unknown], now);
  const invalid = [
    null,
    [],
    { version: 2, lists: [] },
    { ...document, queue: [] },
    { version: 1, lists: [...document.lists, document.lists[0]] },
  ];
  for (const update of [
    { members: [unknown, unknown] },
    { members: [{ source: "JM", workId: "invalid/id" }] },
    { name: "  未清理名称 " },
    { archived: "false" },
    { createdAt: -1 },
    { createdAt: now + 1 },
    { extra: "unsupported" },
  ]) {
    invalid.push({
      version: 1,
      lists: [{ ...document.lists[0], ...update }],
    });
  }
  for (const value of invalid) {
    const before = JSON.stringify(value);
    assert.throws(() => validateBooklists(freeze(value)), Error);
    assert.equal(JSON.stringify(value), before);
  }
});

test("unknown list operations fail without replacing or changing the input document", () => {
  const document = freeze(create());
  const commands = [
    () => renameBooklist(document, "missing", "名称", now),
    () => addBooklistMembers(document, "missing", [jm], now),
    () => removeBooklistMembers(document, "missing", [jm], now),
    () => archiveBooklist(document, "missing", now),
    () => restoreBooklist(document, "missing", now),
  ];
  for (const command of commands) assert.throws(command, /不可用/);
  assert.deepEqual(document, create());
});
