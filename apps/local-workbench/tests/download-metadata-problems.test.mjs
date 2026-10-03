import test from "node:test";
import assert from "node:assert/strict";
import {
  DownloadController,
  DownloadError,
  downloadErrorMessage,
} from "../src/download-runtime.ts";
import { downloadMetadataMessages } from "../src/download-metadata-problems.ts";
import {
  createDiagnosticProblem,
  diagnosticProblemLines,
} from "../src/diagnostics.ts";

const context = (source = "JM") => ({
  scope: { source, sessionId: "synthetic-private-session" },
  rootId: "a".repeat(64),
  generation: 1,
});
const controllerFixture = (extra = {}) => {
  let revision = 0,
    confirmed = 0;
  const adapter = {
    read: async () => ({ revision: ++revision, tasks: [] }),
    prepare: async () => {
      throw new Error("Unexpected preparation");
    },
    confirm: async () => {
      confirmed++;
      throw new Error("Unexpected confirmation");
    },
    ...extra,
  };
  return {
    controller: new DownloadController(adapter),
    confirmations: () => confirmed,
  };
};
const problemLines = (controller) =>
  diagnosticProblemLines(
    controller
      .getState()
      .metadataProblems.map((p) =>
        createDiagnosticProblem(
          "downloadPreparation",
          p.code,
          p.occurredAt,
          p.source,
        ),
      ),
  ).join("\n");

test("every metadata code has consistent inline and redacted diagnostic messages", () => {
  for (const [code, message] of Object.entries(downloadMetadataMessages)) {
    const cause = {
      code,
      message: "private-title C:/private-path",
      token: "private-token",
    };
    const problem = createDiagnosticProblem(
      "downloadPreparation",
      cause,
      1790812800000,
      "Pica",
    );
    assert.equal(problem.code, code);
    assert.equal(downloadErrorMessage(cause), message);
    const line = diagnosticProblemLines([problem]).join("\n");
    assert.ok(line.includes(message));
    assert.doesNotMatch(line, /private-|token|C:\//);
  }
  const problem = createDiagnosticProblem(
    "downloadPreparation",
    "DOWNLOAD_METADATA_PRIVATE_COOKIE",
    1790812800000,
    "JM",
  );
  assert.equal(problem.code, "UNCLASSIFIED_ERROR");
});

test("direct and mixed-source submission failures survive successful polling without creating tasks or retaining metadata", async (t) => {
  const codes = {
    JM: "DOWNLOAD_METADATA_TITLE_CONTROL",
    Pica: "DOWNLOAD_METADATA_AUTHOR_TOO_LONG",
  };
  const f = controllerFixture({
    prepare: async ({ scope }) => {
      throw {
        code: codes[scope.source],
        title: "private-title",
        url: "https://private.invalid/",
        account: "private-account",
      };
    },
  });
  t.after(() => f.controller.dispose());
  const contexts = { JM: context(), Pica: context("Pica") };
  const result = await f.controller.enqueueSelection(contexts, [
    { source: "JM", input: "123456" },
    { source: "Pica", input: "0123456789abcdef01234567" },
  ]);
  assert.equal(result.accepted.length, 0);
  assert.equal(result.failed.length, 2);
  assert.equal(f.confirmations(), 0);
  assert.equal(f.controller.getState().error, "");
  const before = JSON.stringify(f.controller.getState().metadataProblems);
  await f.controller.read();
  assert.equal(
    JSON.stringify(f.controller.getState().metadataProblems),
    before,
  );
  assert.deepEqual(f.controller.getState().snapshot.tasks, []);
  assert.doesNotMatch(
    before,
    /private-|0123456789abcdef01234567|123456|title|url|account|session|path/,
  );
  assert.match(problemLines(f.controller), /准备下载 · JM/);
  assert.match(problemLines(f.controller), /准备下载 · Pica/);
});

test("single preparation keeps only the last twenty fixed-time diagnostics and a fresh controller starts empty", async (t) => {
  let now = 1790812800000;
  t.mock.method(Date, "now", () => now);
  const f = controllerFixture({
    prepare: async () => {
      throw new DownloadError("DOWNLOAD_METADATA_TITLE_TOO_LONG");
    },
  });
  t.after(() => f.controller.dispose());
  for (let i = 0; i < 25; i++) {
    now++;
    await f.controller.prepare(context(), "123456");
  }
  const problems = f.controller.getState().metadataProblems;
  assert.equal(problems.length, 20);
  assert.equal(problems[0].occurredAt, now);
  assert.equal(problems.at(-1).occurredAt, now - 19);
  assert.equal(f.confirmations(), 0);
  const fresh = controllerFixture();
  t.after(() => fresh.controller.dispose());
  assert.deepEqual(fresh.controller.getState().metadataProblems, []);
});

test("batch item failures and rejected batches retain only allowlisted source-scoped diagnostics", async (t) => {
  let rejects = false;
  const f = controllerFixture({
    prepareBatch: async (_ctx, inputs) => {
      if (rejects)
        throw { code: "DOWNLOAD_METADATA_MISSING", private: "secret" };
      return {
        batchId: null,
        plans: [],
        issues: inputs.map((input) => ({
          input,
          errorCode: "DOWNLOAD_METADATA_TAG_EMPTY",
        })),
      };
    },
  });
  t.after(() => f.controller.dispose());
  await f.controller.prepareBatch(context("Pica"), [
    "0123456789abcdef01234567",
    "0123456789abcdef01234568",
  ]);
  assert.equal(f.controller.getState().metadataProblems.length, 2);
  rejects = true;
  await f.controller.prepareBatch(context(), ["123456"]);
  assert.equal(f.controller.getState().metadataProblems[0].source, "JM");
  assert.equal(
    f.controller.getState().metadataProblems[0].code,
    "DOWNLOAD_METADATA_MISSING",
  );
  assert.equal(f.confirmations(), 0);
});

test("cancelled or disposed preparation and changed-source sessions cannot append a late metadata diagnostic", async (t) => {
  for (const mode of ["cancel", "dispose", "session"]) {
    let entered, reject;
    const started = new Promise((resolve) => {
      entered = resolve;
    });
    const pending = new Promise((_resolve, fail) => {
      reject = fail;
    });
    const f = controllerFixture({
      prepare: async () => {
        entered();
        return pending;
      },
    });
    t.after(() => f.controller.dispose());
    let current = { JM: context(), Pica: null };
    const request =
      mode === "session"
        ? f.controller.enqueueSelection(
            current,
            [{ source: "JM", input: "123456" }],
            () => current,
          )
        : f.controller.prepare(context(), "123456");
    await started;
    if (mode === "cancel") f.controller.cancelPlan();
    if (mode === "dispose") f.controller.dispose();
    if (mode === "session") current = { JM: null, Pica: context("Pica") };
    reject(new DownloadError("DOWNLOAD_METADATA_AUTHOR_CONTROL"));
    await request;
    assert.deepEqual(f.controller.getState().metadataProblems, []);
    assert.equal(f.confirmations(), 0);
  }
});
