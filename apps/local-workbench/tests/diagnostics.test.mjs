import test from "node:test";
import assert from "node:assert/strict";
import {
  diagnosticSummary,
  validateWorkbenchInfo,
} from "../src/diagnostics.ts";
import { matchingSettingsPages } from "../src/settings-navigation.ts";
import { emptyLibrary } from "../src/library-types.ts";

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
