import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import type {
  AccountSummary,
  FollowingSnapshot,
  Source,
  SourceWork,
  CatalogSnapshot,
} from "../src/source-types.ts";
import type { BooklistsDocument } from "../src/booklists.ts";
import type { WorkbenchPreferences } from "../src/preferences.ts";
// Inert legacy fixtures prove that removed matching documents are never requested.
type SourceMatchWork = { source: Source; workId: string; title: string };
type SourceMatchesSnapshot = {
  revision: number;
  pairs: { id: string; jm: SourceMatchWork; pica: SourceMatchWork }[];
};

// These are Chromium browser-preview integration tests using synthetic Tauri IPC.
// They do not contact source sites, use real credentials, run native WebViews,
// or establish native secure-storage and network correctness.
test.use({ storageState: { cookies: [], origins: [] } });

type MockOptions = {
  disconnected?: boolean;
  expired?: boolean;
  holdLogin?: boolean;
  holdJM?: boolean;
  partial?: boolean;
  unknownFavorite?: boolean;
  followConflict?: boolean;
  coverCount?: number;
  coverDelay?: number;
  expireJM?: boolean;
  collectionCount?: number;
  collectionCover?: boolean;
  collectionAuthors?: boolean;
  holdPicaPage?: number;
  picaPageFailureOnce?: number;
  picaPageFailureCode?: string;
  picaCachePage?: number;
  picaDuplicateRecord?: number;
  coverFailureOnce?: boolean;
  cacheSnapshot?: CatalogSnapshot;
  crossSourcePhone?: boolean;
  crossSourcePC?: boolean;
  libraryCandidate?: boolean;
  holdMatchDetail?: boolean;
  authorSearchResults?: boolean;
  authorPolicyResults?: boolean;
  reviewedWorkCredits?: boolean;
  workDates?: boolean;
  isolatedListing?: boolean;
  workTags?: Record<string, string[]>;
  detailTags?: Record<string, string[]>;
};
type Call = {
  command: string;
  source?: Source;
  sessionId?: string | null;
  kind?: string;
  page?: number;
  workId?: string;
  desired?: boolean;
  expectedRevision?: number;
  query?: string;
  reverse?: boolean;
};
type Hooks = {
  accounts: AccountSummary[];
  calls: Call[];
  booklists: { revision: number; value: BooklistsDocument };
  preferences: { revision: number; value: WorkbenchPreferences };
  following: Record<Source, FollowingSnapshot>;
  matches: SourceMatchesSnapshot;
  matchHeld: boolean;
  releaseMatch?: () => void;
  loginStarted: boolean;
  jmHeld: boolean;
  picaHeld: boolean;
  releaseLogin?: (success: boolean) => void;
  releaseJM?: () => void;
  releasePica?: () => void;
  changedDetailCredit?: boolean;
  coverActive?: number;
  coverMax?: number;
};
declare global {
  interface Window {
    sourceTest: Hooks;
  }
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const collected: string[] = [];
  errors.set(page, collected);
  page.on("pageerror", (error) => collected.push(error.message));
});

test("cached 2000-work catalog uses bounded rows, full-data selection and stable density/detail anchors", async ({
  page,
}) => {
  const items: SourceWork[] = Array.from({ length: 2000 }, (_, i) => ({
    source: "JM",
    workId: String(i + 1),
    title: "合成验收 JM 作品 " + (i + 1) + " 账号1",
    authors: ["合成验收作者"],
    description: null,
    tags: [],
    favorite: null,
    chapterCount: null,
    pageCount: null,
    coverAvailable: true,
  }));
  const snapshot: CatalogSnapshot = {
    items,
    page: 100,
    total: 2000,
    pages: 100,
    hasMore: false,
    folders: [],
    complete: true,
    updatedAt: 1800000000000,
    firstPageIds: items.slice(0, 20).map((work) => work.workId),
  };
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    collectionCount: 2000,
    collectionCover: true,
    cacheSnapshot: snapshot,
  });
  await openFavorites(page);
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏",
  );
  await expect(page.getByTestId("source-grid")).toHaveAttribute(
    "data-total-items",
    "2000",
  );
  expect(
    await page.getByTestId("source-grid").locator("article").count(),
  ).toBeLessThan(90);
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 2000 部",
  );
  await page.getByTestId("source-grid").evaluate((element) => {
    element.closest("main")!.scrollTop = 18000;
  });
  let anchor: string | null = null;
  // Scrolling replaces virtual rows on the next animation frame. Resolve the
  // attached rows inside the same DOM read instead of holding detached handles.
  await expect
    .poll(async () => {
      anchor = await page.getByTestId("source-grid").evaluate((grid) => {
        const main = grid.closest("main");
        if (!main) return null;
        const bounds = main.getBoundingClientRect();
        return (
          [...grid.querySelectorAll("article")]
            .find((element) => {
              const row = element.getBoundingClientRect();
              return row.top >= bounds.top && row.top < bounds.bottom;
            })
            ?.getAttribute("data-source-work-key") ?? null
        );
      });
      return anchor;
    })
    .toMatch(/^JM:\d+$/);
  // Density controls stay above the virtual rows; a direct click avoids scrolling the anchor away.
  for (const density of [5, 9, 7]) {
    await page
      .getByRole("button", { name: "来源每行 " + density + " 部", exact: true })
      .evaluate((button: HTMLButtonElement) => button.click());
    await expect(page.getByTestId("source-card-" + anchor)).toBeVisible();
    await expect(page.getByTestId("source-selection-bar")).toContainText(
      "已选 2000 部",
    );
    expect(
      await page.getByTestId("source-grid").locator("article").count(),
    ).toBeLessThan(90);
  }
  await page.getByTestId("source-open-" + anchor).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByTestId("source-card-" + anchor)).toBeVisible();
  // Real wheel input immediately after return must override pending anchor settling.
  await page.mouse.move(1200, 700);
  await page.mouse.wheel(0, 1000000);
  await expect(page.getByTestId("source-card-JM:2000")).toBeVisible();
  await page
    .getByTestId("source-workbench")
    .locator(".source-sort select")
    .selectOption("source-reverse");
  await expect(page.getByTestId("source-card-JM:2000")).toBeVisible();
  await expect
    .poll(() =>
      page
        .getByTestId("source-grid")
        .evaluate((element) => element.closest("main")!.scrollTop),
    )
    .toBe(0);
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "JM:2000");
  await expect(
    page.getByTestId("source-cover-JM:2000").locator("img"),
  ).toBeVisible();
  await page.getByTestId("source-open-JM:2000").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await page.mouse.move(1200, 700);
  await page.mouse.wheel(0, 1000000);
  await expect(page.getByTestId("source-card-JM:1")).toBeVisible();
  await expect
    .poll(() =>
      page
        .getByTestId("source-cover-JM:1")
        .locator("img")
        .evaluate(
          (image: HTMLImageElement) => image.complete && image.naturalWidth > 0,
        ),
    )
    .toBe(true);
  for (const viewport of [
    { width: 390, height: 844, columns: 2 },
    { width: 1672, height: 941, columns: 7 },
    { width: 390, height: 844, columns: 2 },
  ]) {
    await page.setViewportSize({
      width: viewport.width,
      height: viewport.height,
    });
    await expect(page.getByTestId("source-workbench")).toBeVisible();
    await expect(
      page.getByTestId("source-grid").locator(".source-virtual-row").first(),
    ).toHaveAttribute("data-columns", String(viewport.columns));
    await expect(page.getByTestId("source-selection-bar")).toContainText(
      "已选 2000 部",
    );
    const heights = await page.getByTestId("source-grid").evaluate(
      (element) =>
        new Promise<string[]>((resolve) => {
          const values: string[] = [];
          const sample = () => {
            values.push((element as HTMLElement).style.height);
            if (values.length === 16) resolve(values.slice(-8));
            else requestAnimationFrame(sample);
          };
          requestAnimationFrame(sample);
        }),
    );
    expect(
      new Set(heights).size,
      "row geometry must settle instead of oscillating between estimated and measured heights",
    ).toBe(1);
    expect(
      await page.getByTestId("source-grid").locator("article").count(),
    ).toBeLessThan(90);
    // The final selected work and its decoded synthetic cover remain reachable after reflow.
    await page.mouse.move(viewport.width - 40, viewport.height / 2);
    await page.mouse.wheel(0, 1000000);
    await expect(page.getByTestId("source-select-JM:1")).toBeChecked();
    await expect(page.getByTestId("source-card-JM:1")).toBeVisible();
    await expect(
      page.getByTestId("source-cover-JM:1").locator("img"),
    ).toBeVisible();
  }
});

test("explicit Pica time switches finish one forward catalog and reuse it for local reversal and filtering", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { collectionCount: 65, collectionAuthors: true });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1]);
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-Pica:1").check();
  await page
    .getByTestId("source-workbench")
    .locator(".source-sort select")
    .selectOption("source-reverse");
  await expect(page.getByTestId("source-card-Pica:65")).toBeVisible();
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("source-workbench")).toContainText(
    "临时选择已清空",
  );
  await expect(page.getByTestId("source-next-page")).toHaveCount(0);
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
  await expect(page.getByTestId("source-search-input")).toHaveAttribute(
    "placeholder",
    "搜索全部收藏的作品或作者…",
  );
  for (const query of ["作品 1 账号", "目录作者:1:"]) {
    await page.getByTestId("source-search-input").fill(query);
    await expect(page.getByTestId("source-grid")).toHaveAttribute(
      "data-total-items",
      "1",
    );
    await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  }
  await page.getByTestId("source-search-input").fill("");
  const sort = page
    .getByTestId("source-workbench")
    .locator(".source-sort select");
  await sort.selectOption("source");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:1");
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  await sort.selectOption("source-reverse");
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:65");
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

test("Pica finishes 1877 raw records, deduplicates works and reverses the complete catalog without another scan", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    collectionCount: 1877,
    collectionAuthors: true,
    picaCachePage: 92,
    picaDuplicateRecord: 1860,
  });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("collection-progress")).toContainText(
    "1840 / 1877",
  );
  const sort = page
    .getByTestId("source-workbench")
    .locator(".source-sort select");
  await sort.selectOption("source-reverse");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 1877 / 1877 条来源记录 · 1876 部不同作品 · 1 条重复记录",
  );
  await expect(page.getByTestId("source-grid")).toHaveAttribute(
    "data-total-items",
    "1876",
  );
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:1877");
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 1876 部",
  );
  for (const id of ["1", "1859"]) {
    await page.getByTestId("source-search-input").fill("目录作者:" + id + ":");
    await expect(page.getByTestId("source-grid")).toHaveAttribute(
      "data-total-items",
      "1",
    );
    await expect(page.getByTestId("source-card-Pica:" + id)).toHaveCount(1);
    await expect(page.getByTestId("source-card-Pica:" + id)).toBeVisible();
  }
  await page.getByTestId("source-search-input").fill("");
  await sort.selectOption("source");
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:1");
  await sort.selectOption("source-reverse");
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:1877");
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 93, 94]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

test("Pica choosing oldest-first from title sorting reads the complete forward catalog", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { collectionCount: 65 });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  const sort = page
    .getByTestId("source-workbench")
    .locator(".source-sort select");
  await sort.selectOption("title");
  await sort.selectOption("source-reverse");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:65");
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

test("Pica complete reading stays paused across settings and verifies before resuming its forward catalog", async ({
  page,
}) => {
  await installMock(page, { collectionCount: 65, holdPicaPage: 2 });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  await page
    .getByTestId("source-workbench")
    .locator(".source-sort select")
    .selectOption("source-reverse");
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.picaHeld))
    .toBe(true);
  await page
    .getByTestId("collection-pause")
    .evaluate((button: HTMLButtonElement) => button.click());
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("collection-pause")).toHaveText("继续自动读取");
  await page.evaluate(() => window.sourceTest.releasePica?.());
  await expect(page.getByTestId("collection-progress")).toContainText(
    "40 / 65",
  );
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2]);
  await page
    .getByTestId("collection-pause")
    .evaluate((button: HTMLButtonElement) => button.click());
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 1, 3, 4]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

test("favorite full selection waits for every page and includes offscreen results", async ({
  page,
}) => {
  await installMock(page, { collectionCount: 65, holdPicaPage: 2 });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.picaHeld))
    .toBe(true);
  await expect(page.getByTestId("source-select-all")).toContainText(
    "完成后全选",
  );
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await page.evaluate(() => window.sourceTest.releasePica?.());
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 65 部",
  );
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  await expect(page.getByTestId("source-select-all")).toHaveText(
    "全选当前筛选范围",
  );
});

test("Pica time switching reuses its outstanding forward page and keeps complete-reading intent", async ({
  page,
}) => {
  await installMock(page, { collectionCount: 65, holdPicaPage: 2 });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  const sort = page
    .getByTestId("source-workbench")
    .locator(".source-sort select");
  await sort.selectOption("source-reverse");
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.picaHeld))
    .toBe(true);
  await sort.selectOption("source");
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
  await page.evaluate(() => window.sourceTest.releasePica?.());
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:1");
  await expect(page.getByTestId("source-grid")).toHaveAttribute(
    "data-total-items",
    "65",
  );
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  await sort.selectOption("source-reverse");
  await expect(
    page.getByTestId("source-grid").locator("article").first(),
  ).toHaveAttribute("data-source-work-key", "Pica:65");
  expect(await picaFavoritePages(page, true)).toEqual([]);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
});

for (const code of ["SOURCE_RATE_LIMITED", "INVALID_RESPONSE"]) {
  test(`Pica ${code} stays stopped across settings and time switching until explicit retry`, async ({
    page,
  }) => {
    await installMock(page, {
      collectionCount: 65,
      picaPageFailureOnce: 2,
      picaPageFailureCode: code,
    });
    await openFavorites(page);
    await page.getByTestId("source-tab-Pica").click();
    await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
    await page
      .getByTestId("source-workbench")
      .locator(".source-sort select")
      .selectOption("source-reverse");
    await expect(page.getByTestId("collection-retry")).toHaveCount(1);
    await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
    await expect(page.getByTestId("collection-progress")).toContainText(
      "读取已停止，已读内容保留，请点击重试读取",
    );
    await expect(page.getByTestId("collection-progress")).not.toContainText(
      "向下滚动继续读取",
    );
    await expect(page.getByTestId("source-completeness")).toContainText(
      "本次读取未完成",
    );
    await expect(page.getByTestId("collection-pause")).toHaveCount(0);
    await page.getByTestId("nav-settings").click();
    await page.getByTestId("nav-favorites").click();
    await page
      .getByTestId("source-workbench")
      .locator(".source-sort select")
      .selectOption("source");
    await page.waitForTimeout(700);
    expect(await picaFavoritePages(page, false)).toEqual([1, 2]);
    await expect(page.getByTestId("collection-retry")).toHaveCount(1);
    await page
      .getByTestId("collection-retry")
      .evaluate((button: HTMLButtonElement) => button.click());
    await expect(page.getByTestId("collection-progress")).toContainText(
      "已读取全部收藏 · 已读取 65 / 65",
    );
    expect(await picaFavoritePages(page, false)).toEqual([1, 2, 1, 2, 3, 4]);
    expect(await picaFavoritePages(page, true)).toEqual([]);
  });
}

test("retrying an ordinary Pica first-page failure keeps viewport-driven reading", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    collectionCount: 65,
    picaPageFailureOnce: 1,
    picaPageFailureCode: "INVALID_RESPONSE",
  });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("collection-retry")).toHaveCount(1);
  await page
    .getByTestId("collection-retry")
    .evaluate((button: HTMLButtonElement) => button.click());
  await expect(page.getByTestId("collection-progress")).toContainText(
    "20 / 65",
  );
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 1]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

test("title sorting cancels Pica full-reading intent while explicit retry still retries the failed page", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    collectionCount: 65,
    picaPageFailureOnce: 2,
    picaPageFailureCode: "INVALID_RESPONSE",
  });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  const sort = page
    .getByTestId("source-workbench")
    .locator(".source-sort select");
  await sort.selectOption("source-reverse");
  await expect(page.getByTestId("collection-retry")).toHaveCount(1);
  await sort.selectOption("title");
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2]);
  await page
    .getByTestId("collection-retry")
    .evaluate((button: HTMLButtonElement) => button.click());
  await expect(page.getByTestId("collection-progress")).toContainText(
    "40 / 65",
  );
  await page.waitForTimeout(700);
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 2]);
  expect(await picaFavoritePages(page, true)).toEqual([]);
});

for (const action of ["title", "refresh", "source"] as const) {
  test(`Pica complete reading stops on ${action} without authorizing another full scan`, async ({
    page,
  }) => {
    await installMock(page, { collectionCount: 65, holdPicaPage: 2 });
    await openFavorites(page);
    await page.getByTestId("source-tab-Pica").click();
    await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
    const sort = page
      .getByTestId("source-workbench")
      .locator(".source-sort select");
    await sort.selectOption("source-reverse");
    await expect
      .poll(() => page.evaluate(() => window.sourceTest.picaHeld))
      .toBe(true);
    if (action === "title") await sort.selectOption("title");
    else if (action === "refresh")
      await page.getByTestId("source-refresh").click();
    else await page.getByTestId("source-tab-JM").click();
    await page.evaluate(() => window.sourceTest.releasePica?.());
    if (action === "source") {
      await expect(page.getByTestId("source-card-JM:1")).toBeVisible();
      await page.getByTestId("source-tab-Pica").click();
      await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
    }
    await expect(page.getByTestId("collection-progress")).toContainText(
      action === "title" ? "40 / 65" : "20 / 65",
    );
    await page.waitForTimeout(700);
    expect(await picaFavoritePages(page, true)).toEqual([]);
    expect(await picaFavoritePages(page, false)).toEqual(
      action === "title" ? [1, 2] : [1, 2, 1],
    );
  });
}

test("partial favorites rebind scrolling after detail and do not fetch more for an empty local filter", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { collectionCount: 2000 });
  await openFavorites(page);
  await expect(page.getByTestId("collection-progress")).toContainText(
    "20 / 2000",
  );
  await page.getByTestId("source-search-input").fill("绝不存在的本地筛选");
  await expect(page.getByTestId("source-empty")).toContainText("没有匹配作品");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "清空筛选后继续",
  );
  // The empty grid leaves its sentinel in view for longer than the page throttle.
  await page.waitForTimeout(700);
  expect(await favoritePages(page)).toEqual([1]);
  await page.getByRole("button", { name: "清空来源搜索", exact: true }).click();
  await page.getByTestId("source-open-JM:1").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByTestId("source-card-JM:1")).toBeInViewport();
  // Returning restores the saved anchor over rendering frames. Actual wheel
  // input supersedes that restoration; scrollIntoView does not express user
  // intent and can be overwritten before the sentinel enters the viewport.
  const scrollArea = page.getByRole("main");
  await scrollArea.hover();
  await page.mouse.wheel(
    0,
    await scrollArea.evaluate((element) => element.scrollHeight),
  );
  await expect(page.getByTestId("collection-progress")).toContainText(
    "40 / 2000",
  );
  expect(await favoritePages(page)).toEqual([1, 2]);
});

test("paused favorites verify their head after a settings visit before resuming; JM folder changes reset full-reverse intent", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { collectionCount: 2000 });
  await openFavorites(page);
  await expect(page.getByTestId("collection-progress")).toContainText(
    "20 / 2000",
  );
  await page
    .getByTestId("collection-pause")
    .evaluate((button: HTMLButtonElement) => button.click());
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("collection-pause")).toHaveText("继续自动读取");
  expect(await favoritePages(page)).toEqual([1]);
  await page
    .getByTestId("collection-pause")
    .evaluate((button: HTMLButtonElement) => button.click());
  await expect.poll(() => favoritePages(page)).toEqual([1, 1]);
  await page
    .getByTestId("source-workbench")
    .locator(".source-sort select")
    .selectOption("source-reverse");
  await expect(page.getByTestId("collection-sentinel")).toContainText(
    "正在准备完整来源倒序",
  );
  await page.getByTestId("source-folder").selectOption("folder-one");
  await expect(
    page.getByTestId("source-workbench").locator(".source-sort select"),
  ).toHaveValue("source");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "20 / 2000",
  );
  await expect(page.getByTestId("collection-sentinel")).not.toContainText(
    "正在准备完整来源倒序",
  );
  await page.waitForTimeout(700);
  await expect(page.getByTestId("collection-progress")).toContainText(
    "20 / 2000",
  );
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? [], "browser preview runtime errors").toEqual([]);
  // Browsing, favorites and local booklist interactions may restore the queue,
  // but none may prepare, confirm, control or otherwise start download work.
  const forbidden = await page.evaluate(() =>
    (window.sourceTest?.calls ?? []).filter(
      (call) =>
        !["jm_download_read", "download_inventory_read"].includes(
          call.command,
        ) &&
        /download|enqueue|delete|remove_file|move_file|production|promote/.test(
          call.command,
        ),
    ),
  );
  expect(forbidden).toEqual([]);
});

async function installMock(page: Page, options: MockOptions = {}) {
  await page.addInitScript((options: MockOptions) => {
    const sources: Source[] = ["JM", "Pica"];
    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
    let coverFailureUsed = false;
    const makeAccount = (source: Source, epoch = 1): AccountSummary => ({
      source,
      sessionId: "synthetic-" + source + "-" + epoch,
      accountId: "synthetic-account-" + epoch,
      displayName: "合成验收账号 " + source + " " + epoch,
      state: "connected",
      remembered: false,
      errorCode: null,
    });
    const makeWork = (
      source: Source,
      workId = "123",
      epoch = 1,
    ): SourceWork => ({
      source,
      workId,
      title: "合成验收 " + source + " 作品 " + workId + " 账号" + epoch,
      authors: options.collectionAuthors
        ? ["目录作者:" + workId + ":"]
        : options.collectionCover
          ? ["合成验收作者"]
          : [],
      description: null,
      tags: options.workTags?.[workId] ?? [],
      favorite: null,
      chapterCount: null,
      pageCount: null,
      coverAvailable: Boolean(options.collectionCover),
    });
    const makeCollectionWork = (source: Source, record: number, epoch = 1) =>
      makeWork(
        source,
        String(
          source === "Pica" && record === options.picaDuplicateRecord
            ? record - 1
            : record,
        ),
        epoch,
      );
    const accounts = sources.map((source) =>
      options.disconnected || options.expired
        ? {
            ...makeAccount(source),
            sessionId: null,
            accountId: null,
            displayName: null,
            state: options.expired
              ? ("expired" as const)
              : ("disconnected" as const),
            remembered: Boolean(options.expired),
            errorCode: options.expired ? "SESSION_EXPIRED" : null,
          }
        : makeAccount(source),
    );
    const hooks: Hooks = (window.sourceTest = {
      accounts,
      calls: [],
      loginStarted: false,
      jmHeld: false,
      picaHeld: false,
      matchHeld: false,
      matches: JSON.parse(
        localStorage.getItem("synthetic.source.matches") ??
          '{"revision":0,"pairs":[]}',
      ) as SourceMatchesSnapshot,
      booklists: { revision: 0, value: { version: 1, lists: [] } },
      preferences: {
        revision: 0,
        value: {
          version: 1,
          appearance: {
            backgroundMode: "B",
            density: 7,
            backgroundImage: null,
            backgroundName: null,
          },
          resources: {
            profile: "balanced",
            simultaneousWorks: 2,
            imageRequests: 4,
          },
        },
      },
      following: {
        JM: {
          source: "JM",
          sessionId: "synthetic-JM-1",
          revision: 0,
          works: [],
          authors:
            options.authorPolicyResults || options.reviewedWorkCredits
              ? ["Mint"]
              : [],
        },
        Pica: {
          source: "Pica",
          sessionId: "synthetic-Pica-1",
          revision: 0,
          works: [],
          authors: options.authorSearchResults ? ["Mint"] : [],
        },
      },
    });
    let jmHoldUsed = false;
    let picaHoldUsed = false;
    let expiryUsed = false;
    let libraryConfirmed = false;
    const catalogs = new Map<
      string,
      {
        snapshot: CatalogSnapshot | null;
        completeSnapshot: CatalogSnapshot | null;
      }
    >();
    let pageFailureUsed = false;
    let favoriteFailureUsed = false;
    let followConflictUsed = false;
    const remoteFavorite: Record<Source, boolean> = { JM: false, Pica: false };
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, raw: Record<string, unknown> = {}) => {
          const source = raw.source as Source;
          // Intentionally whitelist fields: never retain username, password or raw IPC args.
          hooks.calls.push({
            command,
            source,
            sessionId: raw.sessionId as string | null | undefined,
            kind: raw.kind as string | undefined,
            page: raw.page as number | undefined,
            workId: raw.workId as string | undefined,
            desired: raw.desired as boolean | undefined,
            expectedRevision: raw.expectedRevision as number | undefined,
            query: raw.query as string | undefined,
            reverse: raw.reverse as boolean | undefined,
          });
          if (command === "read_preferences") return clone(hooks.preferences);
          if (command === "jm_download_read") return { revision: 0, tasks: [] };
          if (
            command === "download_inventory_read" &&
            options.authorSearchResults
          )
            return {
              revision: 0,
              libraryRevision: 0,
              rootId: "a".repeat(64),
              items: [],
            };
          if (command === "source_matches_read") return clone(hooks.matches);
          if (command === "source_matches_confirm") {
            if (raw.revision !== hooks.matches.revision)
              throw { code: "REVISION_CONFLICT" };
            const jm = raw.jm as SourceMatchWork,
              pica = raw.pica as SourceMatchWork;
            if (
              hooks.matches.pairs.some(
                (pair) =>
                  pair.jm.workId === jm.workId ||
                  pair.pica.workId === pica.workId,
              )
            )
              throw { code: "SOURCE_MATCH_CONFLICT" };
            hooks.matches = {
              revision: hooks.matches.revision + 1,
              pairs: [
                ...hooks.matches.pairs,
                {
                  id: "a".repeat(64),
                  jm,
                  pica,
                  confirmedAt: 1800000000000,
                  evidence: "manual",
                },
              ],
            };
            localStorage.setItem(
              "synthetic.source.matches",
              JSON.stringify(hooks.matches),
            );
            return clone(hooks.matches);
          }
          if (command === "source_matches_unlink") {
            if (raw.revision !== hooks.matches.revision)
              throw { code: "REVISION_CONFLICT" };
            hooks.matches = {
              revision: hooks.matches.revision + 1,
              pairs: hooks.matches.pairs.filter(
                (pair) => pair.id !== raw.pairId,
              ),
            };
            localStorage.setItem(
              "synthetic.source.matches",
              JSON.stringify(hooks.matches),
            );
            return clone(hooks.matches);
          }
          if (command === "read_booklists") return clone(hooks.booklists);
          if (
            command === "library_read" ||
            command === "library_reconcile" ||
            command === "library_associate"
          ) {
            if (command === "library_associate") libraryConfirmed = true;
            const configured =
              options.crossSourcePC || options.authorSearchResults;
            const snapshot = {
              revision: options.crossSourcePC ? 1 : 0,
              rootId: configured ? "a".repeat(64) : null,
              rootPath: configured ? "C:\\Synthetic PC Library" : null,
              generation: configured ? 1 : 0,
              phase: configured ? "complete" : "idle",
              freshness: configured ? "live" : "none",
              items: options.crossSourcePC
                ? [
                    {
                      id: "e".repeat(64),
                      relativePath: "[合成作者] 电脑作品.zip",
                      fileName: "[合成作者] 电脑作品.zip",
                      format: "zip",
                      title: options.libraryCandidate
                        ? "[合成作者] 合成验收 JM 作品 123 账号1.zip"
                        : "合成电脑作品",
                      links: libraryConfirmed
                        ? [
                            {
                              reference: { source: "JM", workId: "123" },
                              evidence: "manual",
                              linkedAt: 1800000000000,
                            },
                          ]
                        : [],
                      authors: ["合成作者"],
                      description: null,
                      tags: [],
                      bytes: 4096,
                      modifiedAt: 1800000000000,
                      pageCount: 20,
                      coverAvailable: false,
                      state: "indexed",
                      errorCode: null,
                      sourceRef: {
                        source: "Pica",
                        workId: "0123456789abcdef01234567",
                      },
                      identityEvidence: "metadata",
                    },
                  ]
                : [],
              visited: options.crossSourcePC ? 1 : 0,
              skipped: 0,
              updatedAt: null,
              errorCode: null,
            };
            return command === "library_reconcile"
              ? {
                  snapshot,
                  linked: 0,
                  examined: (raw.works as unknown[]).length,
                }
              : snapshot;
          }
          if (command === "phone_library_read")
            return {
              revision: options.crossSourcePhone ? 1 : 0,
              importedNames: [],
              importedAt: null,
              importFileName: null,
              manualEntries: options.crossSourcePhone
                ? [
                    {
                      id: "e".repeat(64),
                      name: "合成手机作品",
                      reference: {
                        source: "Pica",
                        workId: "0123456789abcdef01234567",
                      },
                      markedAt: 1800000000000,
                    },
                  ]
                : [],
            };
          if (
            command === "write_preferences" ||
            command === "write_booklists"
          ) {
            const target =
              command === "write_preferences"
                ? hooks.preferences
                : hooks.booklists;
            if (raw.expectedRevision !== target.revision)
              throw { code: "REVISION_CONFLICT" };
            if (command === "write_preferences")
              hooks.preferences = {
                revision: target.revision + 1,
                value: clone(raw.value as WorkbenchPreferences),
              };
            else
              hooks.booklists = {
                revision: target.revision + 1,
                value: clone(raw.value as BooklistsDocument),
              };
            return clone(
              command === "write_preferences"
                ? hooks.preferences
                : hooks.booklists,
            );
          }
          if (command === "source_accounts") return clone(hooks.accounts);
          if (command === "source_login") {
            hooks.loginStarted = true;
            if (options.holdLogin)
              await new Promise<void>((resolve, reject) => {
                hooks.releaseLogin = (success) =>
                  success ? resolve() : reject({ code: "LOGIN_REJECTED" });
              });
            const previous = hooks.accounts.find(
              (item) => item.source === source,
            )!;
            const epoch =
              Number(previous.sessionId?.split("-").at(-1) ?? 1) + 1;
            const next = {
              ...makeAccount(source, epoch),
              remembered: Boolean(raw.remember),
            };
            hooks.accounts = hooks.accounts.map((item) =>
              item.source === source ? next : item,
            );
            hooks.following[source] = {
              source,
              sessionId: next.sessionId!,
              revision: 0,
              works: [],
              authors: [],
            };
            return clone(next);
          }
          if (command === "source_logout") {
            const next: AccountSummary = {
              source,
              sessionId: null,
              accountId: null,
              displayName: null,
              state: "disconnected",
              remembered: false,
              errorCode: null,
            };
            hooks.accounts = hooks.accounts.map((item) =>
              item.source === source ? next : item,
            );
            return clone(next);
          }
          const scope = { source, sessionId: raw.sessionId as string };
          if (command === "source_author_policy")
            return {
              ...scope,
              revision: 0,
              author: raw.author,
              queries: options.authorPolicyResults
                ? [source === "JM" ? "Mentha～" : "Mentha Name", "Mentha"]
                : [raw.author],
              verifiedAliases: options.authorPolicyResults ? ["Mentha"] : [],
              exactCredits: [],
              workCredits: options.reviewedWorkCredits
                ? [
                    {
                      workId:
                        source === "Pica" ? "201".padStart(24, "0") : "201",
                      expectedAuthors: ["Incorrect credit"],
                      correctedAuthors: ["Harbor Studio (Mint)"],
                    },
                    {
                      workId:
                        source === "Pica" ? "204".padStart(24, "0") : "204",
                      expectedAuthors: ["Guest、Mint"],
                      correctedAuthors: ["Different Writer"],
                    },
                  ]
                : [],
              queryFingerprint: "a".repeat(64),
            };
          if (command === "source_catalog") {
            const key =
              source +
              ":" +
              scope.sessionId +
              ":" +
              String(raw.folderId) +
              ":" +
              String(raw.reverse);
            let cached = options.cacheSnapshot ?? null;
            if (source === "Pica" && options.picaCachePage && !raw.reverse) {
              const count = options.collectionCount!;
              const length = Math.min(options.picaCachePage * 20, count);
              cached = {
                items: Array.from({ length }, (_, index) =>
                  makeCollectionWork(source, index + 1),
                ),
                page: options.picaCachePage,
                total: count,
                pages: Math.ceil(count / 20),
                hasMore: length < count,
                folders: [],
                complete: length === count,
                updatedAt: Date.now() - 1000,
                firstPageIds: Array.from(
                  { length: Math.min(20, count) },
                  (_, index) => makeCollectionWork(source, index + 1).workId,
                ),
                pageEnds: Array.from(
                  { length: options.picaCachePage },
                  (_, index) => Math.min((index + 1) * 20, count),
                ),
              };
            }
            const current = catalogs.get(key) ?? {
              snapshot: cached,
              completeSnapshot: cached?.complete ? cached : null,
            };
            if (raw.action === "write") {
              current.snapshot = clone(raw.snapshot as CatalogSnapshot);
              if (current.snapshot.complete)
                current.completeSnapshot = current.snapshot;
              catalogs.set(key, current);
            }
            return { ...scope, ...clone(current) };
          }
          if (command === "source_query") {
            if (
              options.holdMatchDetail &&
              source === "Pica" &&
              raw.kind === "detail"
            ) {
              hooks.matchHeld = true;
              await new Promise<void>((resolve) => {
                hooks.releaseMatch = resolve;
              });
            }
            if (options.expireJM && source === "JM" && !expiryUsed) {
              expiryUsed = true;
              hooks.accounts = hooks.accounts.map((account) =>
                account.source === "JM"
                  ? {
                      ...account,
                      sessionId: null,
                      state: "expired",
                      errorCode: "SESSION_EXPIRED",
                    }
                  : account,
              );
              throw { code: "SESSION_EXPIRED" };
            }
            const epoch = Number(scope.sessionId.split("-").at(-1));
            const pageNumber = raw.page as number;
            if (
              options.isolatedListing &&
              ["search", "favorites"].includes(raw.kind as string)
            ) {
              return {
                ...scope,
                items:
                  pageNumber === 2
                    ? []
                    : [makeWork(source, String(pageNumber), epoch)],
                issues:
                  pageNumber === 2
                    ? [
                        {
                          page: 2,
                          index: 1,
                          workId: null,
                          code: "SOURCE_ITEM_INVALID",
                        },
                      ]
                    : [],
                page: pageNumber,
                total: 3,
                pages: 3,
                hasMore: pageNumber < 3,
                folders: [],
              };
            }
            if (options.authorSearchResults && raw.kind === "search") {
              if (
                options.workDates &&
                options.partial &&
                pageNumber === 2 &&
                !pageFailureUsed
              ) {
                pageFailureUsed = true;
                throw { code: "SOURCE_TIMEOUT" };
              }
              const candidates: SourceWork[] = [
                { ...makeWork(source, "202"), authors: ["Mintleaf"] },
                {
                  ...makeWork(source, "203"),
                  title: "Mint 合成标题命中",
                  authors: [],
                },
                { ...makeWork(source, "205"), authors: ["Other Writer"] },
                {
                  ...makeWork(source, "201"),
                  authors: ["Harbor Studio (Mint)"],
                },
                { ...makeWork(source, "204"), authors: ["Guest、Mint"] },
              ];
              if (options.authorPolicyResults) {
                candidates[3].authors = ["Harbor Studio (Mentha)"];
                candidates[4].authors = ["Guest、Mentha"];
                if (raw.query === "Mentha")
                  candidates.push({
                    ...makeWork(source, "206"),
                    authors: ["Mentha"],
                  });
              }
              if (options.reviewedWorkCredits && source === "Pica")
                candidates.forEach((work) => {
                  work.workId = work.workId.padStart(24, "0");
                });
              if (options.reviewedWorkCredits)
                candidates[3].authors = ["Incorrect credit"];
              if (options.workDates)
                candidates.forEach((work, index) => {
                  work.sourceUpdatedAt = [
                    "2026-09-21",
                    null,
                    "2026-09-18",
                    "2026-09-15",
                    "2026-09-20",
                  ][index];
                });
              return {
                ...scope,
                items:
                  pageNumber === 1
                    ? candidates.slice(0, 3)
                    : candidates.slice(3),
                page: pageNumber,
                total: candidates.length,
                pages: 2,
                hasMore: pageNumber < 2,
                folders: [],
              };
            }
            if (
              options.holdJM &&
              source === "JM" &&
              raw.kind === "favorites" &&
              !jmHoldUsed
            ) {
              jmHoldUsed = true;
              hooks.jmHeld = true;
              await new Promise<void>((resolve) => {
                hooks.releaseJM = resolve;
              });
            }
            if (options.partial && pageNumber === 2 && !pageFailureUsed) {
              pageFailureUsed = true;
              throw { code: "SOURCE_TIMEOUT" };
            }
            if (source === "Pica" && raw.kind === "favorites") {
              if (pageNumber === options.holdPicaPage && !picaHoldUsed) {
                picaHoldUsed = true;
                hooks.picaHeld = true;
                await new Promise<void>((resolve) => {
                  hooks.releasePica = resolve;
                });
              }
              if (
                pageNumber === options.picaPageFailureOnce &&
                !pageFailureUsed
              ) {
                pageFailureUsed = true;
                throw {
                  code: options.picaPageFailureCode ?? "SOURCE_RATE_LIMITED",
                };
              }
            }
            const work = makeWork(
              source,
              raw.kind === "detail"
                ? String(raw.query)
                : pageNumber === 2
                  ? "456"
                  : "123",
              epoch,
            );
            if (raw.kind === "detail") {
              work.favorite = remoteFavorite[source];
              work.tags = options.detailTags?.[work.workId] ?? work.tags;
            }
            if (
              options.reviewedWorkCredits &&
              raw.kind === "detail" &&
              raw.query ===
                (source === "Pica" ? "201".padStart(24, "0") : "201")
            )
              work.authors = [
                hooks.changedDetailCredit
                  ? "Website now changed credit"
                  : "Incorrect credit",
              ];
            if (
              options.workDates &&
              raw.kind === "detail" &&
              raw.query === "203"
            )
              work.sourceUpdatedAt = "2026-09-22";
            const resultItems =
              options.coverCount && raw.kind !== "detail"
                ? Array.from({ length: options.coverCount }, (_, index) => ({
                    ...makeWork(source, String(index + 100)),
                    coverAvailable: true,
                  }))
                : [work];
            if (options.collectionCount && raw.kind === "favorites") {
              const count = options.collectionCount;
              const offset = (pageNumber - 1) * 20;
              return {
                ...scope,
                items: Array.from(
                  { length: Math.min(20, count - offset) },
                  (_, i) =>
                    makeCollectionWork(
                      source,
                      raw.reverse ? count - offset - i : offset + i + 1,
                      epoch,
                    ),
                ),
                page: pageNumber,
                total: count,
                pages: Math.ceil(count / 20),
                hasMore: offset + 20 < count,
                folders:
                  source === "JM"
                    ? [{ id: "folder-one", name: "合成收藏夹", count }]
                    : [],
              };
            }
            return {
              ...scope,
              items: resultItems,
              page: pageNumber,
              total:
                options.partial && pageNumber === 1
                  ? null
                  : options.partial
                    ? 2
                    : (options.coverCount ?? 1),
              pages: null,
              hasMore: Boolean(options.partial && pageNumber === 1),
              folders:
                source === "JM"
                  ? [{ id: "folder-one", name: "合成收藏夹", count: null }]
                  : [],
            };
          }
          if (command === "source_cover") {
            hooks.coverActive = (hooks.coverActive ?? 0) + 1;
            hooks.coverMax = Math.max(hooks.coverMax ?? 0, hooks.coverActive);
            if (options.coverDelay)
              await new Promise((resolve) =>
                setTimeout(resolve, options.coverDelay),
              );
            hooks.coverActive--;
            if (
              options.coverFailureOnce &&
              source === "Pica" &&
              raw.workId === "100" &&
              !coverFailureUsed
            ) {
              coverFailureUsed = true;
              throw {
                code: "SOURCE_COVER_ACCESS_DENIED",
                message: "SECRET URL TOKEN",
              };
            }
            return {
              ...scope,
              workId: raw.workId,
              dataUrl:
                options.coverCount || options.collectionCover
                  ? "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
                  : null,
            };
          }
          if (command === "source_favorite") {
            remoteFavorite[source] = Boolean(raw.desired);
            if (options.unknownFavorite && !favoriteFailureUsed) {
              favoriteFailureUsed = true;
              throw { code: "FAVORITE_OUTCOME_UNKNOWN" };
            }
            return {
              ...scope,
              workId: raw.workId,
              favorite: remoteFavorite[source],
              changed: true,
              verified: true,
            };
          }
          if (command === "source_following")
            return clone({ ...hooks.following[source], ...scope });
          if (command === "source_follow") {
            const previous = hooks.following[source];
            if (options.followConflict && !followConflictUsed) {
              followConflictUsed = true;
              hooks.following[source] = {
                ...previous,
                revision: previous.revision + 1,
                authors: ["合成外部作者"],
              };
              throw { code: "REVISION_CONFLICT" };
            }
            if (raw.expectedRevision !== previous.revision)
              throw { code: "REVISION_CONFLICT" };
            const value = raw.value as string;
            const next = {
              ...previous,
              ...scope,
              revision: previous.revision + 1,
            };
            if (raw.kind === "author")
              next.authors = raw.desired
                ? [...new Set([...next.authors, value])]
                : next.authors.filter((item) => item !== value);
            else
              next.works = raw.desired
                ? [
                    ...next.works.filter((item) => item.workId !== value),
                    { workId: value, title: makeWork(source, value).title },
                  ]
                : next.works.filter((item) => item.workId !== value);
            hooks.following[source] = next;
            return clone(next);
          }
          throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
        },
      },
    });
  }, options);
  await page.goto("/");
}
async function openFavorites(page: Page) {
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("source-workbench")).toBeVisible();
}

test("both source lists show tag-only language badges without detail requests and keep selection reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    collectionCount: 5,
    workTags: {
      "1": ["中文"],
      "2": ["生肉"],
      "3": ["日漫", "合成汉化组"],
      "4": ["中文", "日本語"],
      "5": ["日本語"],
    },
    detailTags: { "3": ["中文"] },
  });
  await openFavorites(page);
  await mkdir("visual-evidence", { recursive: true });
  for (const source of ["JM", "Pica"] as const) {
    await page.getByTestId("source-tab-" + source).click();
    for (const [workId, label, kind] of [
      ["1", "已汉化", "chinese"],
      ["2", "生肉", "untranslated"],
      ["3", "未知", "unknown"],
      ["4", "未知", "unknown"],
      ["5", "生肉", "untranslated"],
    ]) {
      const badge = page
        .getByTestId(`source-card-${source}:${workId}`)
        .getByTestId("source-language-badge");
      await expect(badge).toHaveText(label);
      await expect(badge).toHaveAttribute("data-language-kind", kind);
      await expect(badge).toHaveAttribute("data-language-context", "source");
      await expect(badge).toHaveAttribute("aria-label", new RegExp(label));
    }
    await expect(
      page
        .getByTestId(`source-card-${source}:4`)
        .getByTestId("source-language-badge"),
    ).toHaveAttribute("title", /冲突/);
    expect(
      await page.evaluate(() =>
        window.sourceTest.calls.filter(
          (call) => call.command === "source_query" && call.kind === "detail",
        ),
      ),
    ).toEqual([]);
    await page.getByTestId("source-toggle-selection").click();
    await page.getByTestId(`source-select-${source}:1`).check();
    await page.getByTestId(`source-select-${source}:2`).check();
    await expect(page.getByTestId("source-selection-bar")).toContainText(
      "已选 2 部",
    );
    await page.getByTestId(`source-card-${source}:1`).scrollIntoViewIfNeeded();
    for (const workId of ["1", "2", "3", "4", "5"]) {
      await expect(
        page
          .getByTestId(`source-card-${source}:${workId}`)
          .getByTestId("source-language-badge"),
      ).toBeInViewport({ ratio: 1 });
    }
    await expect(page.getByTestId(`source-select-${source}:1`)).toBeInViewport({
      ratio: 1,
    });
    await expect(page.getByTestId(`source-select-${source}:2`)).toBeInViewport({
      ratio: 1,
    });
    const badge = await page
      .getByTestId(`source-card-${source}:1`)
      .getByTestId("source-language-badge")
      .boundingBox();
    const selection = await page
      .getByTestId(`source-select-${source}:1`)
      .boundingBox();
    expect(badge).not.toBeNull();
    expect(selection).not.toBeNull();
    expect(
      badge!.x + badge!.width <= selection!.x ||
        selection!.x + selection!.width <= badge!.x ||
        badge!.y + badge!.height <= selection!.y ||
        selection!.y + selection!.height <= badge!.y,
      "the language badge must not cover the selection control",
    ).toBe(true);
    await page.screenshot({
      path: `visual-evidence/source-language-${source.toLowerCase()}-selection.png`,
    });
    await page.getByTestId("source-toggle-selection").click();
    if (source === "JM")
      await page.setViewportSize({ width: 1280, height: 900 });
  }

  await page.getByTestId("source-open-Pica:3").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(
    page.getByTestId("source-detail").getByTestId("source-language-badge"),
  ).toHaveText("已汉化");
  await page.getByTestId("source-detail-back").click();
  await expect(
    page.getByTestId("source-card-Pica:3").getByTestId("source-language-badge"),
  ).toHaveText("已汉化");
  await expect(
    page.getByTestId("source-card-Pica:4").getByTestId("source-language-badge"),
  ).toHaveText("未知");
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter(
          (call) => call.command === "source_query" && call.kind === "detail",
        )
        .map((call) => [call.source, call.query]),
    ),
  ).toEqual([["Pica", "3"]]);
  await page.getByTestId("source-tab-JM").click();
  await expect(
    page.getByTestId("source-card-JM:3").getByTestId("source-language-badge"),
  ).toHaveText("未知");
  await openAccounts(page);
  await page.getByTestId("account-logout-Pica").click();
  await page.getByTestId("account-connect-Pica").click();
  await page.getByTestId("account-username").fill("synthetic-user");
  await page.getByTestId("account-password").fill("fixture-only-password");
  await page.getByTestId("account-login-submit").click();
  await expect(page.getByTestId("account-Pica")).toContainText(
    "合成验收账号 Pica 2",
  );
  await page.getByTestId("account-favorites-Pica").click();
  await expect(
    page.getByTestId("source-card-Pica:3").getByTestId("source-language-badge"),
  ).toHaveText("未知");
});

test("cached source languages display immediately without reading details or guessing from a title", async ({
  page,
}) => {
  const items: SourceWork[] = [[], ["中文"], ["日本語"]].map((tags, index) => ({
    source: "JM",
    workId: String(index + 1),
    title: "[汉化] 日本語 合成旧收藏 " + (index + 1),
    authors: ["合成作者"],
    description: "合成汉化组说明，不属于语言标签",
    tags,
    favorite: true,
    chapterCount: null,
    pageCount: null,
    coverAvailable: false,
  }));
  await installMock(page, {
    collectionCount: items.length,
    cacheSnapshot: {
      items,
      page: 1,
      total: items.length,
      pages: 1,
      hasMore: false,
      folders: [],
      complete: true,
      updatedAt: 1800000000000,
      firstPageIds: items.map((work) => work.workId),
    },
  });
  await openFavorites(page);
  for (const [index, label] of ["未知", "已汉化", "生肉"].entries()) {
    await expect(
      page
        .getByTestId("source-card-JM:" + (index + 1))
        .getByTestId("source-language-badge"),
    ).toHaveText(label);
  }
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter(
        (call) => call.command === "source_query" && call.kind !== "favorites",
      ),
    ),
  ).toEqual([]);
});

test("favorites status filtering clears hidden selection and offers a direct full-read action", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { collectionCount: 65, collectionAuthors: true });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:1")).toBeVisible();
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-Pica:1").check();
  await page.getByTestId("source-filter-owned").click();
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("source-filter-count")).toContainText(
    "当前显示 0 部",
  );
  await page.getByTestId("collection-read-all").click();
  await expect(page.getByTestId("collection-progress")).toContainText(
    "已读取全部收藏 · 已读取 65 / 65",
  );
  expect(await picaFavoritePages(page, false)).toEqual([1, 2, 3, 4]);
  await page.getByTestId("source-filter-unknown").click();
  await expect(page.getByTestId("source-filter-count")).toContainText(
    "当前显示 65 部",
  );
  await page.getByTestId("source-tab-JM").click();
  await expect(page.getByTestId("source-filter-all")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/favorites-usability.png" });
});

test("disconnected source opens account settings; pending login clears secret and preserves retry input", async ({
  page,
}) => {
  await installMock(page, { disconnected: true, holdLogin: true });
  await openFavorites(page);
  await page
    .getByTestId("source-account-required")
    .getByRole("button", { name: "前往账号设置" })
    .click();
  await expect(page.getByTestId("source-account-settings")).toBeVisible();
  await connectJM(page);
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.loginStarted))
    .toBe(true);
  await expect(page.getByTestId("account-password")).toHaveValue("");
  await expect(page.getByTestId("account-login-submit")).toBeDisabled();
  await expect(page.getByTestId("account-JM")).not.toContainText("已连接");
  await page.evaluate(() => window.sourceTest.releaseLogin!(false));
  await expect(
    page.getByTestId("account-login-dialog").getByRole("alert"),
  ).toContainText("登录未成功");
  await expect(page.getByTestId("account-username")).toHaveValue(
    "synthetic-user",
  );
  await expect(page.getByTestId("account-password")).toHaveValue("");
  await page.getByTestId("account-password").fill("fixture-only-password");
  await page.getByTestId("account-login-submit").click();
  await expect(page.getByTestId("account-login-submit")).toBeDisabled();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.sourceTest.calls.filter(
            (call) => call.command === "source_login",
          ).length,
      ),
    )
    .toBe(2);
  await page.evaluate(() => window.sourceTest.releaseLogin!(true));
  await expect(page.getByTestId("account-login-dialog")).toBeHidden();
  await expect(page.getByTestId("account-JM")).toContainText(
    "合成验收账号 JM 2",
  );
  await expect(page.getByTestId("account-Pica")).toContainText("未连接");
  expect(
    await page.evaluate(
      () =>
        JSON.stringify(localStorage) + JSON.stringify(window.sourceTest.calls),
    ),
  ).not.toContain("fixture-only-password");
  await page.getByTestId("account-favorites-JM").click();
  await expect(page.getByTestId("source-card-JM:123")).toContainText(
    "合成验收 JM",
  );
});

test("expired remembered account without a usable session can forget its saved login", async ({
  page,
}) => {
  await installMock(page, { expired: true });
  await openAccounts(page);
  await expect(page.getByTestId("account-favorites-JM")).toHaveCount(0);
  await expect(page.getByTestId("account-logout-JM")).toHaveText(
    "忘记保存的会话",
  );
  await page.getByTestId("account-logout-JM").click();
  await expect(page.getByTestId("account-JM")).toContainText("未连接");
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter(
        (call) => call.command === "source_logout",
      ),
    ),
  ).toEqual([{ command: "source_logout", source: "JM", sessionId: null }]);
});

test("switching sources discards a late previous response and clears only temporary source selection", async ({
  page,
}) => {
  await installMock(page, { holdJM: true });
  await openFavorites(page);
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.jmHeld))
    .toBe(true);
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-card-Pica:123")).toContainText(
    "合成验收 Pica",
  );
  await page.evaluate(() => window.sourceTest.releaseJM!());
  await expect(page.getByTestId("source-card-JM:123")).toHaveCount(0);
  await expect(page.getByTestId("source-folder")).toHaveCount(0);
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-Pica:123").check();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 1 部",
  );
  await page.getByTestId("source-tab-JM").click();
  await expect(page.getByTestId("source-card-JM:123")).toBeVisible();
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("source-select-JM:123")).toHaveCount(0);
});

test("logging into a new account rejects metadata from an outstanding old session", async ({
  page,
}) => {
  await installMock(page, { holdJM: true });
  await openFavorites(page);
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.jmHeld))
    .toBe(true);
  await openAccounts(page);
  await connectJM(page);
  await expect(page.getByTestId("account-login-dialog")).toBeHidden();
  await expect(page.getByTestId("account-JM")).toContainText(
    "合成验收账号 JM 2",
  );
  await page.evaluate(() => window.sourceTest.releaseJM!());
  await page.getByTestId("account-favorites-JM").click();
  await expect(page.getByTestId("source-card-JM:123")).toContainText("账号2");
  await expect(page.getByTestId("source-workbench")).not.toContainText(
    "作品 123 账号1",
  );
});

test("partial pagination keeps unknown totals and retained data when the next page fails", async ({
  page,
}) => {
  await installMock(page, { partial: true });
  await openFavorites(page);
  await expect(page.getByTestId("source-card-JM:123")).toBeVisible();
  await expect(page.getByTestId("source-completeness")).toContainText(
    "尚未读全",
  );
  await expect(page.getByTestId("source-workbench")).toContainText("总数未知");
  await page.getByTestId("collection-sentinel").scrollIntoViewIfNeeded();
  await expect(page.getByTestId("collection-retry")).toBeVisible();
  await expect(page.getByTestId("source-card-JM:123")).toBeVisible();
  await expect(page.getByTestId("source-empty")).toHaveCount(0);
  await expect(
    page.getByTestId("collection-sentinel").getByRole("alert"),
  ).toBeVisible();
  await page.getByTestId("collection-retry").click();
  await expect(page.getByTestId("source-grid").locator("article")).toHaveCount(
    2,
  );
  await expect(page.getByTestId("source-completeness")).toContainText(
    "完整范围",
  );
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter(
          (call) =>
            call.command === "source_query" && call.kind === "favorites",
        )
        .map((call) => call.page),
    ),
  ).toEqual([1, 2, 2]);
});

test("unknown metadata and uncertain favorite writes never imply zero counts or success", async ({
  page,
}) => {
  await installMock(page, { unknownFavorite: true });
  await detail(page);
  await expect(
    page.getByTestId("source-detail").locator(".source-facts dd"),
  ).toHaveText(["更新时间未知", "未知", "未知", "尚未设置漫画库"]);
  // This entry can guide the user to PC-folder selection. Unknown source counts
  // still cannot create or start a task without a separate native plan/confirm.
  await expect(page.getByTestId("source-download")).toBeEnabled();
  await expect(page.getByTestId("source-detail")).toContainText(
    "作者资料未取得",
  );
  await page.getByTestId("source-favorite").click();
  await expect(
    page.getByTestId("source-detail").getByRole("alert"),
  ).toContainText("结果未确认");
  await expect(page.getByTestId("source-favorite")).toBeDisabled();
  await expect(page.getByTestId("source-favorite")).toHaveText(
    "收藏状态待核对",
  );
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) => call.command === "source_favorite",
        ).length,
    ),
  ).toBe(1);
  await page.getByTestId("source-detail-reload").click();
  await expect(page.getByTestId("source-favorite")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("source-favorite")).toBeEnabled();
  await page.getByTestId("source-favorite").click();
  await expect(page.getByTestId("source-favorite")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter((call) => call.command === "source_favorite")
        .map((call) => call.desired),
    ),
  ).toEqual([true, false]);
});

test("following conflict reload keeps the requested action for explicit retry and retains external authors", async ({
  page,
}) => {
  await installMock(page, { followConflict: true });
  await detail(page);
  await page.getByTestId("source-follow-work").click();
  await expect(
    page.getByTestId("source-detail").getByRole("alert"),
  ).toContainText("另一处改变");
  await expect(page.getByTestId("source-follow-work")).toHaveText("关注作品");
  await page.getByTestId("source-following-reload").click();
  await expect(page.getByTestId("source-following-retry")).toBeEnabled();
  expect(
    await page.evaluate(() => window.sourceTest.following.JM.works),
  ).toEqual([]);
  await page.getByTestId("source-following-retry").click();
  await expect(page.getByTestId("source-follow-work")).toHaveText(
    "取消作品关注",
  );
  expect(
    await page.evaluate(() => window.sourceTest.following.JM.authors),
  ).toEqual(["合成外部作者"]);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter((call) => call.command === "source_follow")
        .map((call) => call.expectedRevision),
    ),
  ).toEqual([0, 1]);
  await page.getByTestId("nav-authors").click();
  await expect(page.getByTestId("source-authors")).toContainText(
    "合成外部作者",
  );
  await expect(page.getByTestId("source-workbench")).toContainText(
    "手动查看与检查",
  );
});

test("the next screen is prefetched without scrolling or loading the whole catalog", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { coverCount: 120, coverDelay: 80 });
  await openFavorites(page);
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  const candidate = await page.getByTestId("source-grid").evaluate((grid) => {
    const main = grid.closest("main")!;
    const edge = main.getBoundingClientRect().bottom;
    return [
      ...grid.querySelectorAll<HTMLElement>(
        '[data-testid^="source-cover-JM:"]',
      ),
    ].find((card) => {
      const top = card.getBoundingClientRect().top;
      return top > edge + 80 && top < edge + 550;
    })?.dataset.testid;
  });
  expect(candidate).toBeTruthy();
  const card = page.getByTestId(candidate!);
  await expect(card.locator("img")).toHaveCount(1);
  await expect(card).not.toBeInViewport();
  expect(
    await page.getByRole("main").evaluate((element) => element.scrollTop),
  ).toBe(0);
  const id = candidate!.split(":")[1];
  await card.scrollIntoViewIfNeeded();
  await expect(card.locator("img")).toBeVisible();
  expect(
    await page.evaluate(
      (id) =>
        window.sourceTest.calls.filter(
          (call) => call.command === "source_cover" && call.workId === id,
        ).length,
      id,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(() => window.sourceTest.coverMax),
  ).toBeLessThanOrEqual(4);
});

test("slow source covers use four bounded slots without fetching the full catalog", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { coverCount: 120, coverDelay: 80 });
  await openFavorites(page);
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.coverMax))
    .toBe(4);
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) => call.command === "source_cover",
        ).length,
    ),
  ).toBeLessThan(35);
  await page.getByTestId("source-grid").evaluate((element) => {
    const main = element.closest("main")!;
    main.scrollTop = main.scrollHeight;
  });
  await expect(
    page.getByTestId("source-cover-JM:219").locator("img"),
  ).toBeVisible();
  expect(await page.evaluate(() => window.sourceTest.coverMax)).toBe(4);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.some(
        (call) => call.command === "source_cover" && call.workId === "160",
      ),
    ),
  ).toBe(false);
});

test("source covers release offscreen images but reuse successful session thumbnails after scrolling, settings and detail", async ({
  page,
}) => {
  await installMock(page, { coverCount: 120 });
  await openFavorites(page);
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  await page.getByTestId("source-workbench").evaluate((element) => {
    const main = element.closest("main")!;
    main.scrollTop = main.scrollHeight;
  });
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("source-cover-JM:219").locator("img"),
  ).toBeVisible();
  await openAccounts(page);
  await expect(page.getByTestId("source-workbench").locator("img")).toHaveCount(
    0,
  );
  await page.getByTestId("account-favorites-JM").click();
  // Returning from settings restores the saved bottom-of-list anchor on a
  // rendering frame. Wait for that observable position before scrolling as a
  // user; an immediate scrollTop assignment can be overwritten by restoration.
  await expect(
    page.getByTestId("source-cover-JM:219").locator("img"),
  ).toBeInViewport();
  const scrollArea = page.getByRole("main");
  await scrollArea.hover();
  await page.mouse.wheel(
    0,
    -(await scrollArea.evaluate((element) => element.scrollHeight)),
  );
  await expect
    .poll(() => scrollArea.evaluate((element) => element.scrollTop))
    .toBe(0);
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) =>
            call.command === "source_cover" &&
            call.source === "JM" &&
            call.workId === "100",
        ).length,
    ),
  ).toBe(1);
  await page.getByTestId("source-open-JM:100").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) =>
            call.command === "source_cover" &&
            call.source === "JM" &&
            call.workId === "100",
        ).length,
    ),
  ).toBe(1);
  await openAccounts(page);
  await page.getByTestId("account-logout-JM").click();
  await expect(page.getByTestId("account-connect-JM")).toBeVisible();
  await connectJM(page);
  await expect(page.getByTestId("account-JM")).toContainText("已连接");
  await page.getByTestId("account-favorites-JM").click();
  await expect(
    page.getByTestId("source-cover-JM:100").locator("img"),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) =>
            call.command === "source_cover" &&
            call.source === "JM" &&
            call.workId === "100",
        ).length,
    ),
  ).toBe(2);
});

test("Pica cover failure shows a fixed diagnostic and explicit retry succeeds without exposing native text", async ({
  page,
}) => {
  await installMock(page, { coverCount: 120, coverFailureOnce: true });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  const cover = page.getByTestId("source-cover-Pica:100");
  await expect(cover).toContainText("SOURCE_COVER_ACCESS_DENIED");
  await expect(cover).toContainText("401/403");
  await expect(page.getByTestId("source-workbench")).not.toContainText(
    "SECRET",
  );
  await page.getByTestId("source-cover-retry").click();
  await expect(cover.locator("img")).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) =>
            call.command === "source_cover" &&
            call.source === "Pica" &&
            call.workId === "100",
        ).length,
    ),
  ).toBe(2);
});

test("an explicit expired session refreshes account state and removes the connected source view", async ({
  page,
}) => {
  await installMock(page, { expireJM: true });
  await openFavorites(page);
  await expect(page.getByTestId("source-account-required")).toContainText(
    "账号需要重新登录",
  );
  await expect(page.getByTestId("source-grid")).toHaveCount(0);
  await page
    .getByTestId("source-account-required")
    .getByRole("button", { name: "前往账号设置" })
    .click();
  await expect(page.getByTestId("account-JM")).toContainText("需要重新登录");
  await expect(page.getByTestId("account-favorites-JM")).toHaveCount(0);
  await expect(page.getByTestId("account-Pica")).toContainText("已连接");
  expect(
    await page.evaluate(
      () =>
        window.sourceTest.calls.filter(
          (call) => call.command === "source_query" && call.source === "JM",
        ).length,
    ),
  ).toBe(1);
});

test("source search stays right-aligned at baseline width and fits a narrow window", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page);
  await openFavorites(page);
  await expect(page.getByTestId("source-card-JM:123")).toBeVisible();
  const search = page.getByTestId("source-search-control");
  const geometry = () =>
    search.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const header = element.closest("header")!;
      const bounds = header.getBoundingClientRect();
      const style = getComputedStyle(header);
      return {
        left: rect.left,
        right: rect.right,
        width: rect.width,
        availableRight: bounds.right - parseFloat(style.paddingRight),
        overflow: Math.max(
          document.documentElement.scrollWidth -
            document.documentElement.clientWidth,
          header.scrollWidth - header.clientWidth,
        ),
      };
    });
  for (const navigation of ["nav-favorites", "nav-discovery", "nav-authors"]) {
    await page.getByTestId(navigation).click();
    await expect(search).toBeVisible();
    await expect
      .poll(async () => (await geometry()).width)
      .toBeGreaterThanOrEqual(478);
    expect((await geometry()).width).toBeLessThanOrEqual(482);
    expect(
      Math.abs((await geometry()).right - (await geometry()).availableRight),
    ).toBeLessThanOrEqual(2);
    expect((await geometry()).left).toBeGreaterThan(1672 / 2);
  }
  await page.getByTestId("nav-favorites").click();
  await page.getByTestId("source-open-JM:123").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail").locator("h1")).toBeVisible();
  const detailLayout = await page
    .getByTestId("source-detail")
    .evaluate((element) => {
      const cover = element
        .querySelector(".source-detail-main > .source-cover")!
        .getBoundingClientRect();
      const info = element.querySelector(".source-detail-info")!;
      const description = info
        .querySelector(".source-description")!
        .getBoundingClientRect();
      return {
        coverWidth: cover.width,
        coverHeight: cover.height,
        coverRight: cover.right,
        descriptionLeft: description.left,
      };
    });
  expect(detailLayout.coverWidth).toBeGreaterThanOrEqual(420);
  expect(detailLayout.coverWidth).toBeLessThanOrEqual(440);
  expect(detailLayout.coverHeight).toBeGreaterThanOrEqual(620);
  expect(
    detailLayout.descriptionLeft - detailLayout.coverRight,
  ).toBeGreaterThanOrEqual(48);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-search-input").fill("合成验收来源查询");
  await expect(page.getByTestId("source-search-submit")).toBeVisible();
  await expect
    .poll(async () => (await geometry()).overflow)
    .toBeLessThanOrEqual(1);
  const narrow = await geometry();
  expect(narrow.left).toBeGreaterThanOrEqual(0);
  expect(narrow.right).toBeLessThanOrEqual(390);
  expect(narrow.width).toBeGreaterThan(250);
});

for (const source of ["JM", "Pica"] as const) {
  test(`${source} source author search separates keyword hits before counting and selecting across all pages`, async ({
    page,
  }) => {
    await installMock(page, { authorSearchResults: true });
    await page.goto("/");
    await page.getByTestId("nav-discovery").click();
    await page.getByTestId("source-tab-" + source).click();
    await expect(page.getByTestId("source-query-mode")).toHaveValue("author");
    await page.getByTestId("source-search-input").fill("Mint");
    await page.getByTestId("source-search-submit").click();
    await expect(page.getByTestId("source-completeness")).toContainText(
      "已读完",
    );
    await expect(page.getByTestId("source-author-evidence")).toContainText(
      "作者作品 2 部 · 其他关键词结果 3 部",
    );
    await expect(page.getByTestId("source-filter-count")).toContainText(
      "未入库 2 部 · 当前显示 2 部",
    );
    await expect(
      page.getByTestId("source-card-" + source + ":201"),
    ).toBeVisible();
    await expect(
      page.getByTestId("source-card-" + source + ":204"),
    ).toBeVisible();
    for (const id of ["202", "203", "205"])
      await expect(
        page.getByTestId("source-card-" + source + ":" + id),
      ).toHaveCount(0);
    if (source === "Pica") {
      await mkdir("visual-evidence", { recursive: true });
      await page.screenshot({
        path: "visual-evidence/source-author-results.png",
        fullPage: true,
      });
    }
    await page.getByTestId("source-toggle-selection").click();
    await page.getByTestId("source-select-all").click();
    await expect(page.getByTestId("source-selection-bar")).toContainText(
      "已选 2 部",
    );
    await page.getByTestId("source-author-results-toggle").click();
    await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
    await expect(page.getByTestId("source-toggle-selection")).toHaveCount(0);
    await expect(
      page.getByTestId("source-grid").getByRole("checkbox"),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("source-card-" + source + ":202"),
    ).toContainText("Mintleaf");
    await expect(
      page.getByTestId("source-card-" + source + ":203"),
    ).toContainText("作者资料未取得");
    await expect(
      page.getByTestId("source-card-" + source + ":205"),
    ).toContainText("Other Writer");
    await expect(page.getByTestId("source-filter-count")).toContainText(
      "当前显示 3 部",
    );
    await expect(page.getByTestId("source-all-owned")).toHaveCount(0);
    if (source === "Pica")
      await page.screenshot({
        path: "visual-evidence/source-other-keywords.png",
        fullPage: true,
      });
    await page.getByTestId("source-author-results-toggle").click();
    await expect(page.getByTestId("source-filter-count")).toContainText(
      "当前显示 2 部",
    );
    expect(
      await page.evaluate(() =>
        window.sourceTest.calls
          .filter(
            (call) => call.command === "source_query" && call.kind === "search",
          )
          .map((call) => call.page),
      ),
    ).toEqual([1, 2]);
  });
}

test("source search date order is stable, unknown-last, persistent and independent of favorite ordering", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 1020 });
  await installMock(page, { authorSearchResults: true, workDates: true });
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-query-mode").selectOption("search");
  await page.getByTestId("source-search-input").fill("Mint");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-completeness")).toContainText("已读完");
  const cards = page.getByTestId("source-grid").locator("article");
  await expect(cards).toHaveCount(5);
  await expect(cards.first()).toHaveAttribute("data-source-work-key", "JM:202");
  await expect(cards.last()).toContainText("更新时间未知");
  await page.getByTestId("source-sort").selectOption("updated-asc");
  await expect(cards.first()).toHaveAttribute("data-source-work-key", "JM:201");
  await expect(cards.last()).toHaveAttribute("data-source-work-key", "JM:203");
  await page.getByTestId("source-filter-missing").click();
  await expect(cards).toHaveCount(5);
  await expect(page.getByTestId("source-date-sort-scope")).toContainText(
    "已读取完整范围",
  );
  await page.getByTestId("source-open-JM:201").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-updated-at")).toHaveText("2026-09-15");
  await page.getByTestId("source-detail-back").click();
  await page.getByTestId("source-open-JM:203").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-updated-at")).toHaveText("2026-09-22");
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByTestId("source-card-JM:203")).toContainText(
    "更新：2026-09-22",
  );
  await page.getByTestId("source-sort").selectOption("updated-desc");
  await expect(cards.first()).toHaveAttribute("data-source-work-key", "JM:203");
  await page.getByTestId("source-sort").selectOption("updated-asc");
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/source-search-work-dates-wide.png",
  });
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter(
          (call) => call.command === "source_query" && call.kind === "search",
        )
        .map((call) => call.page),
    ),
  ).toEqual([1, 2]);
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("source-sort")).toHaveValue("source");
  await page.reload();
  await page.getByTestId("nav-discovery").click();
  await expect(page.getByTestId("source-sort")).toHaveValue("updated-asc");
});

test("failed source pagination never labels a partial date order as a full catalog", async ({
  page,
}) => {
  await installMock(page, {
    authorSearchResults: true,
    workDates: true,
    partial: true,
  });
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-query-mode").selectOption("search");
  await page.getByTestId("source-search-input").fill("Mint");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-grid").locator("article")).toHaveCount(
    3,
  );
  await expect(page.getByTestId("source-date-sort-scope")).toContainText(
    "排序仅覆盖已读取结果",
  );
  await expect(page.getByTestId("source-date-sort-scope")).not.toContainText(
    "已读取完整范围",
  );
});

test("source searches keep later pages after isolated rows and show diagnostics separately", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { isolatedListing: true });
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-query-mode").selectOption("search");
  await page.getByTestId("source-search-input").fill("Synthetic query");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-completeness")).toContainText(
    "分页已读完，来源记录仍待核对",
  );
  await expect(page.getByTestId("source-grid").locator("article")).toHaveCount(
    2,
  );
  await expect(page.getByTestId("source-all-owned")).toHaveCount(0);
  const issues = page.getByTestId("source-issues");
  await issues.locator("summary").click();
  await expect(issues).toContainText("JM · 第 2 页 · 第 1 条 · 编号缺失");
  await expect(issues.getByRole("button")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter(
          (call) => call.command === "source_query" && call.kind === "search",
        )
        .map((call) => call.page),
    ),
  ).toEqual([1, 2, 3]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/search-isolated-records.png",
  });
});

test("favorite inversion reads past an issue-only page and excludes it from selection", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { isolatedListing: true });
  await openFavorites(page);
  await page.getByTestId("source-tab-Pica").click();
  await page.getByTestId("source-sort").selectOption("source-reverse");
  await expect(page.getByTestId("collection-progress")).toContainText(
    "收藏分页已读完，来源记录仍待核对",
  );
  await expect(page.getByTestId("source-grid").locator("article")).toHaveCount(
    2,
  );
  const issues = page.getByTestId("source-issues");
  await issues.locator("summary").click();
  await expect(issues).toContainText("哔咔 · 第 2 页 · 第 1 条 · 编号缺失");
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 2 部",
  );
  await expect(issues.getByRole("checkbox")).toHaveCount(0);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/favorites-isolated-records.png",
  });
});

test("source author mode blocks broad initials while explicit keyword mode remains available", async ({
  page,
}) => {
  await installMock(page, { authorSearchResults: true });
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-search-input").fill("P");
  await page.getByTestId("source-search-submit").click();
  await expect(
    page.getByText(/单个字母或数字无法限定作者范围，本次未发送查询/),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter(
        (c) => c.command === "source_query" && c.kind === "search",
      ),
    ),
  ).toEqual([]);
  await page.getByTestId("source-query-mode").selectOption("search");
  await page.getByTestId("source-search-input").fill("P");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-completeness")).toContainText("已读完");
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls
        .filter((c) => c.command === "source_query" && c.kind === "search")
        .map((c) => c.page),
    ),
  ).toEqual([1, 2]);
});

test("explicit work-keyword mode retains all hits and mode changes clear author selections", async ({
  page,
}) => {
  await installMock(page, { authorSearchResults: true });
  await page.goto("/");
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-search-input").fill("Mint");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-completeness")).toContainText("已读完");
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 2 部",
  );
  await page.getByTestId("source-query-mode").selectOption("search");
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("source-author-evidence")).toHaveCount(0);
  await expect(page.getByTestId("source-card-JM:201")).toHaveCount(0);
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-completeness")).toContainText("已读完");
  await expect(page.getByTestId("source-keyword-scope")).toContainText(
    "不代表这些作品属于同一作者",
  );
  await expect(page.getByTestId("source-filter-count")).toContainText(
    "当前显示 5 部",
  );
  await expect(page.getByTestId("source-card-JM:203")).toContainText(
    "Mint 合成标题命中",
  );
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 5 部",
  );
  await page.getByTestId("source-query-mode").selectOption("detail");
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await page.getByTestId("source-search-input").fill("203");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-detail")).toContainText(
    "合成验收 JM 作品 203",
  );
});

for (const source of ["JM", "Pica"] as const) {
  test(`${source} author lookup resolves source spelling and aliases identically from search and followed rows`, async ({
    page,
  }) => {
    await installMock(page, {
      authorSearchResults: true,
      authorPolicyResults: true,
    });
    await page.goto("/");
    const primary = source === "JM" ? "Mentha～" : "Mentha Name";
    for (const entry of ["search", "following"] as const) {
      if (entry === "search") {
        await page.getByTestId("nav-discovery").click();
        await page.getByTestId("source-tab-" + source).click();
        await page.getByTestId("source-search-input").fill("Mint");
        await page.getByTestId("source-search-submit").click();
      } else {
        await page.getByTestId("nav-authors").click();
        await page.getByTestId("source-tab-" + source).click();
        await page
          .getByRole("button", { name: "搜索该作者", exact: true })
          .click();
      }
      await expect(page.getByTestId("source-completeness")).toContainText(
        "已读完",
      );
      await expect(page.getByTestId("source-author-evidence")).toContainText(
        "作者作品 3 部 · 其他关键词结果 3 部",
      );
      for (const id of ["201", "204", "206"])
        await expect(
          page.getByTestId(`source-card-${source}:${id}`),
        ).toBeVisible();
      for (const id of ["202", "203", "205"])
        await expect(
          page.getByTestId(`source-card-${source}:${id}`),
        ).toHaveCount(0);
      await page.getByTestId("source-toggle-selection").click();
      await page.getByTestId("source-select-all").click();
      await expect(page.getByTestId("source-selection-bar")).toContainText(
        "已选 3 部",
      );
    }
    expect(
      await page.evaluate(
        (source) =>
          window.sourceTest.calls
            .filter(
              (call) =>
                call.command === "source_query" &&
                call.kind === "search" &&
                call.source === source,
            )
            .map((call) => [call.query, call.page]),
        source,
      ),
    ).toEqual([
      [primary, 1],
      [primary, 2],
      ["Mentha", 1],
      ["Mentha", 2],
      [primary, 1],
      [primary, 2],
      ["Mentha", 1],
      ["Mentha", 2],
    ]);
  });
}

for (const source of ["JM", "Pica"] as const) {
  test(`${source} reviewed work credits agree between discovery and followed-author entries and never override fresh changed credits`, async ({
    page,
  }) => {
    const correctId = source === "Pica" ? "201".padStart(24, "0") : "201";
    const otherId = source === "Pica" ? "204".padStart(24, "0") : "204";
    await installMock(page, {
      authorSearchResults: true,
      reviewedWorkCredits: true,
    });
    await page.goto("/");
    for (const entry of ["search", "following"] as const) {
      if (entry === "search") {
        await page.getByTestId("nav-discovery").click();
        await page.getByTestId("source-tab-" + source).click();
        await page.getByTestId("source-search-input").fill("Mint");
        await page.getByTestId("source-search-submit").click();
      } else {
        await page.getByTestId("nav-authors").click();
        await page.getByTestId("source-tab-" + source).click();
        await page
          .getByRole("button", { name: "搜索该作者", exact: true })
          .click();
      }
      await expect(page.getByTestId("source-completeness")).toContainText(
        "已读完",
      );
      await expect(page.getByTestId("source-author-evidence")).toContainText(
        "作者作品 1 部 · 其他关键词结果 4 部",
      );
      await expect(page.getByTestId("source-filter-count")).toContainText(
        "未入库 1 部 · 当前显示 1 部",
      );
      const correct = page.getByTestId(`source-card-${source}:${correctId}`);
      await expect(correct).toContainText("Harbor Studio (Mint)");
      await expect(
        correct.getByTestId("author-credit-reviewed"),
      ).toHaveAttribute(
        "title",
        "已按本作品核对署名。来源原署名：Incorrect credit",
      );
      await expect(
        page.getByTestId(`source-card-${source}:${otherId}`),
      ).toHaveCount(0);
      await page.getByTestId("source-toggle-selection").click();
      await page.getByTestId("source-select-all").click();
      await expect(page.getByTestId("source-selection-bar")).toContainText(
        "已选 1 部",
      );
      await page.getByTestId("source-author-results-toggle").click();
      const other = page.getByTestId(`source-card-${source}:${otherId}`);
      await expect(other).toContainText("Different Writer");
      await expect(other.getByTestId("author-credit-reviewed")).toHaveAttribute(
        "title",
        "已按本作品核对署名。来源原署名：Guest、Mint",
      );
      await expect(other.getByRole("checkbox")).toHaveCount(0);
      await expect(page.getByTestId("source-select-all")).toHaveCount(0);
      await page.getByTestId("source-author-results-toggle").click();
      await correct.locator("h3 button").click();
      await expect(page.getByTestId("source-detail")).toContainText(
        "Harbor Studio (Mint)",
      );
      await expect(
        page.getByTestId("source-detail").getByTestId("author-credit-reviewed"),
      ).toHaveCount(1);
      await page.getByTestId("source-detail-back").click();
    }
    await page
      .getByTestId(`source-card-${source}:${correctId}`)
      .locator("h3 button")
      .click();
    await page.evaluate(() => {
      window.sourceTest.changedDetailCredit = true;
    });
    // Reopening obtains fresh metadata; the old expected-author guard must fail.
    await page.getByTestId("source-detail-back").click();
    await page
      .getByTestId(`source-card-${source}:${correctId}`)
      .locator("h3 button")
      .click();
    await expect(page.getByTestId("source-detail")).toContainText(
      "Website now changed credit",
    );
    await expect(
      page.getByTestId("source-detail").getByTestId("author-credit-reviewed"),
    ).toHaveCount(0);
  });
}

test("searching a followed source author uses the same explicit author evidence", async ({
  page,
}) => {
  await installMock(page, { authorSearchResults: true });
  await page.goto("/");
  await page.getByTestId("nav-authors").click();
  await page.getByTestId("source-tab-Pica").click();
  await expect(page.getByTestId("source-authors")).toContainText("Mint");
  await page.getByRole("button", { name: "搜索该作者", exact: true }).click();
  await expect(page.getByTestId("source-sort")).toHaveValue("updated-desc");
  await page.getByTestId("source-sort").selectOption("updated-asc");
  await expect(page.getByTestId("source-completeness")).toContainText("已读完");
  await expect(page.getByTestId("source-author-evidence")).toContainText(
    "作者作品 2 部 · 其他关键词结果 3 部",
  );
  await expect(page.getByTestId("source-card-Pica:202")).toHaveCount(0);
  await expect(page.getByTestId("source-query-mode")).toHaveCount(0);
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await expect(page.getByTestId("source-selection-bar")).toContainText(
    "已选 2 部",
  );
  await page.getByTestId("source-author-results-toggle").click();
  await expect(page.getByTestId("source-selection-bar")).toHaveCount(0);
  await expect(page.getByTestId("source-card-Pica:205")).toBeVisible();
});

async function favoritePages(page: Page) {
  return page.evaluate(() =>
    window.sourceTest.calls
      .filter(
        (call) =>
          call.command === "source_query" &&
          call.kind === "favorites" &&
          call.source === "JM",
      )
      .map((call) => call.page),
  );
}

async function picaFavoritePages(page: Page, reverse: boolean) {
  return page.evaluate(
    (direction) =>
      window.sourceTest.calls
        .filter(
          (call) =>
            call.command === "source_query" &&
            call.kind === "favorites" &&
            call.source === "Pica" &&
            Boolean(call.reverse) === direction,
        )
        .map((call) => call.page),
    reverse,
  );
}

async function openAccounts(page: Page) {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-accounts").click();
  await expect(page.getByTestId("source-account-settings")).toBeVisible();
}

async function connectJM(page: Page) {
  await page.getByTestId("account-connect-JM").click();
  await page.getByTestId("account-username").fill("synthetic-user");
  await page.getByTestId("account-password").fill("fixture-only-password");
  await page.getByTestId("account-login-submit").click();
}

async function detail(page: Page, source: Source = "JM") {
  await openFavorites(page);
  await page.getByTestId("source-tab-" + source).click();
  await page.getByTestId("source-open-" + source + ":123").click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toContainText(
    "合成验收 " + source,
  );
}
