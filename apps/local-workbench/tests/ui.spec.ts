import { expect, test, type Locator, type Page } from "@playwright/test";

// Each test gets a fresh browser context. Do not clear storage on page load:
// the close/reopen/reload test must exercise the app's own durable demo state.
test.use({ storageState: { cookies: [], origins: [] } });

const pageErrors = new WeakMap<Page, string[]>();

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  pageErrors.set(page, errors);
  page.on("pageerror", (error) => errors.push(error.stack ?? error.message));
  await page.goto("/");
  await expect(page.getByTestId("demo-label")).toContainText("交互样例");
  await expect(page.getByTestId("demo-label")).toContainText("模拟数据");
});

test.afterEach(async ({ page }) => {
  expect(
    pageErrors.get(page) ?? [],
    "the workbench must not raise browser runtime errors",
  ).toEqual([]);
});

async function openDiscovery(page: Page) {
  await page.getByTestId("nav-discovery").click();
  await page.getByRole("button", { name: "全部作品", exact: true }).click();
}

const savedSettingsFeedback = /^(?:外观)?已保存到/;

async function expectSettingsSaved(page: Page) {
  await expect(page.locator(".settings-save-message")).toContainText(
    savedSettingsFeedback,
  );
  const save = page.getByTestId("save-settings-page");
  // A disabled button alone also matches an unfinished asynchronous write.
  await expect(save).toHaveText("保存设置");
  await expect(save).toBeDisabled();
}

async function saveSettingsSuccessfully(page: Page) {
  await page.getByTestId("save-settings-page").click();
  await expectSettingsSaved(page);
}

async function selectPair(page: Page) {
  await openDiscovery(page);
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-rain").check();
  await page.getByTestId("select-flight").check();
  await page.getByTestId("batch-download").click();
  const dialog = page.getByTestId("confirm-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("雨停之前");
  await expect(dialog).toContainText("白昼航线");
}

async function confirmPair(page: Page) {
  // One confirmation authorizes both selected demo tasks.
  await page.getByTestId("confirm-download").click();
  await expect(page.getByTestId("confirm-dialog")).toBeHidden();
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("queue-page")).toBeVisible();
  await expectPairOnce(page);
}

async function expectPairOnce(page: Page) {
  // Keep the three existing demo tasks as well as the two newly approved works.
  for (const workId of ["sea", "moon", "train", "rain", "flight"]) {
    await expect(page.getByTestId(`task-${workId}`)).toHaveCount(1);
  }
  await expect(page.locator('[data-testid^="task-"]')).toHaveCount(5);
}

async function expectNoHorizontalOverflow(page: Page) {
  await expect(page.locator("main")).toBeVisible();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        // The app scrolls inside main; shell overflow:hidden can conceal an
        // overflowing content area even when body itself fits the viewport.
        return Math.max(
          document.documentElement.scrollWidth -
            document.documentElement.clientWidth,
          document.body.scrollWidth - document.documentElement.clientWidth,
          ...Array.from(
            document.querySelectorAll("main, dialog[open]"),
            (main) => main.scrollWidth - main.clientWidth,
          ),
        );
      }),
    )
    .toBeLessThanOrEqual(1);
}

test("discovery search survives a visit to the independent detail page", async ({
  page,
}) => {
  await openDiscovery(page);
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(8);
  await page.getByTestId("search-input").fill("雨");
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
  await expect(page.getByTestId("card-rain")).toContainText("雨停之前");
  await page.getByTestId("open-rain").click();
  await expect(page.getByTestId("detail-page")).toBeVisible();
  await expect(page.getByTestId("detail-page")).toContainText("雨停之前");
  await page.getByTestId("back-library").click();
  await expect(page.getByTestId("search-input")).toHaveValue("雨");
  await expect(page.getByTestId("card-rain")).toBeVisible();
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
  await page.getByTestId("search-input").fill("");
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(8);
});

test("the queue detail author opens source-scoped works including remote titles", async ({
  page,
}) => {
  await page.getByTestId("nav-queue").click();
  await page
    .getByRole("button", { name: "查看《海街来信》详情", exact: true })
    .click();
  await expect(page.getByTestId("detail-page")).toBeVisible();
  await page
    .getByTestId("detail-page")
    .getByRole("button", { name: "七岛灯", exact: true })
    .click();
  await expect(page.getByTestId("card-sea")).toBeVisible();
  await expect(page.getByTestId("search-input")).toHaveValue("七岛灯");
  await expect(page.getByLabel("来源筛选")).toHaveValue("Pica");
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
  await expect(page.getByTestId("queue-page")).toBeHidden();
});

test("one batch confirmation queues two works and prevents duplicate selection", async ({
  page,
}) => {
  await selectPair(page);
  await confirmPair(page);
  await openDiscovery(page);
  await page.getByTestId("toggle-selection").click();
  // Queued works remain selectable for local organization, but cannot enqueue again.
  await page.getByTestId("select-rain").check();
  await page.getByTestId("select-flight").check();
  await expect(page.getByTestId("batch-booklist")).toBeEnabled();
  await expect(page.getByTestId("batch-download")).toBeDisabled();
  await page.getByTestId("nav-queue").click();
  await expectPairOnce(page);
});

test("an unresolved review work has no batch-download selection", async ({
  page,
}) => {
  await openDiscovery(page);
  await page.getByTestId("toggle-selection").click();
  const review = page.getByTestId("card-echo");
  await expect(review).toContainText("星光回声");
  await expect(review.getByRole("checkbox")).toHaveCount(1);
  await page.getByTestId("select-echo").check();
  await expect(page.getByTestId("batch-booklist")).toBeEnabled();
  await expect(page.getByTestId("batch-download")).toBeDisabled();
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("task-echo")).toHaveCount(0);
});

test("safe close and reopen retain tasks and an explicitly paused queue", async ({
  page,
}) => {
  await selectPair(page);
  await confirmPair(page);
  await page.getByTestId("pause-queue").click();
  await expect(page.getByTestId("pause-queue")).toContainText("继续队列");
  await page.getByTestId("demo-offline").click();
  await expect(page.getByTestId("demo-offline")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByTestId("demo-close").click();
  await expect(page.getByTestId("demo-reopen")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("demo-reopen")).toBeVisible();
  await page.getByTestId("demo-reopen").click();
  await page.getByTestId("nav-queue").click();
  await expectPairOnce(page);
  await expect(page.getByTestId("pause-queue")).toContainText("继续队列");
  await expect(page.getByTestId("demo-offline")).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  // Reopening the overlay alone could pass with in-memory state. A real reload
  // also verifies that queued tasks and the user's pause choice were persisted.
  await page.reload();
  await page.getByTestId("nav-queue").click();
  await expectPairOnce(page);
  await expect(page.getByTestId("pause-queue")).toContainText("继续队列");
  await expect(page.getByTestId("demo-offline")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test.describe("390px workbench layout", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("library, detail, confirmation, queue and settings fit the viewport", async ({
    page,
  }) => {
    await expectNoHorizontalOverflow(page);
    await openDiscovery(page);
    await expect(page.getByTestId("card-rain")).toBeVisible();
    await expectNoHorizontalOverflow(page);
    await page.getByTestId("open-rain").click();
    await expect(page.getByTestId("detail-page")).toBeVisible();
    await expectNoHorizontalOverflow(page);
    await page.getByTestId("back-library").click();
    await selectPair(page);
    await expectNoHorizontalOverflow(page);
    await confirmPair(page);
    await expectNoHorizontalOverflow(page);
    await page.getByTestId("nav-settings").click();
    await expect(
      page.getByRole("heading", { name: "设置", exact: true }),
    ).toBeVisible();
    await page.getByTestId("settings-network").click();
    await expect(page.getByRole("button", { name: "重置样例" })).toBeVisible();
    await expectNoHorizontalOverflow(page);
    for (const tab of ["accounts", "library", "appearance", "resources"]) {
      await page.getByTestId(`settings-${tab}`).click();
      await expectNoHorizontalOverflow(page);
    }
  });
});

test("author following uses names and source-scoped navigation without avatars", async ({
  page,
}) => {
  await page.getByTestId("nav-authors").click();
  await expect(
    page.locator(".authors-list img, .authors-list [class*='avatar']"),
  ).toHaveCount(0);
  await expect(page.locator(".author-row")).toHaveCount(8);
  await page.getByRole("button", { name: /林间折页/ }).click();
  await expect(page.getByTestId("card-rain")).toBeVisible();
  await expect(page.getByLabel("来源筛选")).toHaveValue("JM");
  await expect(page.locator('[data-testid^="card-"]')).toHaveCount(1);
});

test("appearance and resource page drafts save separately and redundant controls are absent", async ({
  page,
}) => {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("settings-density-5").click();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
  await page.getByTestId("settings-resources").click();
  const panel = page.locator(".settings-panel");
  await expect(panel.getByRole("combobox")).toHaveCount(2);
  await expect(panel).not.toContainText("ZIP 打包");
  await expect(panel).not.toContainText("图像处理");
  await page.getByTestId("resource-profile-economy").click();
  await saveSettingsSuccessfully(page);
  await page.getByTestId("settings-appearance").click();
  await expect(page.getByTestId("background-mode-A")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByTestId("nav-library").click();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "B",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("settings-density-5").click();
  await saveSettingsSuccessfully(page);
  await page.reload();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
  await expect(
    page.getByRole("button", { name: "每行 5 部", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-resources").click();
  await expect(page.getByLabel("同时下载作品", { exact: true })).toHaveValue(
    "1",
  );
  await expect(page.getByLabel("全局图片请求数", { exact: true })).toHaveValue(
    "2",
  );
});

test("a chosen background survives reload and invalid files preserve it", async ({
  page,
}) => {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
    "base64",
  );
  await page.getByTestId("background-file").setInputFiles({
    name: "wallpaper.png",
    mimeType: "image/png",
    buffer: png,
  });
  await expect(page.getByTestId("background-filename")).toContainText(
    "wallpaper.png",
  );
  await saveSettingsSuccessfully(page);
  await page.reload();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await expect(page.getByTestId("background-filename")).toContainText(
    "wallpaper.png",
  );
  await page.getByTestId("background-file").setInputFiles({
    name: "invalid.png",
    mimeType: "image/png",
    buffer: Buffer.from("not an image"),
  });
  await expect(page.getByTestId("background-filename")).toContainText(
    "wallpaper.png",
  );
  await expect(page.getByRole("alert")).toBeVisible();
  await page.getByTestId("settings-density-9").click();
  await page.getByTestId("restore-default-background").click();
  await expect(page.getByTestId("settings-density-9")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("clearing an empty favorites search preserves its source", async ({
  page,
}) => {
  await page.getByTestId("nav-favorites").click();
  await page.getByLabel("来源筛选").selectOption("Pica");
  await page.getByTestId("search-input").fill("no-result-example");
  await page.getByRole("button", { name: "清空筛选", exact: true }).click();
  await expect(page.getByLabel("来源筛选")).toHaveValue("Pica");
  await expect(page.locator(".source-badge")).toHaveText([
    "Pica",
    "Pica",
    "Pica",
    "Pica",
  ]);
});

test("a preference write failure keeps the visible draft without claiming it saved", async ({
  page,
}) => {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.evaluate(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key === "mangamonitor.workbench.preferences.v1")
        throw new DOMException("quota", "QuotaExceededError");
      return original.call(this, key, value);
    };
  });
  await page.getByTestId("save-settings-page").click();
  await expect(page.getByTestId("background-mode-A")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.locator(".settings-save-message")).toContainText(
    "保存失败，草稿已保留",
  );
  await page.reload();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "B",
  );
});

const fixtureIds = Array.from(
  { length: 100 },
  (_, index) => "fixture-" + String(index + 1).padStart(3, "0"),
);

async function openReadyFixture(page: Page) {
  await page.goto("/?fixture=ready-100");
  await openDiscovery(page);
  await page.getByTestId("search-input").fill("示例作品");
  await expect(
    page.getByTestId("cover-grid").locator("[data-work-id]"),
  ).toHaveCount(100);
}

async function expectVerticalFixture(grid: Locator, columns: number) {
  const cards = grid.locator('[data-work-id^="fixture-"]');
  await expect(cards).toHaveCount(100);
  const positions = await cards.evaluateAll((elements) =>
    elements.map((element) => {
      const { x, y } = element.getBoundingClientRect();
      return {
        id: element.getAttribute("data-work-id"),
        x: Math.round(x),
        y: Math.round(y),
      };
    }),
  );
  expect(positions.map((position) => position.id)).toEqual(fixtureIds);
  expect(new Set(positions.map((position) => position.y)).size).toBe(
    Math.ceil(100 / columns),
  );
  for (let start = 0; start < positions.length; start += columns) {
    const row = positions.slice(start, start + columns);
    expect(new Set(row.map((position) => position.y)).size).toBe(1);
    expect(row.map((position) => position.x)).toEqual(
      [...row.map((position) => position.x)].sort((a, b) => a - b),
    );
    if (start > 0) {
      expect(row[0].y).toBeGreaterThan(positions[start - columns].y);
    }
  }
  await expect(grid.locator('img:not([loading="lazy"])')).toHaveCount(0);
}

async function positionFixtureAnchor(page: Page) {
  const card = page
    .getByTestId("cover-grid")
    .locator('[data-work-id="fixture-046"]');
  await card.evaluate((element) => {
    const main = element.closest("main")!;
    main.scrollTop +=
      element.getBoundingClientRect().top - main.getBoundingClientRect().top;
    const toolbar = main.querySelector(".library-toolbar")!;
    main.scrollTop -=
      toolbar.getBoundingClientRect().bottom -
      main.getBoundingClientRect().top +
      8;
  });
  return card.evaluate(
    (element) =>
      element.getBoundingClientRect().top -
      element.closest("main")!.getBoundingClientRect().top,
  );
}

async function expectFixtureAnchor(page: Page, offset: number) {
  await expect
    .poll(() =>
      page
        .getByTestId("cover-grid")
        .locator('[data-work-id="fixture-046"]')
        .evaluate(
          (element, expected) =>
            Math.abs(
              element.getBoundingClientRect().top -
                element.closest("main")!.getBoundingClientRect().top -
                expected,
            ),
          offset,
        ),
    )
    .toBeLessThanOrEqual(3);
}

test("the library separates local works from remote-only titles in both grids", async ({
  page,
}) => {
  for (const id of ["recent-grid", "cover-grid"]) {
    const grid = page.getByTestId(id);
    await expect(grid.locator('[data-work-id="summer"]')).toBeVisible();
    await expect(grid.locator('[data-work-id="moon"]')).toBeVisible();
    for (const workId of ["rain", "flight", "bookshop", "echo"]) {
      await expect(grid.locator('[data-work-id="' + workId + '"]')).toHaveCount(
        0,
      );
    }
    await expect(grid.getByRole("checkbox")).toHaveCount(0);
  }
  await expect(
    page.getByRole("button", { name: "本地书单", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "全部作品", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".selection-bar")).toHaveCount(0);
});

test("discovery and both favorite sources keep browsing separate from selection", async ({
  page,
}) => {
  await openDiscovery(page);
  await expect(
    page.getByTestId("cover-grid").getByRole("checkbox"),
  ).toHaveCount(0);
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-rain").check();
  await expect(page.locator(".selection-bar")).toBeVisible();
  await page.getByTestId("toggle-selection").click();
  await expect(
    page.getByTestId("cover-grid").getByRole("checkbox"),
  ).toHaveCount(0);
  await expect(page.locator(".selection-bar")).toHaveCount(0);
  for (const source of ["JM", "Pica"]) {
    await page.getByTestId("nav-favorites").click();
    await page.getByLabel("来源筛选").selectOption(source);
    await expect(
      page.getByTestId("cover-grid").getByRole("checkbox"),
    ).toHaveCount(0);
    await page.getByTestId("toggle-selection").click();
    await page
      .getByTestId(source === "JM" ? "select-rain" : "select-bookshop")
      .check();
    await expect(page.locator(".selection-bar")).toBeVisible();
    await page.getByTestId("toggle-selection").click();
    await expect(page.locator(".selection-bar")).toHaveCount(0);
  }
});

test("clearing search or changing source clears selection and explains the scope change", async ({
  page,
}) => {
  await openDiscovery(page);
  await page.getByTestId("search-input").fill("雨");
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  await expect(page.getByTestId("select-rain")).toBeChecked();
  await page.getByRole("button", { name: "清空搜索", exact: true }).click();
  await expect(
    page.getByTestId("cover-grid").locator("[data-work-id]"),
  ).toHaveCount(8);
  await expect(page.locator(".selection-bar")).toHaveCount(0);
  await expect(
    page.getByRole("status").filter({ hasText: /选择.*清空|清空.*选择/ }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  await page.getByLabel("来源筛选").selectOption("Pica");
  await expect(page.locator(".selection-bar")).toHaveCount(0);
  await expect(
    page.getByRole("status").filter({ hasText: /选择.*清空|清空.*选择/ }),
  ).toBeVisible();
});

for (const mode of ["A", "B"] as const) {
  for (const density of [5, 7, 9] as const) {
    test(
      mode +
        " background and density " +
        density +
        " persist with 100 ordered records",
      async ({ page }) => {
        await page.setViewportSize({ width: 1672, height: 941 });
        await openReadyFixture(page);
        await expect(page.locator(".app-shell")).toHaveAttribute(
          "data-background-mode",
          "B",
        );
        await page.getByTestId("nav-settings").click();
        await page.getByTestId("settings-appearance").click();
        await page.getByTestId("background-mode-" + mode).click();
        await page.getByTestId("settings-density-" + density).click();
        if (mode !== "B" || density !== 7) {
          await saveSettingsSuccessfully(page);
        } else {
          await expect(page.getByTestId("save-settings-page")).toBeDisabled();
          await expect(page.locator(".settings-save-message")).toContainText(
            "本页设置已与保存内容一致",
          );
        }
        await page.getByTestId("nav-discovery").click();
        await expect(page.getByTestId("search-input")).toHaveValue("示例作品");
        await expect(page.locator(".app-shell")).toHaveAttribute(
          "data-background-mode",
          mode,
        );
        await expect(page.getByTestId("cover-grid")).toHaveAttribute(
          "data-density",
          String(density),
        );
        await expectVerticalFixture(page.getByTestId("cover-grid"), density);
        await expectNoHorizontalOverflow(page);
        await page.reload();
        await openDiscovery(page);
        await page.getByTestId("search-input").fill("示例作品");
        await expect(page.locator(".app-shell")).toHaveAttribute(
          "data-background-mode",
          mode,
        );
        await expect(page.getByTestId("cover-grid")).toHaveAttribute(
          "data-density",
          String(density),
        );
        await page.setViewportSize({ width: 390, height: 844 });
        await expectVerticalFixture(page.getByTestId("cover-grid"), 2);
        await expectNoHorizontalOverflow(page);
        await page.setViewportSize({ width: 1672, height: 941 });
        await expectVerticalFixture(page.getByTestId("cover-grid"), density);
      },
    );
  }
}

test("recent and full library grids each retain all 100 owned fixture records", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await page.goto("/?fixture=owned-100");
  await page.getByRole("button", { name: "每行 5 部", exact: true }).click();
  for (const id of ["recent-grid", "cover-grid"]) {
    await expectVerticalFixture(page.getByTestId(id), 5);
  }
  await page
    .getByTestId("cover-grid")
    .locator("[data-work-id]")
    .last()
    .scrollIntoViewIfNeeded();
  expect(
    await page.locator("main").evaluate((main) => main.scrollTop),
  ).toBeGreaterThan(0);
  await expectNoHorizontalOverflow(page);
});

test("density and settings changes retain a mid-list anchor, selection and filters", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await openReadyFixture(page);
  await page.getByRole("button", { name: "每行 5 部", exact: true }).click();
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  const beforeDensity = await positionFixtureAnchor(page);
  await page.getByRole("button", { name: "每行 9 部", exact: true }).click();
  await expectFixtureAnchor(page, beforeDensity);
  await expect(
    page.getByTestId("cover-grid").locator('input[type="checkbox"]:checked'),
  ).toHaveCount(100);
  const beforeSettings = await positionFixtureAnchor(page);
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await saveSettingsSuccessfully(page);
  await page.getByTestId("nav-discovery").click();
  await expectFixtureAnchor(page, beforeSettings);
  await expect(page.getByTestId("search-input")).toHaveValue("示例作品");
  await expect(page.getByLabel("来源筛选")).toHaveValue("all");
  await expect(
    page.getByRole("button", { name: "全部作品", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("cover-grid")).toHaveAttribute(
    "data-density",
    "9",
  );
  await expect(
    page.getByTestId("cover-grid").locator('input[type="checkbox"]:checked'),
  ).toHaveCount(100);
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
});

test("full-scope selection excludes local, queued and unresolved default works", async ({
  page,
}) => {
  await openDiscovery(page);
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  for (const id of ["rain", "flight", "bookshop"]) {
    await expect(page.getByTestId("select-" + id)).toBeChecked();
  }
  await expect(
    page.getByTestId("cover-grid").locator('input[type="checkbox"]:checked'),
  ).toHaveCount(3);
  await page.getByTestId("batch-download").click();
  await expect(page.locator(".confirmation-work")).toHaveCount(3);
  await expect(page.locator(".confirmation-work strong")).toHaveText([
    "雨停之前",
    "白昼航线",
    "雾中书店",
  ]);
});

test("100 offscreen records stay selected through scrolling and fit the batch dialog", async ({
  page,
}) => {
  await page.setViewportSize({ width: 780, height: 680 });
  await openReadyFixture(page);
  const grid = page.getByTestId("cover-grid");
  const last = grid.locator('[data-work-id="fixture-100"]');
  expect(
    await last.evaluate(
      (element) => element.getBoundingClientRect().top > window.innerHeight,
    ),
  ).toBe(true);
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  await expect(grid.locator('input[type="checkbox"]:checked')).toHaveCount(100);
  await expect(page.locator(".selected-count")).toHaveText("100");
  await last.scrollIntoViewIfNeeded();
  await expect(grid.locator('input[type="checkbox"]:checked')).toHaveCount(100);
  await page.locator("main").evaluate((main) => {
    main.scrollTop = main.scrollHeight;
  });
  const lastBottom = await last.evaluate(
    (element) => element.getBoundingClientRect().bottom,
  );
  const barTop = await page
    .locator(".selection-bar")
    .evaluate((element) => element.getBoundingClientRect().top);
  expect(lastBottom).toBeLessThanOrEqual(barTop + 1);
  await page.getByTestId("batch-download").click();
  const dialog = page.getByTestId("confirm-dialog");
  await expect(dialog.locator(".confirmation-work")).toHaveCount(100);
  await expect(dialog.locator(".confirmation-work strong")).toHaveText(
    fixtureIds.map(
      (_, index) => "示例作品 " + String(index + 1).padStart(3, "0"),
    ),
  );
  await dialog.locator(".confirmation-work").last().scrollIntoViewIfNeeded();
  expect(
    await dialog
      .locator(".confirmation-list")
      .evaluate((element) => element.scrollTop),
  ).toBeGreaterThan(0);
  await expect(page.getByTestId("confirm-download")).toBeInViewport();
  await expectNoHorizontalOverflow(page);
  await page.getByTestId("confirm-download").click();
  await expect(page.getByTestId("queue-page")).toBeVisible();
  await expect(page.locator('[data-testid^="task-fixture-"]')).toHaveCount(100);
  await page.reload();
  await page.getByTestId("nav-queue").click();
  await expect(page.locator('[data-testid^="task-fixture-"]')).toHaveCount(100);
  // This only proves isolated demo state; it grants no real download authority.
  await page.goto("/");
  await page.getByTestId("nav-queue").click();
  await expect(page.locator('[data-testid^="task-fixture-"]')).toHaveCount(0);
});
test("appearance settings preserve an explicit favorite source and pending selection", async ({
  page,
}) => {
  await page.getByTestId("nav-favorites").click();
  await page.getByLabel("来源筛选").selectOption("Pica");
  await page.getByRole("button", { name: "待下载", exact: true }).click();
  await page.getByTestId("search-input").fill("雾");
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  await expect(page.getByTestId("select-bookshop")).toBeChecked();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await saveSettingsSuccessfully(page);
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByLabel("来源筛选")).toHaveValue("Pica");
  await expect(page.getByTestId("search-input")).toHaveValue("雾");
  await expect(
    page.getByRole("button", { name: "待下载", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("select-bookshop")).toBeChecked();
  await expect(page.locator(".selected-count")).toHaveText("1");
});

test("search from a source detail retains its source list", async ({
  page,
}) => {
  await page.getByTestId("nav-favorites").click();
  await page.getByTestId("open-rain").click();
  await page.getByTestId("search-input").fill("雨");
  await expect(page.getByTestId("detail-page")).toBeHidden();
  await expect(page.getByTestId("nav-favorites")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByTestId("card-rain")).toBeVisible();
  await expect(page.getByLabel("来源筛选")).toHaveValue("JM");
});

test("a detail visit retains the list anchor after changing density in settings", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await openReadyFixture(page);
  const offset = await positionFixtureAnchor(page);
  await page.getByTestId("open-fixture-046").click();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("settings-density-9").click();
  await saveSettingsSuccessfully(page);
  await page.getByTestId("nav-discovery").click();
  await expect(page.getByTestId("detail-page")).toBeVisible();
  await page.getByTestId("back-library").click();
  await expectFixtureAnchor(page, offset);
});

test("settings search uses its own query without changing the source selection", async ({
  page,
}) => {
  await openDiscovery(page);
  await page.getByTestId("search-input").fill("雨");
  await page
    .getByRole("button", { name: "全选待下载（当前筛选）", exact: true })
    .click();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("search-input").fill("外观");
  await expect(page.getByTestId("settings-appearance")).toBeVisible();
  await expect(page.getByTestId("settings-accounts")).toHaveCount(0);
  await page.getByTestId("nav-discovery").click();
  await expect(page.getByTestId("search-input")).toHaveValue("雨");
  await expect(page.getByTestId("select-rain")).toBeChecked();
});

test("the A background remains visible under the toolbar until it sticks", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await openReadyFixture(page);
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await saveSettingsSuccessfully(page);
  await page.getByTestId("nav-discovery").click();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
  const toolbar = page.locator(".library-toolbar");
  await expect(toolbar).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await positionFixtureAnchor(page);
  await expect(toolbar).toHaveCSS("backdrop-filter", "blur(12px)");
  await page.locator("main").evaluate((main) => {
    main.scrollTop = 0;
  });
  await expect(toolbar).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
});

declare global {
  interface Window {
    settingsWriteTestHooks?: {
      lockRequests: number;
      releaseWrite?: () => void;
    };
  }
}

test("a delayed settings write stays pending until storage commits and survives reopening", async ({
  page,
}) => {
  // This delays only the browser preview's preference lock, not native I/O.
  const preferenceKey = "mangamonitor.workbench.preferences.v1";
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("settings-density-9").click();
  const save = page.getByTestId("save-settings-page");
  await expect(save).toBeEnabled();
  const before = await page.evaluate(
    (key) => localStorage.getItem(key),
    preferenceKey,
  );
  await page.evaluate((key) => {
    const request = navigator.locks.request;
    const hooks = (window.settingsWriteTestHooks = {
      lockRequests: 0,
    } as NonNullable<Window["settingsWriteTestHooks"]>);
    navigator.locks.request = ((
      ...args: Parameters<LockManager["request"]>
    ) => {
      const run = () =>
        Reflect.apply(request, navigator.locks, args) as Promise<unknown>;
      if (args[0] !== key) return run();
      hooks.lockRequests += 1;
      return new Promise((resolve, reject) => {
        hooks.releaseWrite = () => {
          delete hooks.releaseWrite;
          void run().then(resolve, reject);
        };
      });
    }) as LockManager["request"];
  }, preferenceKey);

  await save.click();
  await expect
    .poll(() =>
      page.evaluate(() => window.settingsWriteTestHooks?.lockRequests),
    )
    .toBe(1);
  await expect(save).toBeDisabled();
  await expect(save).toHaveText("正在保存…");
  await expect(page.locator(".settings-save-message")).not.toContainText(
    savedSettingsFeedback,
  );
  expect(
    await page.evaluate((key) => localStorage.getItem(key), preferenceKey),
  ).toBe(before);

  await page.evaluate(() => window.settingsWriteTestHooks?.releaseWrite?.());
  await expectSettingsSaved(page);
  const saved = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)!),
    preferenceKey,
  );
  expect(saved.appearance.backgroundMode).toBe("A");
  expect(saved.appearance.density).toBe(9);
  await page.reload();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await expect(page.getByTestId("background-mode-A")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("settings-density-9")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("save-settings-page")).toBeDisabled();
});
