import test from "node:test";
import assert from "node:assert/strict";
import {
  authorNameMatches,
  partitionAuthorRecords,
  partitionAuthorWorks,
} from "../src/author-evidence.ts";

test("author evidence accepts explicit circle/coauthor names and Unicode width/case differences", () => {
  for (const [query, name] of [
    ["Writer", "  WRITER  "],
    ["Writer", "Ｓｔｕｄｉｏ（Ｗｒｉｔｅｒ）"],
    ["Writer", "Studio (Other、Writer)"],
    ["Writer", "Other, Writer"],
    ["Writer", "Other & Writer"],
    ["Writer", "【Studio】 [Writer]"],
    ["Writer", "Studio (Circle (Writer))"],
    ["Studio (Writer)", "Writer"],
    ["Studio (Writer)", "Other studio (Writer)"],
    ["バナナ", "ハ\u3099ナナ"],
  ])
    assert.equal(authorNameMatches(query, name), true, `${query} / ${name}`);
});

test("evidenced aliases classify cached metadata per source without guessing spaces, traditional characters or circle membership", () => {
  const policy = {
    source: "JM",
    author: "Writer Name",
    queries: ["Writer Name"],
    verifiedAliases: ["WriterName", "筆名"],
    exactCredits: [],
    queryFingerprint: "a".repeat(64),
  };
  const records = [
    record("11", ["WriterName"], [policy.author], "JM"),
    record("12", ["筆名"], [policy.author], "JM"),
    record("13", ["笔名"], [policy.author], "JM"),
    record("14", ["WriterName"], [policy.author], "Pica"),
    record("15", ["WriterNameTwo"], [policy.author], "JM"),
  ];
  const before = structuredClone(records);
  assert.equal(partitionAuthorRecords(records).confirmed.length, 0);
  const classified = partitionAuthorRecords(records, "", "all", [policy]);
  assert.deepEqual(
    classified.confirmed.map((row) => row.work.workId),
    ["11", "12"],
  );
  assert.deepEqual(
    classified.other.map((row) => row.work.workId),
    ["13", "14", "15"],
  );
  assert.deepEqual(
    records,
    before,
    "alias-only projection never mutates stored works or ownership",
  );
  const joined = {
    ...policy,
    author: "Studio (Writer)",
    verifiedAliases: ["WriterName"],
  };
  const candidates = [
    record("21", ["Studio (Different Writer)"], [joined.author], "JM"),
    record("22", ["WriterName"], [joined.author], "JM"),
    record("23", ["Studio"], [joined.author], "JM"),
  ];
  assert.deepEqual(
    partitionAuthorRecords(candidates, "", "all", [joined]).confirmed.map(
      (row) => row.work.workId,
    ),
    ["22"],
  );
});

test("whole-credit evidence accepts only the verified composite field and does not create a coauthor alias", () => {
  const policy = {
    source: "JM",
    author: "Writer",
    queries: ["Writer"],
    verifiedAliases: [],
    exactCredits: ["WriterCollaborator"],
    queryFingerprint: "a".repeat(64),
  };
  const works = [
    record("31", ["WriterCollaborator"], ["Writer"], "JM").work,
    record("32", ["Collaborator"], ["Writer"], "JM").work,
    record("33", ["Studio (WriterCollaborator)"], ["Writer"], "JM").work,
    record("34", ["WriterCollaborator"], ["Writer"], "Pica").work,
  ];
  const result = partitionAuthorWorks(works, "Writer", policy);
  assert.deepEqual(
    result.confirmed.map((row) => row.workId),
    ["31"],
  );
  assert.deepEqual(
    result.other.map((row) => row.workId),
    ["32", "33", "34"],
  );
});

test("no substring, kana/voicing, common-circle or incomplete-label inference", () => {
  for (const [query, name] of [
    ["Writer", "WriterTwo"],
    ["Writer", "OtherWriter"],
    ["Writer", "Studio (WriterTwo)"],
    ["Writer One", "Writer Two"],
    ["バナナ", "ハナナ"],
    ["かな", "カナ"],
    ["Studio (Writer)", "Studio (Other)"],
    ["Writer", "Studio (Writer"],
    ["Writer", "Studio (Writer…)"],
    ["Writer", ""],
    ["", "Writer"],
  ])
    assert.equal(authorNameMatches(query, name), false, `${query} / ${name}`);
});

const record = (id, authors, queries = ["Writer"], source = "Pica") => ({
  work: {
    source,
    workId: id,
    title: "Writer keyword in title",
    authors,
    description: "Writer",
    tags: ["Writer"],
  },
  matchedAuthors: queries,
  authorVerified: true,
  observedAt: 1,
  scanId: "old-keyword-scan",
});

test("old keyword memberships and verified bits cannot label other/absent authors as the queried author", () => {
  const records = [
    record("1", ["Studio (Writer)"]),
    record("2", ["Other"]),
    record("3", []),
  ];
  const before = structuredClone(records);
  const result = partitionAuthorRecords(records);
  assert.deepEqual(
    result.confirmed.map((r) => r.work.workId),
    ["1"],
  );
  assert.deepEqual(
    result.other.map((r) => r.work.workId),
    ["2", "3"],
  );
  assert.deepEqual(
    records,
    before,
    "projection never deletes saved results or changes receipts",
  );
});

test("shared query IDs are classified for the selected author and source, not any saved membership", () => {
  const records = [
    record("1", ["Other"], ["Writer", "Other"]),
    record("2", ["Writer"], ["Writer"], "JM"),
  ];
  assert.equal(partitionAuthorRecords(records).confirmed.length, 2);
  assert.equal(
    partitionAuthorRecords(records, "Writer", "Pica").confirmed.length,
    0,
  );
  assert.equal(
    partitionAuthorRecords(records, "Writer", "Pica").other.length,
    1,
  );
  assert.equal(partitionAuthorRecords(records, "Other", "JM").other.length, 0);
  assert.equal(
    partitionAuthorRecords(records, "Other", "Pica").confirmed.length,
    1,
  );
});
