import test from "node:test";
import assert from "node:assert/strict";
import { createAuthorCatalogIndex } from "../src/author-catalog-index.ts";
import { sourceWorkKey } from "../src/source-types.ts";

const jm = { source: "JM", sessionId: "synthetic-jm-session" };
const pica = { source: "Pica", sessionId: "synthetic-pica-session" };
const record = (workId, source = "JM", author = "Writer") => ({
  work: {
    source,
    workId,
    title: "Synthetic work",
    authors: [author],
    tags: [],
    description: null,
    favorite: null,
    chapterCount: null,
    pageCount: null,
    coverAvailable: false,
  },
  matchedAuthors: ["Writer"],
  authorVerified: true,
  observedAt: 1,
  scanId: "synthetic-scan",
});
const snapshot = (revision, records, scopes = [jm, pica]) => ({
  revision,
  records,
  scopes,
  run: null,
  authors: [],
});
const key = (workId, source = "JM") => sourceWorkKey({ source, workId });

test("an older local read cannot overwrite newer author catalog membership", async () => {
  const catalog = createAuthorCatalogIndex();
  let finishOldRead;
  const oldRead = new Promise((resolve) => {
    finishOldRead = resolve;
  });
  const received = oldRead.then((value) => catalog.remember(value));
  assert.equal(
    catalog.remember(snapshot(12, [record("new"), record("also-new", "Pica")])),
    true,
  );
  finishOldRead(snapshot(11, [record("old")]));
  assert.equal(await received, false);
  assert.deepEqual(
    [...catalog.read([jm, pica])],
    [key("new"), key("also-new", "Pica")],
  );
});

test("newer complete snapshots can remove records and only confirmed author memberships count", () => {
  const catalog = createAuthorCatalogIndex();
  catalog.remember(snapshot(1, [record("removed"), record("kept")]));
  catalog.remember(
    snapshot(2, [record("kept"), record("unrelated", "JM", "Other Writer")]),
  );
  assert.deepEqual([...catalog.read([jm, pica])], [key("kept")]);
  catalog.remember(snapshot(3, []));
  assert.equal(catalog.read([jm, pica]).size, 0);
});

test("revision ordering is per source session and does not leak across sign-ins", () => {
  const catalog = createAuthorCatalogIndex();
  catalog.remember(snapshot(50, [record("jm-high")], [jm]));
  catalog.remember(snapshot(2, [record("jm-old"), record("pica-low", "Pica")]));
  assert.deepEqual(
    [...catalog.read([jm, pica])],
    [key("jm-high"), key("pica-low", "Pica")],
  );
  const nextJm = { ...jm, sessionId: "new-synthetic-session" };
  assert.equal(catalog.read([nextJm]).size, 0);
  catalog.remember(snapshot(1, [record("new-account")], [nextJm]));
  assert.deepEqual([...catalog.read([nextJm])], [key("new-account")]);
  assert.deepEqual([...catalog.read([jm])], [key("jm-high")]);
});
