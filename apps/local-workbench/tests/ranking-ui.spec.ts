import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type { SourceWork } from "../src/source-types.ts";

declare global {
  interface Window {
    rankingTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      partial: boolean;
      failOptions: boolean;
      failList: boolean;
      hold: boolean;
      isolated: boolean;
      release?: () => void;
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
      window.rankingTest.calls.filter((call) =>
        /discovery_start|download_confirm|source_favorite|source_follow$|source_matches_/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});
async function install(page: Page) {
  const rootId = "a".repeat(64);
  const accounts = (["JM", "Pica"] as const).map((source) => ({
    source,
    sessionId: "fixture-" + source,
    accountId: "account-" + source,
    displayName: "合成账号",
    state: "connected",
    remembered: false,
    errorCode: null,
  }));
  const works: SourceWork[] = accounts.flatMap(({ source }) =>
    [1, 2].map((i) => ({
      source,
      workId: source === "JM" ? String(i) : String(i).padStart(24, "0"),
      title: source + " 合成榜单作品 " + i,
      authors: ["合成作者"],
      description: null,
      tags: [],
      favorite: null,
      chapterCount: 1,
      pageCount: 20,
      coverAvailable: false,
    })),
  );
  await page.addInitScript(
    ({ accounts, works, rootId, preferences, library }) => {
      const hooks = (window.rankingTest = {
        calls: [],
        partial: false,
        failOptions: false,
        failList: false,
        hold: false,
        isolated: false,
      } as Window["rankingTest"]);
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
            if (command === "source_accounts") return structuredClone(accounts);
            if (command === "jm_download_read")
              return { revision: 0, tasks: [] };
            if (command === "download_inventory_read")
              return {
                revision: 0,
                libraryRevision: 1,
                rootId,
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
            if (command === "source_rank_options") {
              if (hooks.failOptions) throw { code: "SOURCE_TIMEOUT" };
              return {
                source: args.source,
                sessionId: args.sessionId,
                options:
                  args.source === "JM"
                    ? {
                        categories: [
                          { id: "42", label: "2026第42期09.11 - 09.04" },
                          { id: "41", label: "合成第41期" },
                        ],
                        periods: [
                          { id: "hanman", label: "韓漫" },
                          { id: "another", label: "其他" },
                          { id: "manga", label: "日漫" },
                        ],
                      }
                    : {
                        categories: [],
                        periods: [
                          { id: "week", label: "周榜" },
                          { id: "day", label: "日榜" },
                          { id: "month", label: "月榜" },
                        ],
                      },
              };
            }
            if (command === "source_query") {
              if (args.kind === "ranking" && hooks.hold) {
                hooks.hold = false;
                await new Promise<void>((resolve) => {
                  hooks.release = resolve;
                });
              }
              if (args.kind === "ranking" && hooks.failList)
                throw { code: "SOURCE_TIMEOUT" };
              const items = works.filter(
                (work) =>
                  work.source === args.source &&
                  !(
                    args.kind === "ranking" &&
                    args.source === "JM" &&
                    args.query === "hanman"
                  ) &&
                  (args.kind !== "detail" || work.workId === args.query),
              );
              return {
                source: args.source,
                sessionId: args.sessionId,
                items,
                issues:
                  hooks.isolated && args.kind === "ranking"
                    ? [
                        {
                          page: 1,
                          index: items.length + 1,
                          workId: "999",
                          code: "SOURCE_ITEM_INVALID",
                        },
                      ]
                    : [],
                page: 1,
                total: hooks.partial
                  ? 20
                  : items.length +
                    (hooks.isolated && args.kind === "ranking" ? 1 : 0),
                pages: hooks.partial ? null : 1,
                hasMore: hooks.partial ? null : false,
                folders: [],
              };
            }
            if (command === "jm_download_prepare")
              throw { code: "SOURCE_TIMEOUT" };
            throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
          },
        },
      });
    },
    {
      accounts,
      works,
      rootId,
      preferences: initialPreferences(),
      library: {
        ...emptyLibrary(),
        rootId,
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
}

test("ranking retains good works and exposes isolated source positions as read-only diagnostics", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    window.rankingTest.isolated = true;
  });
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已读取 2 条 / 来源报告 3 条",
  );
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "分页已读完，仍有来源记录待核对",
  );
  const issues = page.getByTestId("ranking-issues");
  await issues.locator("summary").click();
  await expect(issues).toContainText("JM · 第 1 页 · 第 3 条 · 编号 999");
  await expect(issues.getByRole("button")).toHaveCount(0);
  await expect(page.getByTestId("rank-work-JM:2")).toBeVisible();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/ranking-isolated-records.png",
  });
});

test("weekly and Pica ranks share receipt filters while details preserve the selected list", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已入库 1 条 · 未入库 1 条",
  );
  await expect(page.getByLabel("排行类型")).toHaveValue("manga");
  await expect(page.getByLabel("每周必看期数")).toContainText(
    "2026第42期09.11 - 09.04",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/jm-weekly.png" });
  await page.getByLabel("排行类型").selectOption("hanman");
  await expect(
    page.getByText("本期该类型暂无作品，可以切换期数或类型。"),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.getByLabel("排行类型").selectOption("manga");
  await page.getByLabel("每周必看期数").selectOption("41");
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "本次榜单已读完",
  );
  await page.getByRole("button", { name: "未入库 1", exact: true }).click();
  await expect(page.getByTestId("rank-work-JM:1")).toHaveCount(0);
  await page
    .getByTestId("rank-work-JM:2")
    .getByRole("button", { name: /查看.*详情/ })
    .click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByLabel("每周必看期数")).toHaveValue("41");
  await expect(
    page.getByRole("button", { name: "未入库 1", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("ranking-panel").getByTestId("discovery-Pica").click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已入库 0 条 · 未入库 2 条",
  );
  await page.getByLabel("排行类型").selectOption("month");
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "本次榜单已读完",
  );
  expect(
    await page.evaluate(() =>
      window.rankingTest.calls
        .filter(
          (call) =>
            call.command === "source_query" && call.args.kind === "ranking",
        )
        .map((call) => [call.args.source, call.args.query, call.args.folderId]),
    ),
  ).toContainEqual(["Pica", "month", null]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/pica-rankings.png" });
});

test("options can be retried and short or failed lists never claim complete", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    window.rankingTest.failOptions = true;
  });
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.getByRole("alert")).not.toContainText("已读取榜单保留");
  await page.evaluate(() => {
    window.rankingTest.failOptions = false;
    window.rankingTest.partial = true;
  });
  await page.getByRole("button", { name: "刷新榜单" }).click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已读取 2 条 / 来源报告 20 条",
  );
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "尚未完整确认",
  );
  await page.evaluate(() => {
    window.rankingTest.failList = true;
  });
  await page.getByRole("button", { name: "刷新榜单" }).click();
  await expect(page.getByRole("alert")).toContainText("已读取榜单保留");
  await expect(page.getByTestId("rank-work-JM:2")).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.rankingTest.calls
        .filter(
          (call) =>
            call.command === "source_query" && call.args.kind === "ranking",
        )
        .every((call) => call.args.page === 1),
    ),
  ).toBe(true);
});

test("a late result cannot replace another source and downloading still requires an explicit action", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    window.rankingTest.hold = true;
  });
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByText("正在读取来源榜单…")).toBeVisible();
  await page.waitForFunction(() => Boolean(window.rankingTest.release));
  await page.getByTestId("ranking-panel").getByTestId("discovery-Pica").click();
  await expect(page.getByTestId("ranking-counts")).toContainText("未入库 2 条");
  await page.evaluate(() => window.rankingTest.release?.());
  await expect(page.getByTestId("rank-work-JM:2")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.rankingTest.calls.filter((call) =>
        /download_prepare/.test(call.command),
      ),
    ),
  ).toEqual([]);
  await page
    .getByTestId("rank-work-Pica:000000000000000000000002")
    .getByRole("button", { name: "下载到漫画库" })
    .click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.rankingTest.calls
          .filter((call) => call.command === "jm_download_prepare")
          .map((call) => call.args.input),
      ),
    )
    .toEqual(["000000000000000000000002"]);
});
