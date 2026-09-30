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
