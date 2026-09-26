import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type {
  AccountSummary,
  Source,
  SourceWork,
} from "../src/source-types.ts";

declare global {
  interface Window {
    recentTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      accounts: AccountSummary[];
      version: number;
      failPage: number | null;
      holdPage: number | null;
      release?: () => void;
      total: number;
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
      window.recentTest.calls.filter((call) =>
        /discovery_start|download_(confirm|batch_confirm|selection_confirm)|source_favorite|source_follow$|source_matches_|phone_library_|library_scan/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});

// Synthetic IPC only: no accounts, source traffic, files or download execution.
async function install(page: Page) {
  await page.addInitScript(
    ({ preferences, library }) => {
      const hooks = (window.recentTest = {
        calls: [],
        accounts: (["JM", "Pica"] as const).map((source) => ({
          source,
          sessionId: "fixture-" + source,
          accountId: "fixture-account-" + source,
          displayName: "合成账号",
          state: "connected",
          remembered: false,
          errorCode: null,
        })),
        version: 0,
        failPage: null,
        holdPage: null,
        total: 40,
      } as Window["recentTest"]);
      const work = (
        source: Source,
        id: number,
        session: string,
      ): SourceWork => ({
        source,
        workId: source === "JM" ? String(id) : String(id).padStart(24, "0"),
        title: source + " 合成最近更新 " + id + " · " + session,
        authors: ["合成作者"],
        description: null,
        tags: id % 3 === 1 ? ["中文"] : id % 3 === 2 ? ["生肉"] : [],
        favorite: null,
        chapterCount: 1,
        pageCount: 20,
        coverAvailable: false,
        sourceUpdatedAt:
          id % 3 === 0
            ? null
            : new Date(1800000000000 - id * 1000).toISOString(),
      });
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        value: {
          invoke: async (
            command: string,
            args: Record<string, unknown> = {},
          ) => {
            hooks.calls.push({ command, args: structuredClone(args) });
            if (command === "read_preferences")
              return { revision: 0, value: preferences };
            if (command === "library_read") return structuredClone(library);
            if (command === "source_accounts")
              return structuredClone(hooks.accounts);
            if (command === "jm_download_read")
              return { revision: 0, tasks: [] };
            if (command === "download_inventory_read")
              return {
                rootId: library.rootId,
                revision: 1,
                libraryRevision: 1,
                items: [
                  {
                    source: "JM",
                    workId: "1",
                    libraryEntryId: "b".repeat(64),
                    localFiles: "present",
                  },
                ],
              };
            if (command === "source_following")
              return {
                source: args.source,
                sessionId: args.sessionId,
                revision: 0,
                works: [],
                authors: [],
              };
            if (command === "source_rank_options")
              return {
                source: args.source,
                sessionId: args.sessionId,
                options:
                  args.source === "JM"
                    ? {
                        categories: [{ id: "42", label: "合成第42期" }],
                        periods: [{ id: "manga", label: "日漫" }],
                      }
                    : {
                        categories: [],
                        periods: [{ id: "week", label: "周榜" }],
                      },
              };
            if (command === "source_query") {
              const source = args.source as Source;
              const session = args.sessionId as string;
              const pageNumber = Number(args.page);
              const offset = hooks.version * 100;
              const total = hooks.total;
              const start = pageNumber === 1 ? 1 : pageNumber === 2 ? 20 : 39;
              const ids =
                args.kind === "detail"
                  ? [Number(args.query)]
                  : args.kind === "ranking"
                    ? [901, 902]
                    : Array.from(
                        {
                          length: Math.max(0, Math.min(20, total - start + 1)),
                        },
                        (_, i) => offset + start + i,
                      );
              const response = {
                source,
                sessionId: session,
                items: ids.map((id) => work(source, id, session)),
                page: pageNumber,
                total: args.kind === "recent" ? total : ids.length,
                pages:
                  args.kind === "recent"
                    ? source === "JM"
                      ? null
                      : Math.ceil((total + 1) / 20)
                    : 1,
                hasMore:
                  args.kind === "recent"
                    ? source === "JM"
                      ? null
                      : start + ids.length - 1 < total
                    : false,
                folders: [],
              };
              if (args.kind === "recent" && hooks.holdPage === pageNumber) {
                hooks.holdPage = null;
                await new Promise<void>((resolve) => {
                  hooks.release = resolve;
                });
              }
              if (args.kind === "recent" && hooks.failPage === pageNumber)
                throw { code: "SOURCE_TIMEOUT" };
              return response;
            }
            if (command === "jm_download_prepare") {
              const scope = args.scope as { source: Source; sessionId: string };
              const value = work(
                scope.source,
                Number(args.input),
                scope.sessionId,
              );
              return {
                planId: "c".repeat(64),
                revision: 0,
                source: scope.source,
                workId: value.workId,
                title: value.title,
                authors: value.authors,
                destinationDisplay: "C:\\Synthetic\\" + value.title + ".zip",
                rootId: library.rootId,
                generation: 1,
              };
            }
            if (command === "jm_download_cancel_plan") return null;
            throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
          },
        },
      });
    },
    {
      preferences: initialPreferences(),
      library: {
        ...emptyLibrary(),
        rootId: "a".repeat(64),
        rootPath: "C:\\Synthetic",
        revision: 1,
        generation: 1,
        phase: "complete",
        freshness: "live",
      },
    },
  );
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByRole("button", { name: "最近更新", exact: true }).click();
}

const recentCalls = (page: Page) =>
  page.evaluate(() =>
    window.recentTest.calls
      .filter((c) => c.command === "source_query" && c.args.kind === "recent")
      .map((c) => c.args),
  );
const recentCard = (page: Page, source: Source, id: number) =>
  page.getByTestId(
    `recent-work-${source}:${source === "JM" ? String(id) : String(id).padStart(24, "0")}`,
  );

test("both recent feeds preserve source order, language and unknown dates and reuse ownership, detail and explicit download confirmation", async ({
  page,
}) => {
  await install(page);
  await expect(page.getByLabel("最近更新来源")).toHaveValue("Pica");
  await expect(page.getByTestId("recent-counts")).toContainText("20");
  await expect(
    recentCard(page, "Pica", 1).getByTestId("source-language-badge"),
  ).toHaveText("已汉化");
  await expect(
    recentCard(page, "Pica", 2).getByTestId("source-language-badge"),
  ).toHaveText("生肉");
  await expect(
    recentCard(page, "Pica", 3).getByTestId("source-language-badge"),
  ).toHaveText("未知");
  await expect(recentCard(page, "Pica", 3)).toContainText("更新时间未知");
  expect(
    (await recentCalls(page)).map((args) => [args.source, args.page]),
  ).toEqual([["Pica", 1]]);
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(page.getByTestId("recent-counts")).toContainText("已入库 1");
  await expect(page.getByTestId("recent-progress")).not.toContainText(
    "分页已读完",
  );
  await page
    .getByLabel("最近更新入库筛选")
    .getByRole("button", { name: "未入库 19", exact: true })
    .click();
  await expect(recentCard(page, "JM", 1)).toHaveCount(0);
  await expect(recentCard(page, "JM", 2)).toBeVisible();
  await recentCard(page, "JM", 2)
    .getByRole("button", { name: /查看.*详情/ })
    .click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(recentCard(page, "JM", 2)).toBeVisible();
  await expect(recentCard(page, "JM", 1)).toHaveCount(0);
  await expect(page.getByTestId("recent-counts")).toContainText("20");
  await page.getByTestId("recent-counts").scrollIntoViewIfNeeded();
  await expect(recentCard(page, "JM", 2)).toBeInViewport();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/recent-updates-jm.png" });
  expect(
    await page.evaluate(() =>
      window.recentTest.calls.filter((c) => /download_prepare/.test(c.command)),
    ),
  ).toEqual([]);
  await recentCard(page, "JM", 2)
    .getByRole("button", { name: "下载到漫画库", exact: true })
    .click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-plan-title")).toContainText(
    "JM 合成最近更新 2",
  );
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  for (const args of await recentCalls(page)) {
    expect(args.query).toBe("");
    expect(args.folderId).toBeNull();
    expect(args).not.toHaveProperty("reverse");
  }
});

test("a downward browse reads only the next page, keeps good cards on failure, and deduplicates the retried live page", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await page.evaluate(() => {
    window.recentTest.failPage = 2;
  });
  const main = page.getByRole("main");
  await main.hover();
  await page.mouse.wheel(
    0,
    await main.evaluate((element) => element.scrollHeight),
  );
  await expect(
    page
      .getByTestId("recent-panel")
      .getByRole("button", { name: "重试读取", exact: true }),
  ).toBeVisible();
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await expect(recentCard(page, "Pica", 20)).toBeVisible();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await page.evaluate(() => {
    window.recentTest.failPage = null;
  });
  await page
    .getByTestId("recent-panel")
    .getByRole("button", { name: "重试读取", exact: true })
    .click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(3000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2, 2]);
  await expect(recentCard(page, "Pica", 20)).toHaveCount(1);
  await page
    .getByLabel("筛选已读取最近更新")
    .fill("not-in-this-synthetic-catalog");
  await expect(page.getByTestId("recent-grid").locator("article")).toHaveCount(
    0,
  );
  await main.hover();
  await page.mouse.wheel(0, 10000);
  await page.clock.runFor(2000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2, 2]);
  await page.getByLabel("筛选已读取最近更新").fill("");
  await page.getByTestId("recent-counts").scrollIntoViewIfNeeded();
  await expect(recentCard(page, "Pica", 1)).toBeInViewport();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/recent-updates-pica-partial.png",
  });
});

test("a refresh keeps its previous list on error and replaces the current browsing window instead of combining old and new order", async ({
  page,
}) => {
  await install(page);
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.evaluate(() => {
    window.recentTest.version = 1;
    window.recentTest.failPage = 1;
  });
  await page.getByRole("button", { name: "刷新最近更新", exact: true }).click();
  await expect(
    page.getByTestId("recent-panel").getByRole("alert"),
  ).toBeVisible();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await expect(recentCard(page, "Pica", 101)).toHaveCount(0);
  await page.evaluate(() => {
    window.recentTest.failPage = null;
  });
  await page
    .getByTestId("recent-panel")
    .getByRole("button", { name: "重试读取", exact: true })
    .click();
  await expect(recentCard(page, "Pica", 101)).toBeVisible();
  await expect(recentCard(page, "Pica", 1)).toHaveCount(0);
  await expect(recentCard(page, "Pica", 39)).toHaveCount(0);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([
    1, 2, 1, 1,
  ]);
});

test("late pages cannot enter another source or replacement session, and switching rankings does not scan the recent feed in the background", async ({
  page,
}) => {
  await install(page);
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
  });
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(recentCard(page, "JM", 1)).toBeVisible();
  await page.evaluate(() => window.recentTest.release!());
  await expect(recentCard(page, "Pica", 21)).toHaveCount(0);
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
    delete window.recentTest.release;
  });
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.recentTest.accounts[0].sessionId = "replacement-JM";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.evaluate(() => window.recentTest.release!());
  await page.getByTestId("nav-discovery").click();
  await page.getByRole("button", { name: "最近更新", exact: true }).click();
  await expect(recentCard(page, "JM", 1)).toContainText("replacement-JM");
  await expect(recentCard(page, "JM", 21)).toHaveCount(0);
  const beforeRanks = await recentCalls(page);
  await page.getByRole("button", { name: "JM 每周必看", exact: true }).click();
  await expect(page.getByTestId("rank-work-JM:901")).toBeVisible();
  expect(await recentCalls(page)).toEqual(beforeRanks);
  await page.getByRole("button", { name: "最近更新", exact: true }).click();
  await expect(recentCard(page, "JM", 1)).toContainText("replacement-JM");
  expect(await recentCalls(page)).toEqual(beforeRanks);
});
