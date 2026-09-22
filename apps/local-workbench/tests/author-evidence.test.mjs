import test from "node:test";
import assert from "node:assert/strict";
import {
  authorNameMatches,
  partitionAuthorRecords,
  partitionAuthorWorks,
  projectAuthorWork,
  workHasAuthor,
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

test("reviewed per-work credits replace attribution without altering source records or creating aliases", () => {
  const correction = {
    workId: "101",
    expectedAuthors: ["Wrong Writer", "Other credit"],
    correctedAuthors: ["Studio (True Writer、Guest)"],
  };
  const policy = (author, source = "JM") => ({
    source,
    author,
    queries: [author],
    verifiedAliases: [],
    queryFingerprint: "a".repeat(64),
    workCredits: [correction],
  });
  const raw = record(
    "101",
    [" Other   credit ", "ＷＲＯＮＧ　ＷＲＩＴＥＲ"],
    ["Wrong Writer", "True Writer"],
    "JM",
  );
  const before = structuredClone(raw);
  const trueResults = partitionAuthorWorks(
    [raw.work],
    "True Writer",
    policy("True Writer"),
  );
  assert.equal(trueResults.confirmed.length, 1);
  assert.deepEqual(
    trueResults.confirmed[0].authors,
    correction.correctedAuthors,
  );
  assert.deepEqual(
    trueResults.confirmed[0].authorCreditReview.originalAuthors,
    raw.work.authors,
  );
  assert.equal(
    workHasAuthor(raw.work, "Wrong Writer", policy("Wrong Writer")),
    false,
  );
  assert.equal(workHasAuthor(raw.work, "Guest", policy("Guest")), true);
  assert.equal(
    workHasAuthor(raw.work, "Unlisted Guest", policy("Unlisted Guest")),
    false,
  );
  const policies = [policy("True Writer"), policy("Wrong Writer")];
  assert.equal(
    partitionAuthorRecords([raw], "Wrong Writer", "all", policies).confirmed
      .length,
    0,
  );
  assert.equal(
    partitionAuthorRecords([raw], "True Writer", "all", policies).confirmed
      .length,
    1,
  );
  assert.equal(
    partitionAuthorRecords([raw], "", "all", policies).confirmed.length,
    1,
  );
  const wrongOnly = { ...raw, matchedAuthors: ["Wrong Writer"] };
  assert.equal(
    partitionAuthorRecords([wrongOnly], "True Writer", "JM", policies).confirmed
      .length,
    1,
    "reviewed old record becomes visible to the actual followed author without a new website query",
  );
  assert.equal(
    partitionAuthorRecords([wrongOnly], "Wrong Writer", "JM", policies).other
      .length,
    1,
  );
  assert.deepEqual(
    wrongOnly.matchedAuthors,
    ["Wrong Writer"],
    "saved query membership and coverage are not rewritten",
  );
  assert.equal(
    partitionAuthorRecords(
      [{ ...wrongOnly, work: { ...wrongOnly.work, workId: "999" } }],
      "True Writer",
      "JM",
      policies,
    ).confirmed.length,
    0,
    "ordinary records do not gain memberships",
  );
  assert.deepEqual(
    raw,
    before,
    "title, source ID, stored authors, query membership and metadata remain byte-equivalent",
  );
  for (const changed of [
    { ...raw.work, source: "Pica" },
    { ...raw.work, workId: "102" },
    { ...raw.work, authors: ["Wrong Writer"] },
    { ...raw.work, authors: ["Wrong Writer", "Other credit", "New credit"] },
    { ...raw.work, authors: ["Site corrected this"] },
  ]) {
    assert.equal(projectAuthorWork(changed, [policy("True Writer")]), changed);
    assert.equal(
      workHasAuthor(changed, "True Writer", policy("True Writer")),
      false,
    );
  }
  assert.equal(
    workHasAuthor(raw.work, "True Writer", policy("Wrong Writer")),
    false,
    "wrong active author policy is not reused",
  );
  assert.equal(
    partitionAuthorWorks(
      [{ ...raw.work, workId: "102" }],
      "True Writer",
      policy("True Writer"),
    ).confirmed.length,
    0,
    "another work by the old credit is not an alias",
  );
});

test("work-credit projections are idempotent, revocable and conservatively reject conflicting policy copies", () => {
  const raw = record("103", ["Incorrect"], ["Correct"], "JM").work;
  const policy = {
    source: "JM",
    author: "Correct",
    queries: ["Correct"],
    verifiedAliases: [],
    queryFingerprint: "a".repeat(64),
    workCredits: [
      {
        workId: "103",
        expectedAuthors: ["Incorrect"],
        correctedAuthors: ["Correct"],
      },
    ],
  };
  const projected = projectAuthorWork(raw, [policy]);
  assert.deepEqual(projectAuthorWork(projected, [policy, policy]), projected);
  assert.deepEqual(
    projectAuthorWork(projected, []),
    raw,
    "removing a policy removes its ephemeral projection",
  );
  const conflict = {
    ...policy,
    author: "Conflicting",
    workCredits: [
      { ...policy.workCredits[0], correctedAuthors: ["Conflicting"] },
    ],
  };
  assert.equal(projectAuthorWork(raw, [policy, conflict]), raw);
  const aliasPolicy = {
    ...policy,
    author: "Display Name",
    verifiedAliases: ["Correct"],
  };
  assert.equal(workHasAuthor(raw, "Display Name", aliasPolicy), true);
  assert.equal(
    workHasAuthor({ ...raw, workId: "104" }, "Display Name", aliasPolicy),
    false,
  );
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
