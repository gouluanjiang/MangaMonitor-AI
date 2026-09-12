import { expect, test, type Page } from "@playwright/test";
import type {
  AccountSummary,
  FollowingSnapshot,
  Source,
  SourceWork,
  CatalogSnapshot,
} from "../src/source-types.ts";
import type { BooklistsDocument } from "../src/booklists.ts";
import type { WorkbenchPreferences } from "../src/preferences.ts";
import type {
  SourceMatchesSnapshot,
  SourceMatchWork,
} from "../src/source-matches-types.ts";

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
  holdMatchDetail?: boolean;
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
  await expect(page.getByTestId("source-detail")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await page.getByTestId("collection-sentinel").scrollIntoViewIfNeeded();
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
        call.command !== "jm_download_read" &&
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
      tags: [],
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
          authors: [],
        },
        Pica: {
          source: "Pica",
          sessionId: "synthetic-Pica-1",
          revision: 0,
          works: [],
          authors: [],
        },
      },
    });
    let jmHoldUsed = false;
    let picaHoldUsed = false;
    let expiryUsed = false;
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
          if (command === "library_read")
            return {
              revision: 0,
              rootId: null,
              rootPath: null,
              generation: 0,
              phase: "idle",
              freshness: "none",
              items: [],
              visited: 0,
              skipped: 0,
              updatedAt: null,
              errorCode: null,
            };
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
            if (raw.kind === "detail") work.favorite = remoteFavorite[source];
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
  await expect(page.getByTestId("source-detail")).toContainText(
    "合成验收 " + source,
  );
}
async function createFromDetail(page: Page, name: string) {
  await page.getByTestId("source-detail-booklist").click();
  await page.getByTestId("booklist-picker-create").click();
  await page.getByTestId("booklist-picker-name").fill(name);
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  return page.evaluate(() => window.sourceTest.booklists.value.lists[0].id);
}

test("manual cross-source confirmation previews both works, survives restart and unlinks without changing phone evidence", async ({
  page,
}) => {
  const picaId = "0123456789abcdef01234567";
  await installMock(page, { crossSourcePhone: true });
  await detail(page);
  await expect(page.getByTestId("source-detail-stock")).toContainText(
    "尚未匹配",
  );
  await page.getByTestId("source-match-id").fill(picaId);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter(
        (call) => call.command === "source_matches_confirm",
      ),
    ),
  ).toEqual([]);
  await page.getByTestId("source-match-lookup").click();
  await expect(page.getByTestId("source-match-preview")).toContainText(
    "JM · 123",
  );
  await expect(page.getByTestId("source-match-preview")).toContainText(
    "哔咔 · " + picaId,
  );
  await expect(page.getByTestId("source-match-confirm")).toBeDisabled();
  await page.getByLabel("我已核对内容与版本，确认是同一作品").check();
  await page.getByTestId("source-match-confirm").click();
  await expect(page.getByTestId("source-match-confirmed")).toContainText(
    "已手动确认同一作品",
  );
  await expect(page.getByTestId("source-detail-stock")).toContainText("已入库");
  await page.reload();
  await detail(page);
  await expect(page.getByTestId("source-match-confirmed")).toContainText(
    picaId,
  );
  await expect(page.getByTestId("source-detail-stock")).toContainText("已入库");
  await page.getByTestId("source-match-unlink").click();
  await expect(page.getByTestId("source-detail-stock")).toContainText("已入库");
  await page.getByTestId("source-match-unlink-confirm").click();
  await expect(page.getByTestId("source-match-confirmed")).toHaveCount(0);
  await expect(page.getByTestId("source-detail-stock")).toContainText(
    "尚未匹配",
  );
  expect(await page.evaluate(() => window.sourceTest.matches.pairs)).toEqual(
    [],
  );
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter((call) =>
        /phone_library_(mark|unmark)|library_link|jm_download_(prepare|confirm)/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});

test("editing the opposite ID discards a late detail response before manual confirmation", async ({
  page,
}) => {
  await installMock(page, { holdMatchDetail: true });
  await detail(page);
  await page.getByTestId("source-match-id").fill("0123456789abcdef01234567");
  await page.getByTestId("source-match-lookup").click();
  await expect
    .poll(() => page.evaluate(() => window.sourceTest.matchHeld))
    .toBe(true);
  await page.getByTestId("source-match-id").fill("fedcba9876543210fedcba98");
  await page.evaluate(() => window.sourceTest.releaseMatch!());
  await expect(page.getByTestId("source-match-lookup")).toBeEnabled();
  await expect(page.getByTestId("source-match-preview")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.filter(
        (call) => call.command === "source_matches_confirm",
      ),
    ),
  ).toEqual([]);
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
  ).toHaveText(["未知", "未知", "尚未设置漫画库"]);
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

test("two source identities sharing a work ID join one local booklist and return to their real details", async ({
  page,
}) => {
  await installMock(page);
  await detail(page);
  const id = await createFromDetail(page, "合成跨来源书单");
  await expect(page.getByTestId("source-detail")).toContainText("合成验收 JM");
  await page.getByTestId("source-detail-back").click();
  await detail(page, "Pica");
  await page.getByTestId("source-detail-booklist").click();
  await page.getByTestId("booklist-target-" + id).check();
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
  await expect(page.getByTestId("source-detail")).toContainText(
    "合成验收 Pica",
  );
  await page.getByTestId("source-detail-booklist").click();
  await expect(page.getByTestId("booklist-target-" + id)).toBeDisabled();
  await expect(page.getByTestId("booklist-picker")).toContainText(
    "已包含所选作品",
  );
  await page
    .getByTestId("booklist-picker")
    .getByRole("button", { name: "取消", exact: true })
    .click();
  await expect(page.getByTestId("source-detail")).toContainText(
    "合成验收 Pica",
  );
  expect(
    await page.evaluate(
      () => window.sourceTest.booklists.value.lists[0].members,
    ),
  ).toEqual([
    { source: "JM", workId: "123" },
    { source: "Pica", workId: "123" },
  ]);
  expect(
    await page.evaluate(() =>
      window.sourceTest.calls.some(
        (call) =>
          call.command !== "jm_download_read" &&
          /download|enqueue/.test(call.command),
      ),
    ),
  ).toBe(false);
  await page.getByTestId("nav-library").click();
  await page.getByRole("button", { name: "本地书单", exact: true }).click();
  await page.getByTestId("booklist-select").selectOption(id);
  await expect(page.getByTestId("source-reference-card-JM:123")).toBeVisible();
  await expect(
    page.getByTestId("source-reference-card-Pica:123"),
  ).toBeVisible();
  await page
    .getByTestId("source-reference-card-JM:123")
    .getByRole("button", { name: /查看.*详情/ })
    .click();
  await expect(page.getByTestId("source-detail")).toContainText("合成验收 JM");
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
    "尚未自动检查",
  );
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
  await page.getByTestId("source-workbench").evaluate((element) => {
    element.closest("main")!.scrollTop = 0;
  });
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
