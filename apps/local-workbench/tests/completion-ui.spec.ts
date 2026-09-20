import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type { DiscoverySnapshot } from "../src/completion-types.ts";
import type { DownloadInventorySnapshot } from "../src/download-types.ts";
import type { AccountSummary, SourceWork } from "../src/source-types.ts";

// Synthetic desktop IPC only. No real source, credentials, files or downloads.
declare global {
  interface Window {
    authorTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      accounts: AccountSummary[];
      view: DiscoverySnapshot;
      otherRecords: DiscoverySnapshot["records"];
      inventory: DownloadInventorySnapshot;
      searchRecords: SourceWork[];
      hold: boolean;
      release?: () => void;
      readFailure: string | null;
      cancelFailure: string | null;
      inventoryFailure: boolean;
    };
  }
}
test.use({ storageState: { cookies: [], origins: [] } });
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const captured: string[] = [];
  errors.set(page, captured);
  page.on("pageerror", (error) => captured.push(error.message));
  await page.setViewportSize({ width: 1672, height: 1020 });
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((call) =>
        /download_(prepare|confirm|control)|completeness_|source_matches_|phone_library_|source_follow$|source_favorite$/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});
async function install(page: Page) {
  const accounts: AccountSummary[] = (["JM", "Pica"] as const).map(
    (source) => ({
      source,
      sessionId: "synthetic-" + source,
      accountId: "synthetic-account-" + source,
      displayName: "合成账号",
      state: "connected",
      remembered: false,
      errorCode: null,
    }),
  );
  const works: SourceWork[] = [
    {
      source: "JM",
      workId: "123",
      title: "合成作者 · 已下载作品",
      authors: ["合成作者"],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: 1,
      pageCount: 20,
      coverAvailable: true,
    },
    {
      source: "JM",
      workId: "456",
      title: "合成作者 · 上次未选择的作品",
      authors: ["合成作者"],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: 1,
      pageCount: 20,
      coverAvailable: true,
    },
    {
      source: "Pica",
      workId: "0123456789abcdef01234567",
      title: "合成作者 · 已下载作品",
      authors: ["合成作者"],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: 1,
      pageCount: 20,
      coverAvailable: true,
    },
  ];
  const rootId = "a".repeat(64),
    libraryEntryId = "b".repeat(64);
  const library = {
    ...emptyLibrary(),
    revision: 1,
    rootId,
    rootPath: "C:\\Synthetic",
    generation: 1,
    phase: "complete",
    freshness: "live",
  };
  const view: DiscoverySnapshot = {
    scopes: accounts.map((a) => ({ source: a.source, sessionId: a.sessionId })),
    revision: 1,
    run: null,
    authors: accounts.map((a) => ({
      source: a.source,
      author: "合成作者",
      state: "partial",
      lastAttemptAt: 1800000000000,
      lastCompleteAt: null,
      observedCount: a.source === "JM" ? 2 : 1,
      pagesRead: 1,
      errorCode: "SOURCE_UNAVAILABLE",
    })),
    records: works.map((work) => ({
      work,
      matchedAuthors: ["合成作者"],
      authorVerified: true,
      observedAt: 1800000000000,
      scanId: "old-scan",
    })),
  };
  const inventory: DownloadInventorySnapshot = {
    rootId,
    revision: 1,
    libraryRevision: 1,
    items: [
      { source: "JM", workId: "123", libraryEntryId, localFiles: "present" },
    ],
  };
  await page.addInitScript(
    ({ accounts, works, library, view, inventory, preferences }) => {
      const hooks = (window.authorTest = {
        calls: [],
        accounts,
        view,
        otherRecords: [],
        inventory,
        searchRecords: structuredClone(works),
        hold: false,
        readFailure: null,
        cancelFailure: null,
        inventoryFailure: false,
      } as Window["authorTest"]);
      const clone = (value: unknown) => structuredClone(value);
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        value: {
          invoke: async (
            command: string,
            args: Record<string, unknown> = {},
          ) => {
            hooks.calls.push({
              command,
              args: clone(args) as Record<string, unknown>,
            });
            if (command === "read_preferences")
              return { revision: 0, value: preferences };
            if (command === "library_read") return clone(library);
            if (command === "download_inventory_read") {
              if (hooks.inventoryFailure) throw { code: "BUSY" };
              return clone(hooks.inventory);
            }
            if (command === "source_accounts") return clone(hooks.accounts);
            if (command === "jm_download_read")
              return { revision: 0, tasks: [] };
            if (command === "jm_download_batch_cancel") return null;
            if (command === "jm_download_batch_prepare") {
              const source = (args.scope as { source: string }).source;
              const plans = (args.inputs as string[]).map((id) => {
                const work = works.find(
                  (work) => work.source === source && work.workId === id,
                )!;
                return {
                  planId: (source === "JM" ? "c" : "d").repeat(64),
                  revision: 0,
                  source,
                  workId: id,
                  title: work.title,
                  authors: work.authors,
                  destinationDisplay: "C:\\Synthetic\\" + work.title + ".zip",
                  rootId: library.rootId,
                  generation: library.generation,
                };
              });
              return { batchId: plans[0].planId, plans, issues: [] };
            }
            if (command === "source_following")
              return {
                source: args.source,
                sessionId: args.sessionId,
                revision: 0,
                works: [],
                authors: [],
              };
            if (command === "source_cover")
              return {
                source: args.source,
                sessionId: args.sessionId,
                workId: args.workId,
                dataUrl:
                  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
              };
            if (
              command === "discovery_read" ||
              command === "discovery_progress"
            ) {
              if (hooks.readFailure) throw { code: hooks.readFailure };
              const { records, ...metadata } = hooks.view;
              const result = clone(
                command === "discovery_progress"
                  ? {
                      ...metadata,
                      recordCount: records.length + hooks.otherRecords.length,
                    }
                  : args.includeOther && hooks.view.includesOther === false
                    ? {
                        ...hooks.view,
                        records: [...records, ...hooks.otherRecords],
                        includesOther: true,
                      }
                    : hooks.view,
              );
              if (hooks.hold) {
                hooks.hold = false;
                await new Promise<void>((resolve) => {
                  hooks.release = resolve;
                });
              }
              return result;
            }
            if (
              command === "discovery_start" ||
              command === "discovery_start_unfinished"
            ) {
              const selected = hooks.view.authors.filter(
                (range) =>
                  (!(args.authors as string[]).length ||
                    (args.authors as string[]).includes(range.author)) &&
                  (command !== "discovery_start_unfinished" ||
                    range.state !== "complete"),
              );
              hooks.view.run = {
                id: "scan-2",
                phase: "checking",
                currentAuthor: "合成作者",
                currentSource: "JM",
                currentPage: 1,
                requestsUsed: 1,
                completedScopes: 0,
                totalScopes: selected.length,
                errorCode: null,
                mode: (args.mode ?? "incremental") as "incremental" | "full",
                currentStrategy: (args.mode ?? "incremental") as
                  "incremental" | "full",
              };
              for (const range of selected) range.state = "checking";
              return { runId: "scan-2", snapshot: clone(hooks.view) };
            }
            if (command === "discovery_cancel") {
              if (hooks.cancelFailure) throw { code: hooks.cancelFailure };
              hooks.view.run!.phase = "cancelled";
              for (const range of hooks.view.authors) range.state = "cancelled";
              return clone(hooks.view.run);
            }
            if (command === "source_query") {
              const sourceWorks = (
                args.kind === "detail" ? works : hooks.searchRecords
              ).filter((work) => work.source === args.source);
              const items =
                args.kind === "detail"
                  ? sourceWorks.filter((work) => work.workId === args.query)
                  : [sourceWorks[Number(args.page) - 1]].filter(Boolean);
              return {
                source: args.source,
                sessionId: args.sessionId,
                items,
                page: args.page,
                pages: args.kind === "detail" ? 1 : sourceWorks.length,
                total:
                  args.kind === "detail" ? items.length : sourceWorks.length,
                hasMore:
                  args.kind !== "detail" &&
                  Number(args.page) < sourceWorks.length,
                folders: [],
              };
            }
            throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
          },
        },
      });
    },
    {
      accounts,
      works,
      library,
      view,
      inventory,
      preferences: initialPreferences(),
    },
  );
  await page.goto("/");
}
const open = async (page: Page) => {
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 3 条",
  );
};

const discoveryCalls = (page: Page, command = "reads") =>
  page.evaluate(
    (command) =>
      window.authorTest.calls.filter((call) =>
        command === "reads"
          ? ["discovery_read", "discovery_progress"].includes(call.command)
          : call.command === command,
      ).length,
    command,
  );
const installWithPausedClock = async (page: Page) => {
  await page.clock.install({ time: new Date("2026-09-20T00:00:00Z") });
  await install(page);
  await page.clock.pauseAt(new Date("2026-09-20T01:00:00Z"));
};
const finishSyntheticCheck = (page: Page) =>
  page.evaluate(() => {
    const view = window.authorTest.view;
    if (view.run) {
      view.run.phase = "complete";
      view.run.completedScopes = view.run.totalScopes;
    }
    for (const range of view.authors) {
      range.state = "complete";
      range.errorCode = null;
      range.lastCompleteAt = Date.now();
    }
    view.revision++;
  });

test("active polling transports progress only and refreshes saved catalog once after completion", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  const fullBefore = await discoveryCalls(page, "discovery_read");
  const progressBefore = await discoveryCalls(page, "discovery_progress");
  await page.evaluate(() => {
    const h = window.authorTest;
    h.view.records[1].work.title = "合成作者 · 检查期间新读取标题";
    h.view.run!.currentPage = 10;
    h.view.revision++;
  });
  for (let i = 0; i < 3; i++) {
    await page.clock.runFor(1500);
    await expect
      .poll(() => discoveryCalls(page, "discovery_progress"))
      .toBe(progressBefore + i + 1);
  }
  expect(await discoveryCalls(page, "discovery_read")).toBe(fullBefore);
  await expect(page.getByTestId("completion-progress")).toContainText(
    "第 10 页",
  );
  await expect(page.getByTestId("completion-saved-results-note")).toContainText(
    "列表为最近读取结果",
  );
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "上次未选择的作品",
  );
  await finishSyntheticCheck(page);
  await page.clock.runFor(1500);
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "检查期间新读取标题",
  );
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  expect(await discoveryCalls(page, "discovery_read")).toBe(fullBefore + 1);
  const after = await discoveryCalls(page);
  await page.clock.runFor(30000);
  expect(await discoveryCalls(page)).toBe(after);
});

test("cold keyword results load only on request, keep the previous list while reading, and reuse the loaded catalog", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    const other = structuredClone(h.view.records[1]);
    other.work.workId = "789";
    other.work.authors = ["其他署名"];
    h.otherRecords = [other];
    h.view.includesOther = false;
    h.view.otherRecordCount = 1;
  });
  await open(page);
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 1 条",
  );
  await page.getByLabel("检查作者", { exact: true }).selectOption("合成作者");
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果按需读取",
  );
  expect(
    await page.evaluate(
      () =>
        window.authorTest.calls.filter(
          (c) => c.command === "discovery_read" && c.args.includeOther === true,
        ).length,
    ),
  ).toBe(0);
  await page.evaluate(() => {
    window.authorTest.hold = true;
  });
  await page.getByRole("button", { name: "查看其他关键词结果" }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.authorTest.release)))
    .toBe(true);
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:789")).toHaveCount(0);
  await page.evaluate(() => window.authorTest.release!());
  await expect(page.getByTestId("author-update-JM:789")).toContainText(
    "其他署名",
  );
  await expect(
    page.getByRole("button", { name: "多选", exact: true }),
  ).toHaveCount(0);
  const reads = await discoveryCalls(page);
  await page.getByRole("button", { name: "返回作者作品" }).click();
  await page.getByRole("button", { name: "查看其他关键词结果" }).click();
  expect(await discoveryCalls(page)).toBe(reads);
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
});

test("only unfinished source scopes are explicitly retried and idle scopes are not called failures", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await page.evaluate(() => {
    const [jm, pica] = window.authorTest.view.authors;
    jm.state = "complete";
    jm.lastCompleteAt = 1800000000000;
    jm.errorCode = null;
    pica.state = "idle";
    pica.pagesRead = 0;
    pica.observedCount = 0;
    pica.lastAttemptAt = null;
    pica.errorCode = null;
  });
  await open(page);
  await page.getByText("查看未完成范围", { exact: true }).click();
  await expect(page.getByTestId("completion-unfinished-ranges")).toContainText(
    "尚未开始检查",
  );
  await expect(page.getByTestId("completion-unfinished-check")).toHaveText(
    "仅补查未完成（1）",
  );
  expect(await discoveryCalls(page, "discovery_start_unfinished")).toBe(0);
  await page.getByTestId("completion-unfinished-check").click();
  await expect(page.getByTestId("completion-progress")).toContainText(
    "/ 1 个来源范围",
  );
  expect(
    await page.evaluate(() => window.authorTest.view.authors[0].state),
  ).toBe("complete");
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((c) => c.command === "discovery_start_unfinished")
        .map((c) => c.args.authors),
    ),
  ).toEqual([[]]);
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
  await finishSyntheticCheck(page);
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-unfinished-check")).toBeDisabled();
});

test("a late cold catalog response cannot restore records after an account replacement", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.view.includesOther = false;
    h.view.otherRecordCount = 1;
    const other = structuredClone(h.view.records[1]);
    other.work.workId = "789";
    other.work.authors = ["旧账号其他作者"];
    h.otherRecords = [other];
  });
  await open(page);
  await page.evaluate(() => {
    window.authorTest.hold = true;
  });
  await page.getByRole("button", { name: "查看其他关键词结果" }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.authorTest.release)))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    const h = window.authorTest;
    h.accounts[0].sessionId = "synthetic-new-account";
    h.view.scopes[0].sessionId = "synthetic-new-account";
    h.view.records = [];
    h.view.authors = [];
    h.view.otherRecordCount = 0;
    h.otherRecords = [];
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.getByTestId("nav-completion").click();
  await page.evaluate(() => window.authorTest.release!());
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 0 条",
  );
  await expect(page.getByTestId("author-update-JM:789")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "返回作者作品" })).toHaveCount(
    0,
  );
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
});

test("a deferred catalog checkpoint warning preserves successful source completion", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  await finishSyntheticCheck(page);
  await page.evaluate(() => {
    window.authorTest.view.run!.storageWarningCode =
      "DISCOVERY_CHECKPOINT_FAILED";
  });
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-storage-warning")).toContainText(
    "已读取结果已保存",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-unfinished-check")).toBeDisabled();
});

test("a busy progress read keeps results and stop control, then catches completion without restarting the check", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toContainText(
    "正在检查",
  );
  const readsBefore = await discoveryCalls(page);
  await page.evaluate(() => {
    window.authorTest.readFailure = "BUSY";
  });
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-read-error")).toContainText("BUSY");
  await expect(page.getByTestId("completion-read-error")).toContainText(
    "1.5 秒后重试读取（1 / 3）",
  );
  await expect(page.getByTestId("completion-progress")).toContainText(
    "当前进度待刷新",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "停止本次检查" }),
  ).toBeEnabled();
  await expect(page.getByTestId("completion-start")).toBeDisabled();
  await expect(
    page.getByText(
      "本次检查未完成，已读取的结果会保留。请查看检查范围后重试。",
      {
        exact: true,
      },
    ),
  ).toHaveCount(0);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-progress-temporary-read-error.png",
  });
  await page.evaluate(() => {
    window.authorTest.readFailure = null;
  });
  await finishSyntheticCheck(page);
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  await expect(page.getByTestId("completion-start")).toBeEnabled();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  expect(await discoveryCalls(page)).toBe(readsBefore + 3);
  await page.clock.runFor(30000);
  expect(await discoveryCalls(page)).toBe(readsBefore + 3);
  expect(await discoveryCalls(page, "discovery_start")).toBe(1);
});

test("repeated busy reads stop at a bounded retry budget and manual refresh resumes progress polling", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  const readsBefore = await discoveryCalls(page);
  await page.evaluate(() => {
    window.authorTest.readFailure = "BUSY";
  });
  for (const [delay, next] of [
    [1500, "1 / 3"],
    [1500, "2 / 3"],
    [3000, "3 / 3"],
    [6000, "进度刷新已暂停"],
  ] as const) {
    await page.clock.runFor(delay);
    await expect(page.getByTestId("completion-read-error")).toContainText(next);
  }
  expect(await discoveryCalls(page)).toBe(readsBefore + 4);
  await page.clock.runFor(60000);
  expect(await discoveryCalls(page)).toBe(readsBefore + 4);
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "停止本次检查" }),
  ).toBeEnabled();
  await page.evaluate(() => {
    window.authorTest.readFailure = null;
    window.authorTest.view.run!.currentPage = 42;
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-progress")).toContainText(
    "第 42 页",
  );
  await finishSyntheticCheck(page);
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  expect(await discoveryCalls(page)).toBe(readsBefore + 7);
  expect(await discoveryCalls(page, "discovery_start")).toBe(1);
});

test("an initial busy snapshot read retries without starting any source check", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await page.evaluate(() => {
    window.authorTest.readFailure = "BUSY";
  });
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-read-error")).toContainText(
    "1 / 3",
  );
  const readsBefore = await discoveryCalls(page);
  await page.evaluate(() => {
    window.authorTest.readFailure = null;
  });
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 3 条",
  );
  expect(await discoveryCalls(page)).toBe(readsBefore + 1);
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
});

test("failed cancellation keeps its action error while independent progress reads continue to completion", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  const readsBefore = await discoveryCalls(page);
  await page.evaluate(() => {
    window.authorTest.cancelFailure = "BUSY";
    window.authorTest.view.run!.currentPage = 9;
  });
  await page.getByRole("button", { name: "停止本次检查" }).click();
  const actionError = page.getByText(
    "本次检查未完成，已读取的结果会保留。请查看检查范围后重试。",
    { exact: true },
  );
  await expect(actionError).toBeVisible();
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-progress")).toContainText(
    "第 9 页",
  );
  expect(await discoveryCalls(page)).toBe(readsBefore + 1);
  await finishSyntheticCheck(page);
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "停止本次检查" })).toHaveCount(
    0,
  );
  await expect(page.getByTestId("completion-start")).toBeEnabled();
  await expect(actionError).toBeVisible();
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  expect(await discoveryCalls(page)).toBe(readsBefore + 3);
  // The terminal snapshot ends polling. A successful background read must
  // still retain the separate cancellation failure until explicit refresh.
  await page.clock.runFor(30000);
  expect(await discoveryCalls(page)).toBe(readsBefore + 3);
  await expect(actionError).toBeVisible();
  await expect(page.getByTestId("completion-counts")).not.toContainText(
    "当前检查范围已读完",
  );
  await page.evaluate(() => {
    window.authorTest.view.records[1].work.title =
      "合成作者 · 刷新后的已保存作品";
    window.authorTest.view.revision++;
  });
  const refreshButton = page.getByRole("button", {
    name: "刷新结果与入库状态",
  });
  await refreshButton.click();
  // Clearing the action error exposes the previous terminal snapshot before
  // inventory and discovery IPC resolve. Wait for the new read and its data.
  await expect.poll(() => discoveryCalls(page)).toBe(readsBefore + 4);
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "刷新后的已保存作品",
  );
  await expect(refreshButton).toBeEnabled();
  await expect(actionError).toHaveCount(0);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  expect(await discoveryCalls(page)).toBe(readsBefore + 4);
  expect(await discoveryCalls(page, "discovery_cancel")).toBe(1);
  expect(await discoveryCalls(page, "discovery_start")).toBe(1);
});

for (const code of [
  "DISCOVERY_INVALID",
  "STALE_SESSION",
  "DISCOVERY_UNAVAILABLE",
  "STORE_UNAVAILABLE",
  "STORE_READ_FAILED",
]) {
  test(`progress read ${code} waits for manual refresh instead of looping or restarting`, async ({
    page,
  }) => {
    await installWithPausedClock(page);
    await open(page);
    await page.getByTestId("completion-start").click();
    await expect(page.getByTestId("completion-progress")).toBeVisible();
    const readsBefore = await discoveryCalls(page);
    await page.evaluate((code) => {
      window.authorTest.readFailure = code;
    }, code);
    await page.clock.runFor(1500);
    await expect(page.getByTestId("completion-read-error")).toContainText(code);
    await expect(page.getByTestId("completion-read-error")).toContainText(
      "进度刷新已暂停",
    );
    await expect(page.getByTestId("completion-progress")).toContainText(
      "当前进度待刷新",
    );
    await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
    await page.clock.runFor(60000);
    expect(await discoveryCalls(page)).toBe(readsBefore + 1);
    expect(await discoveryCalls(page, "discovery_start")).toBe(1);
  });
}

test("leaving the author page cancels a pending retry and returning reads the current saved state", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  await page.evaluate(() => {
    window.authorTest.readFailure = "BUSY";
  });
  await page.clock.runFor(1500);
  await expect(page.getByTestId("completion-read-error")).toContainText(
    "1 / 3",
  );
  const readsBefore = await discoveryCalls(page);
  await page.getByTestId("nav-settings").click();
  await page.clock.runFor(60000);
  expect(await discoveryCalls(page)).toBe(readsBefore);
  await page.evaluate(() => {
    window.authorTest.readFailure = null;
  });
  await finishSyntheticCheck(page);
  await open(page);
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  expect(await discoveryCalls(page)).toBe(readsBefore + 1);
  expect(await discoveryCalls(page, "discovery_start")).toBe(1);
});

test("a late read from a replaced account cannot restore its old progress or results", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await open(page);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  const readsBefore = await discoveryCalls(page);
  await page.evaluate(() => {
    window.authorTest.hold = true;
  });
  await page.clock.runFor(1500);
  await expect
    .poll(() => page.evaluate(() => Boolean(window.authorTest.release)))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    const hooks = window.authorTest;
    hooks.accounts[0].sessionId = "synthetic-JM-replacement";
    hooks.view.scopes[0].sessionId = "synthetic-JM-replacement";
    hooks.view.run = null;
    hooks.view.authors = [];
    hooks.view.records = [];
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.getByTestId("nav-completion").click();
  // The obsolete IPC is still held: the replacement session must wait rather
  // than issue a concurrent snapshot read.
  expect(await discoveryCalls(page)).toBe(readsBefore + 1);
  await page.evaluate(() => window.authorTest.release!());
  await expect.poll(() => discoveryCalls(page)).toBe(readsBefore + 2);
  expect(
    await page.evaluate(
      () =>
        window.authorTest.calls
          .filter((call) => call.command === "discovery_read")
          .at(-1)?.args.scopes,
    ),
  ).toEqual([
    { source: "JM", sessionId: "synthetic-JM-replacement" },
    { source: "Pica", sessionId: "synthetic-Pica" },
  ]);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 0 条",
  );
  await expect(page.getByTestId("completion-read-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
  await page.clock.runFor(30000);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 0 条",
  );
  expect(await discoveryCalls(page, "discovery_start")).toBe(1);
});

test("blocked author scopes explain local skips and do not count legacy checkpoints as complete", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    for (const range of window.authorTest.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = 1800000000000;
      range.errorCode = null;
    }
    for (const [index, code] of [
      "AUTHOR_QUERY_PLACEHOLDER",
      "AUTHOR_QUERY_TOO_BROAD",
    ].entries()) {
      window.authorTest.view.authors.push({
        ...window.authorTest.view.authors[0],
        author: index ? "P" : "N/A",
        state: "partial",
        pagesRead: 0,
        observedCount: 0,
        lastCompleteAt: 1800000000000,
        errorCode: code,
      });
    }
  });
  await open(page);
  await expect(page.getByTestId("completion-catalog-scope")).toContainText(
    "已建立完整目录 2 / 4",
  );
  await page.getByText("查看未完成范围", { exact: true }).click();
  await expect(
    page.getByText(/作者名是缺失信息的占位值，本次未发送查询/),
  ).toBeVisible();
  await expect(
    page.getByText(/单个字母或数字无法限定作者范围，本次未发送查询/),
  ).toBeVisible();
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((c) => c.command === "discovery_start"),
    ),
  ).toEqual([]);
});

test("inventory contention displays an actionable unknown state and refresh restores ownership counts", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await page.evaluate(() => {
    window.authorTest.inventoryFailure = true;
  });
  await page
    .getByRole("button", { name: "刷新结果与入库状态", exact: true })
    .click();
  await expect(page.getByTestId("completion-inventory-error")).toContainText(
    "当前不能判断已入库或未入库",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "状态待核实 3 条",
  );
  await page.evaluate(() => {
    window.authorTest.inventoryFailure = false;
  });
  await page
    .getByRole("button", { name: "刷新结果与入库状态", exact: true })
    .click();
  await expect(page.getByTestId("completion-inventory-error")).toHaveCount(0);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 1 条 · 未入库 2 条 · 当前显示 2 条",
  );
});

test("author search blocks an initial before source requests and preserves the normal search entry", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-author-search").click();
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("P");
  await page.getByTestId("completion-start").click();
  await expect(page.getByRole("alert")).toContainText("未发送查询");
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((c) => c.command === "source_query"),
    ),
  ).toEqual([]);
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("合成作者");
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
});

test("author update dates sort only loaded records, compose with ownership and persist independently of author search", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const dates = ["2026-09-01", "2026-09-20", null];
    window.authorTest.view.records.forEach((record, index) => {
      record.work.sourceUpdatedAt = dates[index];
    });
    window.authorTest.searchRecords.forEach((work, index) => {
      work.sourceUpdatedAt = dates[index];
    });
  });
  await open(page);
  const cards = page.getByTestId("completion-panel").locator("article");
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-desc");
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-JM:456",
  );
  await expect(cards.last()).toContainText("更新时间未知");
  await expect(page.getByTestId("completion-date-sort-scope")).toContainText(
    "排序仅覆盖已读取结果",
  );
  await page.getByRole("button", { name: "全部 3", exact: true }).click();
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-JM:123",
  );
  await page.getByLabel("筛选作者更新").fill("上次未选择");
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText("更新：2026-09-20");
  await page.getByLabel("筛选作者更新").fill("");
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-update-work-dates-wide.png",
  });
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-desc");
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("合成作者");
  await page.getByRole("button", { name: "搜索两站作品" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-JM:456",
  );
  await expect(
    page.getByTestId("completion-date-sort-scope"),
  ).not.toContainText("尚未读完");
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({
    path: "visual-evidence/author-search-work-dates-compact.png",
  });
  await page.reload();
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-asc");
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
});

test("saved omissions remain visible, same-source receipts filter ownership, and entering the page never starts a check", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 1 条 · 未入库 2 条 · 当前显示 2 条",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(
    page.getByTestId("author-update-Pica:0123456789abcdef01234567"),
  ).toBeVisible();
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter(
        (call) => call.command === "discovery_start",
      ),
    ),
  ).toEqual([]);
  await page.getByTestId("nav-settings").click();
  await open(page);
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/manual-author-updates.png" });
});

test("checking and stopping are explicit, preserve old results and do not download", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await expect(page.getByTestId("completion-start")).toHaveText(
    "一键检查全部关注作者",
  );
  await page.getByRole("button", { name: "全部 3", exact: true }).click();
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toContainText(
    "正在检查",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:123")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "discovery_start")
        .map((call) => [call.args.authors, call.args.mode]),
    ),
  ).toEqual([[[], "incremental"]]);
  await expect(page.getByTestId("completion-full-check")).toBeDisabled();
  await page.getByRole("button", { name: "停止本次检查" }).click();
  await expect(page.getByTestId("completion-progress")).toHaveCount(0);
  await expect(
    page.getByText("检查范围尚未读完", { exact: false }),
  ).toBeVisible();
  await page.getByLabel("检查作者", { exact: true }).selectOption("合成作者");
  await page.getByTestId("completion-full-check").click();
  await expect(page.getByTestId("completion-progress")).toContainText(
    "本范围读取完整目录",
  );
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "discovery_start")
        .map((call) => [call.args.authors, call.args.mode]),
    ),
  ).toEqual([
    [[], "incremental"],
    [["合成作者"], "full"],
  ]);
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
});

test("incremental completion retains old omissions and never claims a new full catalog check", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    for (const range of window.authorTest.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = 1800000000000;
      range.lastCheckedAt = 1800000001000;
      range.lastCheckMode = "incremental";
      range.errorCode = null;
    }
  });
  await open(page);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "本轮检查已完成（含增量），历史目录已保留",
  );
  await expect(page.getByTestId("completion-counts")).not.toContainText(
    "当前检查范围已读完",
  );
  await expect(page.getByTestId("completion-catalog-scope")).toContainText(
    "已建立完整目录 2 / 2 个来源范围",
  );
  await expect(page.getByTestId("completion-catalog-scope")).toContainText(
    "没有重新读取所有历史分页",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:123")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter(
        (call) => call.command === "source_cover" && call.args.workId === "123",
      ),
    ),
  ).toEqual([]);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 2 本",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/incremental-author-updates.png",
  });
  await page.evaluate(() => {
    const h = window.authorTest;
    h.inventory.items = h.view.records.map((r) => ({
      source: r.work.source,
      workId: r.work.workId,
      libraryEntryId: "b".repeat(64),
      localFiles: "present",
    }));
    h.inventory.revision++;
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 3 条 · 未入库 0 条 · 当前显示 0 条",
  );
  await expect(page.getByTestId("completion-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await page.evaluate(() => {
    for (const range of window.authorTest.view.authors) {
      range.lastCheckMode = "full";
      range.lastCompleteAt = 1800000002000;
      range.lastCheckedAt = 1800000002000;
    }
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-all-owned")).toBeVisible();
});

test("explicit circle membership remains selectable without literal equality and empty complete queries are explicit", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const record = window.authorTest.view.records[1];
    record.work.authors = ["合成社团 (合成作者)"];
    record.authorVerified = false;
  });
  await open(page);
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(page.getByTestId("completion-query-scope")).toContainText(
    "作者关键词",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "未入库 2 条",
  );
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.authorTest.view.records = [];
    for (const range of window.authorTest.view.authors) {
      range.state = "complete";
      range.observedCount = 0;
      range.errorCode = null;
      range.lastCompleteAt = Date.now();
    }
  });
  await page.getByTestId("nav-completion").click();
  await expect(
    page.getByText("本次完整查询没有返回作品。请核对作者名称或切换来源查看。", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
});

test("saved unrelated and missing author fields stay outside author totals, ownership completion and selection", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.view.records[1].work.authors = ["另一个作者"];
    h.view.records[2].work.authors = [];
    for (const range of h.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = Date.now();
      range.errorCode = null;
    }
  });
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已记录 1 条 · 已入库 1 条 · 未入库 0 条",
  );
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 2 条",
  );
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await page.getByRole("button", { name: "查看其他关键词结果" }).click();
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "另一个作者",
  );
  await expect(
    page.getByTestId("author-update-Pica:0123456789abcdef01234567"),
  ).toContainText("作者信息未提供");
  await expect(page.getByTestId("completion-counts")).toContainText(
    "其他关键词结果（未确认作者归属）",
  );
  await expect(
    page.getByRole("button", { name: "多选", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "下载到漫画库", exact: true }),
  ).toHaveCount(0);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-other-keyword-results.png",
  });
  await page.getByRole("button", { name: "返回作者作品" }).click();
  await expect(page.getByTestId("author-update-JM:123")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
});

test("full author selection includes circle members but never other keyword hits", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.view.records[1].work.authors = ["合成社团 (合成作者)"];
    h.view.records[1].authorVerified = false;
    const other = structuredClone(h.view.records[1]);
    other.work.workId = "789";
    other.work.authors = ["合成作者二号"];
    h.view.records.push(other);
    for (const range of h.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = Date.now();
      range.errorCode = null;
    }
  });
  await open(page);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 2 本",
  );
  await expect(page.getByTestId("author-update-JM:789")).toHaveCount(0);
  await page.getByRole("button", { name: "查看下载计划", exact: true }).click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  const inputs = await page.evaluate(() =>
    window.authorTest.calls
      .filter((call) => call.command === "jm_download_batch_prepare")
      .flatMap((call) => call.args.inputs as string[]),
  );
  expect(inputs.sort()).toEqual(["456", "0123456789abcdef01234567"].sort());
});

test("ad-hoc author search classifies every source page, retaining unrelated results for inspection", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const works = window.authorTest.searchRecords;
    works[0].authors = ["新社团（新作者）"];
    works[1].authors = ["新作者二号"];
    works[2].authors = [];
  });
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByTestId("completion-full-check")).toHaveCount(0);
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("新作者");
  await page.getByRole("button", { name: "搜索两站作品" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 1 条",
  );
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 2 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "source_query")
        .map((call) => [call.args.source, call.args.page]),
    ),
  ).toEqual([
    ["JM", 1],
    ["JM", 2],
    ["Pica", 1],
  ]);
  await page.getByRole("button", { name: "全部 1", exact: true }).click();
  await expect(page.getByTestId("author-update-JM:123")).toContainText(
    "新社团（新作者）",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-confirmed-results.png",
  });
});

test("narrowing the title filter to an owned work does not claim the author's missing works are complete", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    for (const range of window.authorTest.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = Date.now();
    }
  });
  await open(page);
  await page.getByLabel("更新来源").selectOption("JM");
  await page.getByLabel("筛选作者更新").fill("已下载作品");
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 1 条 · 未入库 0 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
});

test("valid new receipts refresh counts, but all-owned is withheld until both source ranges finish", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.inventory.items = h.view.records.map((r) => ({
      source: r.work.source,
      workId: r.work.workId,
      libraryEntryId: "b".repeat(64),
      localFiles: "present",
    }));
    h.inventory.revision++;
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 3 条 · 未入库 0 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await page.evaluate(() => {
    for (const range of window.authorTest.view.authors) {
      range.state = "complete";
      range.errorCode = null;
      range.lastCompleteAt = 1800000001000;
    }
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-all-owned")).toContainText(
    "JM 与哔咔",
  );
  await page.evaluate(() => {
    window.authorTest.inventory.items[0].localFiles = "missing";
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByTestId("completion-counts")).toContainText(
    "未入库 1 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
});

test("a new author is searched across every page of both sources without requiring a follow", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    for (const work of window.authorTest.searchRecords)
      work.authors = ["新作者"];
  });
  await page.getByTestId("nav-author-search").click();
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("新作者");
  await page.getByRole("button", { name: "搜索两站作品" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 3 条",
  );
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((c) => c.command === "source_query")
        .map((c) => [c.args.source, c.args.page]),
    ),
  ).toEqual([
    ["JM", 1],
    ["JM", 2],
    ["Pica", 1],
  ]);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 1 条 · 未入库 2 条 · 当前显示 2 条",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/dual-source-author-search.png",
  });
  await page
    .getByTestId("author-update-JM:456")
    .getByRole("button")
    .first()
    .click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toHaveValue(
    "新作者",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 3 条",
  );
  expect(
    await page.evaluate(
      () =>
        window.authorTest.calls.filter(
          (call) =>
            call.command === "source_query" && call.args.kind === "search",
        ).length,
    ),
  ).toBe(3);
});

test("full-range author selection waits for completion, excludes owned works and reviews both sources together", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await expect(page.getByTestId("completion-select-all")).toBeDisabled();
  await page.evaluate(() => {
    window.authorTest.view.authors = window.authorTest.view.authors.map(
      (range) => ({
        ...range,
        state: "complete",
        errorCode: null,
        lastCompleteAt: 1800000000000,
      }),
    );
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 2 本",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/author-full-selection.png" });
  await page
    .getByTestId("completion-selection-bar")
    .getByRole("button", { name: "查看下载计划" })
    .click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "jm_download_batch_prepare")
        .map((call) => [
          (call.args.scope as { source: string }).source,
          call.args.inputs,
        ]),
    ),
  ).toEqual([
    ["JM", ["456"]],
    ["Pica", ["0123456789abcdef01234567"]],
  ]);
  await page.screenshot({
    path: "visual-evidence/mixed-source-download-selection.png",
  });
  await page.getByTestId("download-batch-cancel").click();
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((call) =>
        /download_(selection_confirm|batch_confirm|confirm)$/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
  await page.getByRole("button", { name: "全部 3", exact: true }).click();
  await expect(page.getByTestId("completion-selection-bar")).toHaveCount(0);
});
