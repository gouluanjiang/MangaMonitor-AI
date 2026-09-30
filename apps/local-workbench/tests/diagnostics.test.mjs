import test from "node:test";
import assert from "node:assert/strict";
import {
  diagnosticSummary,
  createDiagnosticProblem,
  diagnosticProblemLines,
  validateWorkbenchInfo,
} from "../src/diagnostics.ts";
import { matchingSettingsPages } from "../src/settings-navigation.ts";
import { emptyLibrary } from "../src/library-types.ts";
import { emptyDownloads } from "../src/download-types.ts";
import { LibraryController } from "../src/library-runtime.ts";
import { DownloadController } from "../src/download-runtime.ts";

const failureProblem = (operation, state) =>
  createDiagnosticProblem(
    operation,
    state.failure.cause,
    state.failure.occurredAt,
  );
const failureSummary = (problem) =>
  diagnosticSummary({
    info: null,
    accounts: [],
    accountsLoading: false,
    accountsFailed: false,
    library: emptyLibrary(),
    libraryFailed: problem.operation === "library",
    downloads: emptyDownloads(),
    downloadsReady: true,
    downloadsFailed: problem.operation === "downloads",
    preferencesReady: true,
    preferencesFailed: false,
    problems: [problem],
  });

test("library read and scan preserve native diagnostic codes and occurrence time while cancel and success retain their error semantics", async (t) => {
  let now = Date.UTC(2026, 8, 30, 3, 4, 5);
  t.mock.method(Date, "now", () => now);
  const cause = {
    code: "DOCUMENT_CORRUPT",
    message: "private-password C:/private-library",
    account: "private-account",
  };
  let failRead = true,
    scanError = "LIBRARY_ROOT_CHANGED";
  const snapshot = {
    ...emptyLibrary(),
    rootId: "a".repeat(64),
    rootPath: "C:/private-library",
    generation: 1,
    phase: "complete",
    freshness: "live",
  };
  const controller = new LibraryController({
    read: async () => {
      if (failRead) throw cause;
      return snapshot;
    },
    choose: async () => null,
    scan: async (_root, generation) => ({
      ...snapshot,
      generation: generation + 1,
      phase: scanError ? "error" : "complete",
      errorCode: scanError,
    }),
  });
  t.after(() => controller.dispose());
  await controller.read();
  const failure = controller.getState().failure;
  assert.equal(failure.cause, cause);
  assert.equal(failure.occurredAt, now);
  const report = failureSummary(
    failureProblem("library", controller.getState()),
  );
  assert.match(report, /DOCUMENT_CORRUPT/);
  assert.match(report, /2026-09-30T03:04:05.000Z/);
  assert.doesNotMatch(
    report + controller.getState().error,
    /private-|password|C:\//,
  );
  now += 60000;
  await controller.choose();
  assert.equal(controller.getState().failure, failure);
  assert.equal(
    failureSummary(failureProblem("library", controller.getState())),
    report,
  );
  failRead = false;
  await controller.read();
  assert.equal(controller.getState().failure, undefined);
  assert.equal(controller.getState().error, "");
  await controller.scan("start");
  assert.equal(controller.getState().failure.cause, scanError);
  assert.equal(controller.getState().failure.occurredAt, now);
  assert.equal(
    failureProblem("library", controller.getState()).code,
    scanError,
  );
  scanError = null;
  await controller.scan("start");
  assert.equal(controller.getState().error, "");
  assert.equal(controller.getState().failure, undefined);
});

test("download read and control preserve distinct failure codes and fixed times without exporting private native causes", async (t) => {
  let now = Date.UTC(2026, 8, 30, 4, 5, 6),
    failRead = true;
  t.mock.method(Date, "now", () => now);
  const readCause = {
    code: "DOWNLOAD_DISK_FULL",
    message: "private-token C:/private-download",
  };
  const controlCause = {
    code: "DOWNLOAD_INDEX_FAILED",
    message: "private-title",
    session: "private-session",
  };
  const controller = new DownloadController({
    read: async () => {
      if (failRead) throw readCause;
      return emptyDownloads();
    },
    control: async () => {
      throw controlCause;
    },
  });
  t.after(() => controller.dispose());
  await controller.read();
  const readFailure = controller.getState().failure;
  assert.equal(readFailure.cause, readCause);
  assert.equal(readFailure.occurredAt, now);
  const report = failureSummary(
    failureProblem("downloads", controller.getState()),
  );
  assert.match(report, /DOWNLOAD_DISK_FULL/);
  assert.match(report, /2026-09-30T04:05:06.000Z/);
  now += 60000;
  assert.equal(
    failureSummary(failureProblem("downloads", controller.getState())),
    report,
  );
  await controller.control(
    { source: "JM", sessionId: "synthetic-session" },
    {
      id: "synthetic-task",
      source: "JM",
      revision: 1,
      allowedActions: ["resume"],
    },
    "resume",
  );
  assert.equal(controller.getState().failure.cause, controlCause);
  assert.equal(controller.getState().failure.occurredAt, now);
  const controlReport = failureSummary(
    failureProblem("downloads", controller.getState()),
  );
  assert.match(controlReport, /DOWNLOAD_INDEX_FAILED/);
  assert.match(controlReport, /2026-09-30T04:06:06.000Z/);
  assert.doesNotMatch(
    report + controlReport + controller.getState().error,
    /private-|token|C:\//,
  );
  failRead = false;
  await controller.read();
  assert.equal(controller.getState().error, "");
  assert.equal(controller.getState().failure, undefined);
});

test("diagnostic problems keep only allowed codes, operations, source and fixed occurrence time", () => {
  const at = Date.UTC(2026, 8, 30, 3, 4, 5);
  const known = createDiagnosticProblem(
    "downloads",
    { code: "DOWNLOAD_INDEX_FAILED", title: "private-title" },
    at,
    "JM",
  );
  assert.deepEqual(known, {
    operation: "downloads",
    code: "DOWNLOAD_INDEX_FAILED",
    occurredAt: at,
    source: "JM",
  });
  const unknown = createDiagnosticProblem(
    "library",
    { code: "LIBRARY_PRIVATE_ACCOUNT_COOKIE", path: "C:/private-path" },
    at,
  );
  assert.equal(unknown.code, "UNCLASSIFIED_ERROR");
  const lines = diagnosticProblemLines([
    known,
    unknown,
    {
      operation: "accounts",
      code: "private-password",
      occurredAt: at,
      source: "private-user",
    },
    { operation: "private-operation", code: "BUSY", occurredAt: at },
    { operation: "preferences", code: "BUSY", occurredAt: NaN },
    { operation: "preferences", code: "BUSY", occurredAt: Infinity },
  ]);
  assert.equal(lines.length, 3);
  assert.match(
    lines[0],
    /下载队列 · JM · 2026-09-30T03:04:05.000Z · DOWNLOAD_INDEX_FAILED/,
  );
  assert.doesNotMatch(lines.join("\n"), /private|COOKIE|password|C:\//);
  assert.deepEqual(
    diagnosticProblemLines([known]),
    diagnosticProblemLines([known]),
  );
});

test("diagnostics report only allowed statuses and counts, even with private DTO fields", () => {
  const state = {
    info: validateWorkbenchInfo({
      version: "0.3.4",
      revision: "a".repeat(40),
      platform: "windows",
      secret: "do-not-copy",
    }),
    accounts: [
      {
        source: "JM",
        state: "connected",
        sessionId: "private-session",
        displayName: "private-account",
        errorCode: "private-error",
      },
    ],
    accountsLoading: false,
    accountsFailed: false,
    library: {
      ...emptyLibrary(),
      rootId: "private-root",
      rootPath: "C:/private-library",
      phase: "complete",
      items: [
        {
          title: "private-title",
          relativePath: "private-path",
          state: "unreadable",
          errorCode: "private-error",
        },
        { state: "indexed", errorCode: null, pageCount: 0 },
        { state: "indexed", errorCode: null, pageCount: null },
        { state: "indexed", errorCode: "private-error", pageCount: 12 },
        { state: "indexed", errorCode: null, pageCount: 12 },
      ],
    },
    libraryFailed: false,
    downloads: {
      revision: 1,
      tasks: [
        {
          phase: "downloaded",
          localFiles: "present",
          libraryEntryId: "entry",
          title: "private-title",
          workId: "private-work",
        },
        { phase: "downloaded", localFiles: "missing", libraryEntryId: "entry" },
        { phase: "error", errorCode: "private-error" },
      ],
    },
    downloadsReady: true,
    downloadsFailed: false,
    preferencesReady: true,
    preferencesFailed: false,
  };
  const text = diagnosticSummary(state);
  assert.match(text, /已下载且文件存在：1/);
  assert.match(text, /需处理：2/);
  assert.match(text, /目录记录：5 · 文件待核对：4/);
  assert.doesNotMatch(text, /private-|do-not-copy|C:\//);
  assert.match(
    diagnosticSummary({
      ...state,
      accountsFailed: true,
      libraryFailed: true,
      downloadsFailed: true,
    }),
    /读取未完成/,
  );
  assert.throws(() =>
    validateWorkbenchInfo({ ...state.info, revision: "private-build-path" }),
  );
});

test("diagnostics accept stable and numbered release candidates while rejecting malformed or unsupported version labels", () => {
  for (const version of [
    "0.3.4",
    "0.0.0",
    "1.0.0",
    "1.0.0-rc.1",
    "1.20.300-rc.12",
    "1.0.0-rc.0",
  ]) {
    const expected = { version, revision: "b".repeat(40), platform: "windows" };
    assert.deepEqual(validateWorkbenchInfo(expected), expected);
  }
  for (const version of [
    "",
    "1.0",
    "v1.0.0",
    "01.0.0",
    "1.00.0",
    "1.0.00",
    "1.0.0-rc",
    "1.0.0-rc.",
    "1.0.0-rc.01",
    "1.0.0-rc.-1",
    "1.0.0-rc.1.2",
    "1.0.0-RC.1",
    "1.0.0-beta.1",
    "1.0.0-rc.1+local",
    "1.0.0/path",
    "1.0.0\n",
    "1.0.0-rc.1\r\n",
    " 1.0.0",
    "1.0.0 ",
    null,
    1,
  ]) {
    assert.throws(
      () =>
        validateWorkbenchInfo({ version, revision: null, platform: "windows" }),
      /VERSION_UNAVAILABLE/,
    );
  }
});

test("settings search finds actual controls through common words and normalized input", () => {
  assert.deepEqual(
    matchingSettingsPages("壁纸").map((x) => x.id),
    ["appearance"],
  );
  assert.deepEqual(
    matchingSettingsPages("记住 会话").map((x) => x.id),
    ["accounts"],
  );
  assert.deepEqual(
    matchingSettingsPages("记住会话").map((x) => x.id),
    ["accounts"],
  );
  assert.deepEqual(
    matchingSettingsPages("ＰＩＣＡ").map((x) => x.id),
    ["accounts"],
  );
  assert.deepEqual(
    matchingSettingsPages("版本").map((x) => x.id),
    ["network"],
  );
  assert.deepEqual(matchingSettingsPages("no-such-setting"), []);
});
