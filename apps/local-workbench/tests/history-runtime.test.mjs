import test from "node:test";
import assert from "node:assert/strict";
import { validateHistory } from "../src/history-runtime.ts";
const entry = {
  identity: { kind: "source", source: "JM", workId: "123" },
  title: "Synthetic",
  visitedAt: 100,
};
const raw = (entries) => ({
  revision: 1,
  value: { version: 1, enabled: true, entries },
});
test("history only accepts bounded stable references and no duplicate identities", () => {
  assert.equal(validateHistory(raw([entry])).entries.length, 1);
  assert.throws(() => validateHistory(raw([entry, entry])));
  assert.throws(() =>
    validateHistory(
      raw([
        {
          ...entry,
          identity: { ...entry.identity, workId: "https://invalid" },
        },
      ]),
    ),
  );
  assert.throws(() => validateHistory(raw([{ ...entry, visitedAt: -1 }])));
  assert.throws(() =>
    validateHistory({
      revision: 1,
      value: { version: 99, enabled: true, entries: [] },
    }),
  );
  assert.throws(() =>
    validateHistory(
      raw(
        Array.from({ length: 101 }, (_, i) => ({
          ...entry,
          identity: { ...entry.identity, workId: String(i) },
        })),
      ),
    ),
  );
});
