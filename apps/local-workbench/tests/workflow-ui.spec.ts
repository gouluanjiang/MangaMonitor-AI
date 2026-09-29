import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { installWorkflow } from "./workflow-fixture.ts";

const pica = (id: number) => "Pica:" + String(id).padStart(24, "0");
const card = (page: Page, key: string) =>
  page.getByTestId("author-update-" + key);
const counts = (page: Page) => page.getByTestId("completion-counts");
const calls = (page: Page, command: string) =>
  page.evaluate(
    (name) => window.workflowTest.calls.filter((call) => call.command === name),
    command,
  );
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const issues: string[] = [];
  errors.set(page, issues);
  page.on("pageerror", (error) => issues.push(error.message));
  // These integration scenarios are entirely synthetic, including native IPC.
  await page.route("**/*", (route) => {
    if (new URL(route.request().url()).hostname === "127.0.0.1")
      return route.continue();
    issues.push("Unexpected external request");
    return route.abort();
  });
  await page.setViewportSize({ width: 1672, height: 1020 });
  await installWorkflow(page);
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter((call) =>
        /source_(follow|favorite)$|source_matches_|phone_library_|completeness_|delete|promote|replace/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});
async function searchAuthor(page: Page) {
  await page.getByTestId("nav-author-search").click();
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("合成新作者");
  await page.getByTestId("completion-start").click();
}
async function downloadOne(page: Page, key: string) {
  await card(page, key)
    .getByRole("button", { name: "下载到漫画库", exact: true })
    .click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-plan-destination")).toContainText(
    ".zip",
  );
  await page.getByTestId("download-confirm").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(page.getByTestId("native-downloads")).toBeVisible();
}

test("manual updates, mixed downloads, automatic ownership, a new author and restart form one continuous workflow", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await page.getByTestId("nav-completion").click();
  await expect(card(page, "JM:103")).toBeVisible();
  expect(await calls(page, "discovery_start")).toHaveLength(0);
  expect(await calls(page, "source_query")).toHaveLength(0);
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  await page.evaluate(() => window.workflowTest.finishCheck());
  await expect(counts(page)).toContainText(
    "当前检查范围已读完 · 已记录 5 条 · 已入库 1 条 · 未入库 4 条",
  );
  // The second source's identically titled copy remains missing by design.
  await expect(card(page, pica(202))).toContainText("未入库");
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await card(page, "JM:102").getByRole("checkbox").check();
  await card(page, pica(201)).getByRole("checkbox").check();
  await page.getByRole("button", { name: "查看下载计划", exact: true }).click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  expect(await page.evaluate(() => window.workflowTest.queue.tasks)).toEqual(
    [],
  );
  await page.getByTestId("download-batch-confirm").click();
  await expect(page.getByTestId("download-batch-confirmation")).toHaveCount(0);
  expect(await calls(page, "jm_download_selection_confirm")).toHaveLength(1);
  await page.getByTestId("nav-completion").click();
  await expect(counts(page)).toContainText("已入库 1 条 · 未入库 4 条");
  await page.evaluate(() => window.workflowTest.finishDownloads());
  // No refresh click: the download controller's completion must update this page.
  await expect(counts(page)).toContainText(
    "已入库 3 条 · 未入库 2 条 · 当前显示 2 条",
  );
  await expect(card(page, "JM:102")).toHaveCount(0);
  await expect(card(page, pica(201))).toHaveCount(0);
  await expect(card(page, "JM:103")).toBeVisible();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/v1-updates-after-download.png",
  });
  await page.getByLabel("更新来源").selectOption("JM");
  await page.getByLabel("筛选作者更新").fill("旧遗漏");

  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("source-filter-count")).toContainText(
    "已入库 2 部 · 未入库 1 部",
  );
  await page.getByTestId("source-filter-missing").click();
  await expect(page.getByTestId("source-card-JM:103")).toBeVisible();
  await expect(page.getByTestId("source-card-JM:102")).toHaveCount(0);
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已入库 2 条 · 未入库 1 条",
  );

  await searchAuthor(page);
  await expect(counts(page)).toContainText(
    "当前检查范围已读完 · 已记录 4 条 · 已入库 2 条 · 未入库 2 条",
  );
  const searches = (await calls(page, "source_query")).filter(
    (c) => c.args.kind === "search",
  );
  expect(searches.map((c) => [c.args.source, c.args.page])).toEqual([
    ["JM", 1],
    ["JM", 2],
    ["Pica", 1],
    ["Pica", 2],
  ]);
  await downloadOne(page, "JM:104");
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByLabel("搜索作者名")).toHaveValue("合成新作者");
  await expect(counts(page)).toContainText("已记录 4 条");
  await page.evaluate(() => window.workflowTest.finishDownloads());
  await expect(counts(page)).toContainText(
    "已入库 3 条 · 未入库 1 条 · 当前显示 1 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  await downloadOne(page, pica(203));
  await page.evaluate(() => window.workflowTest.finishDownloads());
  await page.getByTestId("nav-author-search").click();
  await expect(counts(page)).toContainText(
    "已入库 4 条 · 未入库 0 条 · 当前显示 0 条",
  );
  await expect(page.getByTestId("completion-all-owned")).toContainText(
    "合成新作者",
  );
  expect(
    (await calls(page, "source_query")).filter((c) => c.args.kind === "search"),
  ).toHaveLength(4);
  await page.screenshot({ path: "visual-evidence/v1-search-all-owned.png" });
  await page.getByTestId("nav-completion").click();
  await expect(page.getByLabel("更新来源")).toHaveValue("JM");
  await expect(page.getByLabel("筛选作者更新")).toHaveValue("旧遗漏");
  await expect(card(page, "JM:103")).toBeVisible();
  expect(await calls(page, "discovery_start")).toHaveLength(1);
  expect(
    await page.evaluate(
      () => new Set(window.workflowTest.library.items.map((i) => i.id)).size,
    ),
  ).toBe(5);
  await page.reload();
  await page.getByTestId("nav-completion").click();
  await expect(counts(page)).toContainText("已入库 3 条 · 未入库 2 条");
  expect(await calls(page, "discovery_start")).toHaveLength(0);
  expect(await calls(page, "jm_download_confirm")).toHaveLength(0);
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByLabel("搜索作者名")).toBeEmpty();
  expect(await calls(page, "source_query")).toHaveLength(0);
});

test("a later search-page failure survives navigation without claiming completion or silently retrying", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.workflowTest.searchFault = "fail-last";
  });
  await searchAuthor(page);
  await expect(counts(page)).toContainText("检查范围尚未读完 · 已记录 3 条");
  await page.getByText("查看未完成范围", { exact: true }).click();
  await expect(
    page.getByText(
      "合成新作者 · 哔咔 · 已读取 1 页 · 来源读取未完成（SEARCH_INCOMPLETE）",
      {
        exact: true,
      },
    ),
  ).toBeVisible();
  await page.getByLabel("更新来源").selectOption("Pica");
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByLabel("更新来源")).toHaveValue("Pica");
  await expect(counts(page)).toContainText("检查范围尚未读完 · 已记录 1 条");
  await expect(page.getByTestId("completion-all-owned")).toHaveCount(0);
  expect(await calls(page, "source_query")).toHaveLength(4);
  await page.evaluate(() => {
    window.workflowTest.searchFault = "none";
  });
  await page.getByLabel("更新来源").selectOption("all");
  await page.getByTestId("completion-start").click();
  await expect(counts(page)).toContainText("当前检查范围已读完 · 已记录 4 条");
  expect(await calls(page, "source_query")).toHaveLength(8);
});

test("a manually started search survives a queue visit but a changed account invalidates its pending pages", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.workflowTest.searchFault = "hold-last";
  });
  await searchAuthor(page);
  await expect
    .poll(() => page.evaluate(() => Boolean(window.workflowTest.releasePage)))
    .toBe(true);
  await page.getByTestId("nav-queue").click();
  await page.evaluate(() => {
    window.workflowTest.searchFault = "none";
    window.workflowTest.releasePage!();
  });
  await page.getByTestId("nav-author-search").click();
  await expect(counts(page)).toContainText("当前检查范围已读完 · 已记录 4 条");
  expect(await calls(page, "source_query")).toHaveLength(4);

  await page.evaluate(() => {
    window.workflowTest.searchFault = "hold-first";
    delete window.workflowTest.releasePage;
  });
  await page.getByTestId("completion-start").click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.workflowTest.releasePage)))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.workflowTest.accounts[0].sessionId = "synthetic-new-JM";
  });
  await page.getByTestId("accounts-reload").click();
  await expect(page.getByTestId("accounts-reload")).toBeEnabled();
  await page.evaluate(() => {
    window.workflowTest.searchFault = "none";
    window.workflowTest.releasePage!();
  });
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByLabel("搜索作者名")).toBeEmpty();
  await expect(counts(page)).toContainText("已记录 0 条");
  await searchAuthor(page);
  await expect(counts(page)).toContainText("当前检查范围已读完 · 已记录 4 条");
  const requests = await calls(page, "source_query");
  expect(
    requests
      .filter((c) => c.args.sessionId === "synthetic-JM")
      .map((c) => c.args.page),
  ).toEqual([1, 2, 1]);
  expect(
    requests
      .filter((c) => c.args.sessionId === "synthetic-new-JM")
      .map((c) => c.args.page),
  ).toEqual([1, 2]);
});

test("library read failure makes author and ranking ownership unknown until a successful directory read", async ({
  page,
}) => {
  await page.getByTestId("nav-completion").click();
  await page.getByTestId("completion-start").click();
  await page.evaluate(() => window.workflowTest.finishCheck());
  await expect(counts(page)).toContainText("已入库 1 条 · 未入库 4 条");
  await page.getByTestId("nav-library").click();
  await page.evaluate(() => {
    window.workflowTest.failLibraryRead = true;
  });
  await page.getByTestId("library-refresh").click();
  await expect(page.getByTestId("library-retry")).toBeVisible();
  await page.getByTestId("nav-completion").click();
  await expect(counts(page)).toContainText(
    "已入库 0 条 · 未入库 0 条 · 当前显示 0 条 · 状态待核实 5 条",
  );
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("discovery-JM").click();
  await expect(page.getByTestId("ranking-counts")).toContainText(
    "已入库 0 条 · 未入库 0 条",
  );
  await page.getByTestId("nav-library").click();
  await page.evaluate(() => {
    window.workflowTest.failLibraryRead = false;
  });
  await page.getByTestId("library-retry").click();
  await expect(page.getByTestId("library-refresh")).toBeEnabled();
  await expect(page.getByTestId("library-retry")).toHaveCount(0);
  expect(
    await page.evaluate(() => window.workflowTest.library.generation),
  ).toBe(2);
  await page.getByTestId("nav-completion").click();
  await expect(counts(page)).toContainText("已入库 1 条 · 未入库 4 条");
});
