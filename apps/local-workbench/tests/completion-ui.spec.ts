import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type { DiscoverySnapshot } from "../src/completion-types.ts";
import type { DownloadInventorySnapshot } from "../src/download-types.ts";
import type { SourceWork } from "../src/source-types.ts";

// Synthetic desktop IPC only. No real source, credentials, files or downloads.
declare global {
  interface Window {
    authorTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      view: DiscoverySnapshot;
      inventory: DownloadInventorySnapshot;
      searchRecords: SourceWork[];
      hold: boolean;
      release?: () => void;
      readFailure: boolean;
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
  const accounts = (["JM", "Pica"] as const).map((source) => ({
    source,
    sessionId: "synthetic-" + source,
    accountId: "synthetic-account-" + source,
    displayName: "合成账号",
    state: "connected",
    remembered: false,
    errorCode: null,
  }));
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
        view,
        inventory,
        searchRecords: structuredClone(works),
        hold: false,
        readFailure: false,
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
            if (command === "download_inventory_read")
              return clone(hooks.inventory);
            if (command === "source_accounts") return clone(accounts);
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
            if (command === "discovery_read") {
              if (hooks.readFailure) throw { code: "SOURCE_UNAVAILABLE" };
              const result = clone(hooks.view);
              if (hooks.hold) {
                hooks.hold = false;
                await new Promise<void>((resolve) => {
                  hooks.release = resolve;
                });
              }
              return result;
            }
            if (command === "discovery_start") {
              hooks.view.run = {
                id: "scan-2",
                phase: "checking",
                currentAuthor: "合成作者",
                currentSource: "JM",
                currentPage: 1,
                requestsUsed: 1,
                completedScopes: 0,
                totalScopes: 2,
                errorCode: null,
                mode: args.mode as "incremental" | "full",
                currentStrategy: args.mode as "incremental" | "full",
              };
              for (const range of hooks.view.authors) range.state = "checking";
              return { runId: "scan-2", snapshot: clone(hooks.view) };
            }
            if (command === "discovery_cancel") {
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
