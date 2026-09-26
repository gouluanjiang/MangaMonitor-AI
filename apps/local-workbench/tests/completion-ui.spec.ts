import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type { DiscoverySnapshot } from "../src/completion-types.ts";
import type { DownloadInventorySnapshot } from "../src/download-types.ts";
import type {
  AccountSummary,
  SourceWork,
  AuthorQueryPolicy,
} from "../src/source-types.ts";

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
      detailRecords: SourceWork[];
      authorPolicies: AuthorQueryPolicy[];
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
        detailRecords: structuredClone(works),
        authorPolicies: [],
        hold: false,
        readFailure: null,
        cancelFailure: null,
        inventoryFailure: false,
      } as Window["authorTest"]);
      // Opt-in saved IPC state for reload coverage, never a real account store.
      const savedSummary = sessionStorage.getItem("synthetic-author-summary");
      if (savedSummary) {
        const saved = JSON.parse(savedSummary);
        hooks.view = saved.view;
        hooks.inventory = saved.inventory;
      }
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
            if (command === "source_author_policy") {
              const policy = hooks.authorPolicies.find(
                (item) =>
                  item.source === args.source && item.author === args.author,
              );
              return {
                source: args.source,
                sessionId: args.sessionId,
                revision: 0,
                author: args.author,
                queries: [args.author],
                verifiedAliases: [],
                exactCredits: [],
                queryFingerprint: "a".repeat(64),
                ...(policy ? structuredClone(policy) : {}),
              };
            }
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
                args.kind === "detail"
                  ? hooks.detailRecords
                  : hooks.searchRecords
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

const seedChangeSummary = (page: Page) =>
  page.evaluate(() => {
    const h = window.authorTest;
    const id = "a".repeat(64);
    const at = 1800000001000;
    h.view.lastCheck = {
      id,
      startedAt: at,
      finishedAt: at + 1000,
      phase: "complete",
      mode: "incremental",
      onlyUnfinished: false,
      firstCatalog: false,
      allFollowed: true,
      authorCount: 2,
      totalScopes: 3,
      attemptedScopes: 3,
      completeScopes: 3,
    };
    // Re-reading an old work in this run does not make it newly discovered.
    for (const record of h.view.records) {
      record.scanId = id;
      record.observedAt = at;
    }
    const add = (
      workId: string,
      title: string,
      author = "合成作者",
      source: "JM" | "Pica" = "JM",
    ) => {
      const record = structuredClone(h.view.records[1]);
      record.work = {
        ...record.work,
        source,
        workId,
        title,
        authors: [author],
      };
      record.matchedAuthors = [author];
      record.firstDiscoveredRunId = id;
      h.view.records.push(record);
      return record;
    };
    add("789", "合成作者 · 本次首次发现未入库作品");
    add("890", "合成作者 · 本次首次发现已入库作品");
    add(
      "1123456789abcdef01234567",
      "合成作者 · 本次首次发现待核实作品",
      "合成作者",
      "Pica",
    );
    const other = add("901", "无关关键词结果", "其他署名");
    other.matchedAuthors = ["合成作者"];
    other.authorVerified = false;
    add("902", "另一作者 · 本次首次发现作品", "另一作者");
    h.view.authors.push({ ...h.view.authors[0], author: "另一作者" });
    for (const range of h.view.authors) {
      range.state = "complete";
      range.errorCode = null;
      range.lastCompleteAt = at + 1000;
    }
    h.inventory.items.push(
      {
        source: "JM",
        workId: "890",
        libraryEntryId: "b".repeat(64),
        localFiles: "present",
      },
      {
        source: "Pica",
        workId: "1123456789abcdef01234567",
        libraryEntryId: "c".repeat(64),
        localFiles: "unavailable",
      },
    );
    h.view.revision++;
  });

test("change summary uses first discovery identities, keeps historical omissions and follows current file registration", async ({
  page,
}) => {
  await install(page);
  await seedChangeSummary(page);
  await page.getByTestId("nav-completion").click();
  const summary = page.getByTestId("completion-change-summary");
  const counts = page.getByTestId("completion-change-counts");
  await expect(counts).toContainText("本次首次发现 4 条");
  await expect(counts).toContainText(
    "未入库 2 条 · 已入库 1 条 · 状态待核实 1 条",
  );
  await expect(counts).toContainText("历史保留未入库 2 条");
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:901")).toHaveCount(0);
  await summary.scrollIntoViewIfNeeded();
  await expect(summary).toBeInViewport();
  await expect(page.getByTestId("author-update-JM:789")).toBeInViewport();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-change-summary-complete.png",
  });
  await page.getByTestId("completion-new-only").click();
  await expect(page.getByTestId("completion-new-only")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("author-update-JM:789")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.inventory.items.push({
      source: "JM",
      workId: "789",
      libraryEntryId: "d".repeat(64),
      localFiles: "present",
    });
    h.inventory.revision++;
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(counts).toContainText("本次首次发现 4 条");
  await expect(counts).toContainText(
    "未入库 1 条 · 已入库 2 条 · 状态待核实 1 条",
  );
  await expect(page.getByTestId("author-update-JM:789")).toHaveCount(0);
  await page.getByTestId("completion-new-only").click();
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
  expect(await discoveryCalls(page, "discovery_start_unfinished")).toBe(0);
});

test("change summary filters compose with author and source scopes, clear selections and exclude other keyword hits", async ({
  page,
}) => {
  await install(page);
  await seedChangeSummary(page);
  await page.getByTestId("nav-completion").click();
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 4 本",
  );
  await page.getByTestId("completion-new-only").click();
  await expect(page.getByTestId("completion-selection-bar")).toHaveCount(0);
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 2 本",
  );
  await page.getByLabel("检查作者", { exact: true }).selectOption("合成作者");
  await expect(page.getByTestId("completion-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 3 条",
  );
  await page.getByLabel("更新来源").selectOption("JM");
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 2 条",
  );
  await expect(page.getByTestId("author-update-JM:902")).toHaveCount(0);
  await page.getByLabel("筛选作者更新").fill("未入库作品");
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 1 条",
  );
  await page.getByLabel("筛选作者更新").fill("");
  await page.getByRole("button", { name: "查看其他关键词结果" }).click();
  await expect(page.getByTestId("completion-change-summary")).toHaveCount(0);
  await expect(page.getByTestId("author-update-JM:901")).toBeVisible();
  await expect(page.getByTestId("completion-select-all")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "查看下载计划", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "返回作者作品" }).click();
  await expect(page.getByTestId("completion-new-only")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
});

test("running summaries withhold final counts and partial or interrupted checks report only their read scope", async ({
  page,
}) => {
  await installWithPausedClock(page);
  await seedChangeSummary(page);
  await page.evaluate(() => {
    const view = window.authorTest.view;
    const summary = view.lastCheck!;
    summary.phase = "checking";
    summary.finishedAt = null;
    summary.completeScopes = 1;
    summary.attemptedScopes = 2;
    view.run = {
      id: summary.id,
      phase: "checking",
      currentAuthor: "合成作者",
      currentSource: "Pica",
      currentPage: 2,
      requestsUsed: 2,
      completedScopes: 1,
      totalScopes: 3,
      errorCode: null,
      mode: "incremental",
      currentStrategy: "incremental",
    };
  });
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-change-summary")).toContainText(
    "正在检查，结束后汇总本次新发现",
  );
  await expect(page.getByTestId("completion-change-counts")).toHaveCount(0);
  await expect(page.getByTestId("completion-new-only")).toBeDisabled();
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  await page.evaluate(() => {
    const view = window.authorTest.view;
    view.run!.phase = "partial";
    view.run!.completedScopes = 2;
    view.lastCheck!.phase = "partial";
    view.lastCheck!.finishedAt = 1800000003000;
    view.lastCheck!.attemptedScopes = 3;
    view.lastCheck!.completeScopes = 2;
    view.authors[1].state = "partial";
    view.authors[1].errorCode = "SOURCE_UNAVAILABLE";
    view.revision++;
  });
  await page.clock.runFor(1500);
  const summary = page.getByTestId("completion-change-summary");
  await expect(summary).toContainText(
    "仅统计本批已读取范围，未完成范围仍需补查",
  );
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 4 条",
  );
  await expect(page.getByTestId("completion-new-only")).toBeEnabled();
  await summary.scrollIntoViewIfNeeded();
  await expect(summary).toBeInViewport();
  await expect(page.getByTestId("author-update-JM:789")).toBeInViewport();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-change-summary-partial.png",
  });
  await page.evaluate(() => {
    window.authorTest.view.run = null;
    window.authorTest.view.lastCheck!.phase = "interrupted";
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(summary).toContainText(
    "仅统计本批已读取范围，未完成范围仍需补查",
  );
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 4 条",
  );
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
});

test("legacy catalogs get no invented summary, first collection is explicit and saved summaries survive reload outside author search", async ({
  page,
}) => {
  await install(page);
  await open(page);
  await expect(page.getByTestId("completion-change-summary")).toContainText(
    "下一次检查后生成变化摘要",
  );
  await expect(page.getByTestId("completion-change-counts")).toHaveCount(0);
  await expect(page.getByTestId("completion-new-only")).toBeDisabled();
  await seedChangeSummary(page);
  await page.evaluate(() => {
    const view = window.authorTest.view;
    view.lastCheck!.firstCatalog = true;
    for (const record of view.records)
      record.firstDiscoveredRunId = view.lastCheck!.id;
  });
  await page.getByRole("button", { name: "刷新结果与入库状态" }).click();
  await expect(page.getByTestId("completion-change-summary")).toContainText(
    "首次收录不代表网站新发布",
  );
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 7 条",
  );
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "历史保留未入库 0 条",
  );
  await page.evaluate(() => {
    const { view, inventory } = window.authorTest;
    sessionStorage.setItem(
      "synthetic-author-summary",
      JSON.stringify({ view, inventory }),
    );
  });
  await page.reload();
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-change-summary")).toContainText(
    "首次收录不代表网站新发布",
  );
  await expect(page.getByTestId("completion-change-counts")).toContainText(
    "本次首次发现 7 条",
  );
  expect(await discoveryCalls(page, "discovery_start")).toBe(0);
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByTestId("completion-change-summary")).toHaveCount(0);
  await expect(page.getByTestId("completion-new-only")).toHaveCount(0);
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

test("legacy author results explain that missing dates cannot reverse catalog order without starting a source check", async ({
  page,
}) => {
  await install(page);
  await open(page);
  const cards = page.getByTestId("completion-panel").locator("article");
  const order = () =>
    cards.evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute("data-testid")),
    );
  await expect(cards).toHaveCount(2);
  const before = await order();
  await expect(page.getByTestId("completion-date-coverage")).toHaveText(
    "当前显示作品：有更新时间 0 条 · 更新时间未知 2 条。",
  );
  await expect(page.getByTestId("completion-date-sort-scope")).toContainText(
    "切换时间正倒序不会改变顺序",
  );
  await expect(page.getByTestId("completion-date-sort-scope")).toContainText(
    "排序仅覆盖已读取结果",
  );
  await expect(page.getByTestId("completion-date-refresh-help")).toContainText(
    "选择一位作者",
  );
  await expect(page.getByTestId("completion-date-refresh-help")).toContainText(
    "仅读取本机记录，不会补查网站日期",
  );
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  expect(await order()).toEqual(before);
  await page.getByTestId("completion-sort").selectOption("source");
  await expect(page.getByTestId("completion-date-sort-scope")).toHaveCount(0);
  await expect(page.getByTestId("completion-date-coverage")).toContainText(
    "更新时间未知 2 条",
  );
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((call) =>
        /^(source_query|discovery_start|discovery_start_unfinished)$/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  await page.screenshot({
    path: "visual-evidence/author-update-missing-dates.png",
  });
});

test("date coverage follows visible filters and saved date arrivals immediately re-sort without changing the chosen direction", async ({
  page,
}) => {
  await install(page);
  await open(page);
  const cards = page.getByTestId("completion-panel").locator("article");
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  await page.evaluate(() => {
    window.authorTest.view.records[2].work.sourceUpdatedAt = "2026-09-01";
    window.authorTest.view.revision++;
  });
  await page
    .getByRole("button", { name: "刷新结果与入库状态", exact: true })
    .click();
  await expect(page.getByTestId("completion-date-coverage")).toHaveText(
    "当前显示作品：有更新时间 1 条 · 更新时间未知 1 条。",
  );
  await expect(page.getByTestId("completion-date-sort-scope")).toContainText(
    "仅 1 条按网站更新时间排序，其余 1 条日期未知，排列在最后",
  );
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-Pica:0123456789abcdef01234567",
  );
  await page.getByLabel("更新来源").selectOption("JM");
  await expect(page.getByTestId("completion-date-coverage")).toHaveText(
    "当前显示作品：有更新时间 0 条 · 更新时间未知 1 条。",
  );
  await page.getByLabel("更新来源").selectOption("all");
  await page.evaluate(() => {
    window.authorTest.view.records[1].work.sourceUpdatedAt = "2026-09-20";
    window.authorTest.view.revision++;
  });
  await page
    .getByRole("button", { name: "刷新结果与入库状态", exact: true })
    .click();
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-asc");
  await expect(page.getByTestId("completion-date-coverage")).toHaveText(
    "当前显示作品：有更新时间 2 条 · 更新时间未知 0 条。",
  );
  await expect(page.getByTestId("completion-date-refresh-help")).toHaveCount(0);
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-Pica:0123456789abcdef01234567",
  );
  await page.getByTestId("completion-sort").selectOption("updated-desc");
  await expect(cards.first()).toHaveAttribute(
    "data-testid",
    "author-update-JM:456",
  );
  await expect(cards.first()).toContainText("更新：2026-09-20");
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((call) =>
        /^(source_query|discovery_start|discovery_start_unfinished)$/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
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

for (const entry of ["saved updates", "author search"] as const) {
  test(`${entry} shows the same language evidence without detail sweeps and enriches only an opened work`, async ({
    page,
  }) => {
    await install(page);
    await page.evaluate(() => {
      const tags = [["中文"], ["日本語"], ["日漫", "合成汉化组"]];
      window.authorTest.view.records.forEach((record, index) => {
        record.work.tags = tags[index];
      });
      window.authorTest.searchRecords.forEach((work, index) => {
        work.tags = tags[index];
      });
      window.authorTest.detailRecords[2].tags = ["中文"];
    });
    if (entry === "saved updates") {
      await open(page);
      expect(
        await page.evaluate(() =>
          window.authorTest.calls.filter((call) =>
            /^(source_query|discovery_start|discovery_start_unfinished)$/.test(
              call.command,
            ),
          ),
        ),
      ).toEqual([]);
    } else {
      await page.getByTestId("nav-author-search").click();
      await page.getByRole("textbox", { name: "搜索作者名" }).fill("合成作者");
      await page.getByRole("button", { name: "搜索两站作品" }).click();
      await expect(page.getByTestId("completion-counts")).toContainText(
        "当前检查范围已读完",
      );
    }
    await page.getByRole("button", { name: "全部 3", exact: true }).click();
    for (const [key, label] of [
      ["JM:123", "已汉化"],
      ["JM:456", "生肉"],
      ["Pica:0123456789abcdef01234567", "未知"],
    ]) {
      const badge = page
        .getByTestId("author-update-" + key)
        .getByTestId("source-language-badge");
      await expect(badge).toHaveText(label);
      await expect(badge).toHaveAttribute("data-language-context", "source");
    }
    expect(
      await page.evaluate(() =>
        window.authorTest.calls.filter(
          (call) =>
            call.command === "source_query" && call.args.kind === "detail",
        ),
      ),
    ).toEqual([]);
    await page.getByRole("button", { name: "多选", exact: true }).click();
    const japanese = page.getByTestId("author-update-JM:456");
    await japanese.getByRole("checkbox").check();
    await expect(page.getByTestId("completion-selection-bar")).toContainText(
      "已选 1 本",
    );
    await japanese.scrollIntoViewIfNeeded();
    for (const key of ["JM:123", "JM:456", "Pica:0123456789abcdef01234567"]) {
      await expect(
        page
          .getByTestId("author-update-" + key)
          .getByTestId("source-language-badge"),
      ).toBeInViewport({ ratio: 1 });
    }
    await expect(japanese.getByRole("checkbox")).toBeInViewport({ ratio: 1 });
    await mkdir("visual-evidence", { recursive: true });
    await page.screenshot({
      path: `visual-evidence/language-${entry === "saved updates" ? "author-updates" : "author-search"}.png`,
    });
    await page.getByRole("button", { name: "退出多选", exact: true }).click();
    const unknown = page.getByTestId(
      "author-update-Pica:0123456789abcdef01234567",
    );
    await unknown.locator(".source-card-open").click();
    await page
      .getByTestId("reader-cover-actions")
      .getByRole("button", { name: "作品详情", exact: true })
      .click();
    await expect(
      page.getByTestId("source-detail").getByTestId("source-language-badge"),
    ).toHaveText("已汉化");
    await page.getByTestId("source-detail-back").click();
    await expect(unknown.getByTestId("source-language-badge")).toHaveText(
      "已汉化",
    );
    await expect(japanese.getByTestId("source-language-badge")).toHaveText(
      "生肉",
    );
    expect(
      await page.evaluate(() =>
        window.authorTest.calls
          .filter(
            (call) =>
              call.command === "source_query" && call.args.kind === "detail",
          )
          .map((call) => [call.args.source, call.args.query]),
      ),
    ).toEqual([["Pica", "0123456789abcdef01234567"]]);
    expect(await discoveryCalls(page, "discovery_start")).toBe(0);
  });
}

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
  await expect(page.getByTestId("completion-counts")).toContainText(
    "检查范围尚未读完",
  );
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

test("a metadata placeholder stays inspectable outside complete author counts and bulk downloads", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const hooks = window.authorTest;
    const placeholder = structuredClone(hooks.view.records[1]);
    placeholder.work = {
      ...placeholder.work,
      workId: "789",
      title: "来源作品信息缺失（JM789）",
      authors: [],
      description: null,
      tags: [],
      pageCount: null,
      chapterCount: null,
      coverAvailable: false,
    };
    // Even a legacy verified bit cannot invent the missing author credit.
    placeholder.authorVerified = true;
    hooks.view.records.push(placeholder);
    for (const range of hooks.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = Date.now();
      range.errorCode = null;
    }
  });
  await open(page);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 3 条",
  );
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 1 条",
  );
  await expect(page.getByTestId("author-update-JM:789")).toHaveCount(0);
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByTestId("completion-select-all").click();
  await expect(page.getByTestId("completion-selection-bar")).toContainText(
    "已选 2 本",
  );
  await page.getByRole("button", { name: "查看下载计划", exact: true }).click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "jm_download_batch_prepare")
        .flatMap((call) => call.args.inputs as string[])
        .sort(),
    ),
  ).toEqual(["456", "0123456789abcdef01234567"].sort());
});

test("a placeholder prevents an all-owned author claim and has no download controls in other results", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const hooks = window.authorTest;
    hooks.view.records[1].work = {
      ...hooks.view.records[1].work,
      title: "来源作品信息缺失（JM456）",
      authors: [],
      coverAvailable: false,
    };
    hooks.inventory.items = hooks.view.records
      .filter((record) => record.work.authors.length)
      .map((record) => ({
        source: record.work.source,
        workId: record.work.workId,
        libraryEntryId: "b".repeat(64),
        localFiles: "present",
      }));
    for (const range of hooks.view.authors) {
      range.state = "complete";
      range.lastCompleteAt = Date.now();
      range.errorCode = null;
    }
  });
  await page.getByTestId("nav-completion").click();
  await page
    .getByRole("button", { name: "刷新结果与入库状态", exact: true })
    .click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 2 条 · 已入库 2 条 · 未入库 0 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await page
    .getByRole("button", { name: "查看其他关键词结果", exact: true })
    .click();
  const placeholder = page.getByTestId("author-update-JM:456");
  await expect(placeholder).toContainText("来源作品信息缺失（JM456）");
  await expect(placeholder).toContainText("作者信息未提供");
  await expect(placeholder.getByRole("checkbox")).toHaveCount(0);
  await expect(
    placeholder.getByRole("button", { name: "下载到漫画库", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByTestId("completion-select-all")).toHaveCount(0);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "其他关键词结果（未确认作者归属）",
  );
});

test("saved author policies reclassify cached other results without source IO and stay source scoped", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const hooks = window.authorTest;
    hooks.view.records[0].work.authors = ["ReviewedAlias"];
    hooks.view.records[1].work.authors = ["DifferentWriter"];
    hooks.view.records[2].work.authors = ["ReviewedAlias"];
    hooks.view.authorPolicies = [
      {
        source: "JM",
        author: "合成作者",
        queries: ["合成作者"],
        verifiedAliases: ["ReviewedAlias"],
        exactCredits: [],
        queryFingerprint: "a".repeat(64),
      },
    ];
  });
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "作者作品 1 条",
  );
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 2 条",
  );
  await page.getByRole("button", { name: "全部 1", exact: true }).click();
  await expect(page.getByTestId("author-update-JM:123")).toBeVisible();
  await expect(
    page.getByTestId("author-update-Pica:0123456789abcdef01234567"),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "查看其他关键词结果", exact: true })
    .click();
  await expect(page.getByTestId("author-update-JM:456")).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.authorTest.calls.filter((call) =>
        /^(source_query|source_author_policy|discovery_start|discovery_start_unfinished)$/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
  expect(await page.evaluate(() => window.authorTest.view.records.length)).toBe(
    3,
  );
});

for (const mode of ["updates", "search"] as const) {
  test(`${mode} reviewed work credits change attribution, counts and selection while retaining raw history and explaining details`, async ({
    page,
  }) => {
    await install(page);
    await page.evaluate((mode) => {
      const h = window.authorTest;
      const author = mode === "updates" ? "合成作者" : "新作者";
      const policy: AuthorQueryPolicy = {
        source: "JM",
        author,
        queries: [author],
        verifiedAliases: [],
        queryFingerprint: "a".repeat(64),
        workCredits: [
          {
            workId: "123",
            expectedAuthors: ["Wrong Credit"],
            correctedAuthors: [author],
          },
          {
            workId: "456",
            expectedAuthors: [author],
            correctedAuthors: ["Other Writer"],
          },
        ],
      };
      h.authorPolicies = [policy];
      h.view.authorPolicies = [policy];
      h.searchRecords[0].authors = ["Wrong Credit"];
      h.searchRecords[1].authors = [author];
      h.searchRecords[2].authors = [author];
      h.detailRecords = structuredClone(h.searchRecords);
      h.view.records = h.view.records.map((record, index) => ({
        ...record,
        work: structuredClone(h.searchRecords[index]),
      }));
      for (const range of h.view.authors) {
        range.state = "complete";
        range.lastCompleteAt = Date.now();
        range.errorCode = null;
      }
    }, mode);
    const rawBefore = await page.evaluate(() => ({
      records: window.authorTest.view.records,
      inventory: window.authorTest.inventory,
    }));
    if (mode === "updates") await page.getByTestId("nav-completion").click();
    else {
      await page.getByTestId("nav-author-search").click();
      await page.getByRole("textbox", { name: "搜索作者名" }).fill("新作者");
      await page.getByRole("button", { name: "搜索两站作品" }).click();
    }
    await expect(page.getByTestId("completion-counts")).toContainText(
      "当前检查范围已读完 · 已记录 2 条 · 已入库 1 条 · 未入库 1 条",
    );
    await expect(page.getByTestId("completion-other-results")).toContainText(
      "其他关键词结果 1 条",
    );
    await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
    await page.getByRole("button", { name: "多选", exact: true }).click();
    await page.getByTestId("completion-select-all").click();
    await expect(page.getByTestId("completion-selection-bar")).toContainText(
      "已选 1 本",
    );
    await page.getByRole("button", { name: "全部 2", exact: true }).click();
    const correct = page.getByTestId("author-update-JM:123");
    await expect(correct.getByTestId("author-credit-reviewed")).toHaveAttribute(
      "title",
      "已按本作品核对署名。来源原署名：Wrong Credit",
    );
    await correct.locator(".source-card-open").click();
    await page
      .getByTestId("reader-cover-actions")
      .getByRole("button", { name: "作品详情", exact: true })
      .click();
    const detail = page.getByTestId("source-detail");
    await expect(detail.getByTestId("author-credit-reviewed")).toHaveAttribute(
      "title",
      "已按本作品核对署名。来源原署名：Wrong Credit",
    );
    await expect(detail.locator(".source-detail-authors")).toContainText(
      mode === "updates" ? "合成作者" : "新作者",
    );
    await page.getByTestId("source-detail-back").click();
    await page
      .getByRole("button", { name: "查看其他关键词结果", exact: true })
      .click();
    const other = page.getByTestId("author-update-JM:456");
    await expect(other).toContainText("Other Writer");
    await expect(other.getByTestId("author-credit-reviewed")).toHaveCount(1);
    await expect(other.getByRole("checkbox")).toHaveCount(0);
    await expect(
      other.getByRole("button", { name: "下载到漫画库", exact: true }),
    ).toHaveCount(0);
    await expect(page.getByTestId("completion-select-all")).toHaveCount(0);
    expect(
      await page.evaluate(() => ({
        records: window.authorTest.view.records,
        inventory: window.authorTest.inventory,
      })),
    ).toEqual(rawBefore);
    const searchCalls = await page.evaluate(() =>
      window.authorTest.calls.filter(
        (call) =>
          call.command === "source_query" && call.args.kind === "search",
      ),
    );
    expect(searchCalls.length).toBe(mode === "updates" ? 0 : 3);
  });
}

test("reviewed old records reach their actual followed author without inventing a completed search range", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    const raw = h.view.records[1];
    h.view.records = [raw];
    const policy: AuthorQueryPolicy = {
      source: "JM",
      author: "Actual Author",
      queries: ["Actual Author"],
      verifiedAliases: [],
      queryFingerprint: "a".repeat(64),
      workCredits: [
        {
          workId: "456",
          expectedAuthors: ["合成作者"],
          correctedAuthors: ["Actual Author"],
        },
      ],
    };
    h.view.authorPolicies = [
      policy,
      { ...policy, author: "合成作者", queries: ["合成作者"] },
    ];
    h.view.authors.push({
      ...h.view.authors[0],
      author: "Actual Author",
      pagesRead: 0,
      observedCount: 0,
      lastCompleteAt: null,
    });
  });
  await page.getByTestId("nav-completion").click();
  await page.getByLabel("检查作者").selectOption("Actual Author");
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "Actual Author",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "检查范围尚未读完",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await page.getByLabel("检查作者").selectOption("合成作者");
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
  await page
    .getByRole("button", { name: "查看其他关键词结果", exact: true })
    .click();
  await expect(page.getByTestId("author-update-JM:456")).toContainText(
    "Actual Author",
  );
  expect(
    await page.evaluate(() => window.authorTest.view.records[0].matchedAuthors),
  ).toEqual(["合成作者"]);
});

test("ad-hoc author search applies source-specific policies across every term and preserves displayed author identity", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const hooks = window.authorTest;
    hooks.searchRecords[0].authors = ["Studio (ReviewedAlias)"];
    hooks.searchRecords[1].authors = ["DifferentWriter"];
    hooks.searchRecords[2].authors = ["ReviewedPicaAlias"];
    hooks.authorPolicies = [
      {
        source: "JM",
        author: "新作者",
        queries: ["Reviewed～Alias", "ReviewedAlias"],
        verifiedAliases: ["ReviewedAlias"],
        exactCredits: [],
        queryFingerprint: "a".repeat(64),
      },
      {
        source: "Pica",
        author: "新作者",
        queries: ["Reviewed Pica Alias"],
        verifiedAliases: ["ReviewedPicaAlias"],
        exactCredits: [],
        queryFingerprint: "b".repeat(64),
      },
    ];
  });
  await page.getByTestId("nav-author-search").click();
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("新作者");
  await page.getByRole("button", { name: "搜索两站作品" }).click();
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完 · 已记录 2 条",
  );
  await expect(page.getByTestId("completion-other-results")).toContainText(
    "其他关键词结果 1 条",
  );
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toHaveValue(
    "新作者",
  );
  expect(
    await page.evaluate(() =>
      window.authorTest.calls
        .filter((call) => call.command === "source_query")
        .map((call) => [call.args.source, call.args.query, call.args.page]),
    ),
  ).toEqual([
    ["JM", "Reviewed～Alias", 1],
    ["JM", "Reviewed～Alias", 2],
    ["JM", "ReviewedAlias", 1],
    ["JM", "ReviewedAlias", 2],
    ["Pica", "Reviewed Pica Alias", 1],
  ]);
  await page.getByRole("button", { name: "全部 2", exact: true }).click();
  await expect(page.getByTestId("author-update-JM:123")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:456")).toHaveCount(0);
  await expect(
    page.getByTestId("author-update-Pica:0123456789abcdef01234567"),
  ).toBeVisible();
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

test("completed pagination with isolated records shows positions without claiming all author works owned", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    const h = window.authorTest;
    h.inventory.items = h.view.records.map((record) => ({
      source: record.work.source,
      workId: record.work.workId,
      libraryEntryId: "b".repeat(64),
      localFiles: "present",
    }));
    for (const range of h.view.authors) {
      range.state = range.source === "JM" ? "partial" : "complete";
      range.pagesComplete = true;
      range.errorCode = range.source === "JM" ? "SOURCE_ITEMS_PARTIAL" : null;
      range.issueCount = range.source === "JM" ? 1 : 0;
      range.issueSamples =
        range.source === "JM"
          ? [{ page: 1, index: 3, workId: "789", code: "SOURCE_ITEM_INVALID" }]
          : [];
    }
  });
  await open(page);
  await expect(page.getByTestId("completion-counts")).toContainText(
    "分页已读完，来源记录仍待核对",
  );
  await expect(page.getByTestId("completion-date-sort-scope")).toContainText(
    "异常记录仍待核对，更新时间排序仅覆盖可展示作品。",
  );
  await expect(
    page.getByTestId("completion-date-sort-scope"),
  ).not.toContainText("检查范围尚未读完");
  await expect(page.getByTestId("completion-counts")).toContainText(
    "已入库 3 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  const issues = page.getByTestId("completion-source-issues");
  await issues.locator("summary").click();
  await expect(issues).toContainText("JM · 第 1 页 · 第 3 条 · 编号 789");
  await expect(issues.getByRole("button")).toHaveCount(0);
  await expect(issues.getByRole("checkbox")).toHaveCount(0);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-isolated-records.png",
  });
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
  await page.getByRole("button", { name: "作品详情", exact: true }).click();
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
