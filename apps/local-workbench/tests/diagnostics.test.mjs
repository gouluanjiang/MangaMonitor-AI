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
  assert.match(text, /文件待核对：1/);
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
    matchingSettingsPages("ＰＩＣＡ").map((x) => x.id),
    ["accounts"],
  );
  assert.deepEqual(
    matchingSettingsPages("版本").map((x) => x.id),
    ["network"],
  );
  assert.deepEqual(matchingSettingsPages("no-such-setting"), []);
});
