import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import type { WorkbenchPreferences } from "../src/preferences.ts";
import { installWorkflow } from "./workflow-fixture.ts";

declare global {
  interface Window {
    refinedPreferences: {
      revision: number;
      value: WorkbenchPreferences;
    };
  }
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const issues: string[] = [];
  errors.set(page, issues);
  page.on("pageerror", (error) => issues.push(error.message));
  await page.route("**/*", (route) => {
    if (new URL(route.request().url()).hostname === "127.0.0.1")
      return route.continue();
    issues.push("Unexpected external request");
    return route.abort();
  });
  await page.setViewportSize({ width: 1440, height: 960 });
  await installWorkflow(page);
  await expect(page.getByTestId("library-grid")).toBeVisible();
});

test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /discovery_start|download_.*(?:prepare|confirm)|source_(?:follow|favorite)$|delete|promote|replace/.test(
          command,
        ),
      ),
    ),
  ).toEqual([]);
});

async function appearance(page: Page) {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await expect(page.getByTestId("background-tone-night")).toBeVisible();
}

test("dedicated source navigation keeps the active section and existing discovery controls aligned", async ({
  page,
}) => {
  await page.getByTestId("nav-recent").click();
  await expect(page.getByTestId("recent-panel")).toBeVisible();
  await expect(page.getByTestId("nav-recent")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByTestId("discovery-recent")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/refined-recent-desktop.png" });
  await page.getByTestId("nav-ranking").click();
  await expect(page.getByTestId("ranking-panel")).toBeVisible();
  await expect(page.getByTestId("nav-ranking")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("discovery-Pica").click();
  await expect(
    page.getByRole("heading", { name: "哔咔 · 排行榜" }),
  ).toBeVisible();
  await expect(page.getByTestId("nav-ranking")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("nav-discovery").click();
  await expect(page.getByTestId("source-search-control")).toBeVisible();
  await expect(page.getByTestId("discovery-search")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("nav-discovery")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toBeVisible();
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-start")).toBeVisible();
  await expect(page.locator('.sidebar [aria-current="page"]')).toHaveCount(1);
});

test("source-search navigation dismisses embedded recent and ranking details without discarding its query or loaded results", async ({
  page,
}) => {
  await page.getByTestId("nav-discovery").click();
  await page.getByTestId("source-search-input").fill("合成新作者");
  await page.getByTestId("source-search-submit").click();
  await expect(page.getByTestId("source-filter-count")).toContainText(
    "已读完当前来源的搜索范围",
  );
  await expect(page.getByTestId("source-card-JM:104")).toBeVisible();
  const searchCalls = await page.evaluate(
    () =>
      window.workflowTest.calls.filter(
        ({ command, args }) =>
          command === "source_query" && args.kind === "search",
      ).length,
  );
  expect(searchCalls).toBe(2);

  for (const destination of ["recent", "ranking"] as const) {
    await page.getByTestId(`nav-${destination}`).click();
    if (destination === "recent")
      await page.getByLabel("最近更新来源").selectOption("JM");
    const card = page.getByTestId(
      destination === "recent" ? "recent-work-JM:102" : "rank-work-JM:102",
    );
    await card
      .getByRole("button", { name: "本次选择的 JM 作品", exact: true })
      .click();
    await expect(page.getByTestId("source-detail")).toBeVisible();
    await expect(page.getByTestId("source-detail")).toContainText(
      "本次选择的 JM 作品",
    );
    await page.getByTestId("nav-discovery").click();
    await expect(page.getByTestId("source-detail")).toHaveCount(0);
    await expect(page.getByTestId("source-query-mode")).toBeVisible();
    await expect(page.getByTestId("source-search-input")).toHaveValue(
      "合成新作者",
    );
    await expect(page.getByTestId("source-card-JM:104")).toBeVisible();
    await expect(page.getByTestId("source-filter-count")).toContainText(
      "已读完当前来源的搜索范围",
    );
    expect(
      await page.evaluate(
        () =>
          window.workflowTest.calls.filter(
            ({ command, args }) =>
              command === "source_query" && args.kind === "search",
          ).length,
      ),
    ).toBe(searchCalls);
  }
});

test("sidebar collapse is reversible by pointer and keyboard without losing active navigation", async ({
  page,
}) => {
  const toggle = page.getByTestId("sidebar-toggle");
  const sidebar = page.locator(".sidebar");
  const expanded = (await sidebar.boundingBox())!.width;
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await page.getByTestId("nav-author-search").click();
  await page
    .getByRole("textbox", { name: "搜索作者名" })
    .fill("尚未提交的作者");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect
    .poll(async () => (await sidebar.boundingBox())!.width)
    .toBeLessThan(expanded - 50);
  await expect(page.getByTestId("nav-author-search")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toHaveValue(
    "尚未提交的作者",
  );
  await toggle.focus();
  await page.keyboard.press("Enter");
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect
    .poll(async () => Math.abs((await sidebar.boundingBox())!.width - expanded))
    .toBeLessThan(1);
  await expect(toggle).toBeFocused();
  await page.keyboard.press("Space");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  expect(
    await sidebar.evaluate((element) => element.getAnimations().length),
  ).toBe(0);
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-grid")).toBeVisible();
  await expect(page.getByTestId("nav-library")).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("library and settings searches belong to their pages and retain independent queries", async ({
  page,
}) => {
  const search = page.getByTestId("search-input");
  await expect(page.locator("main").getByTestId("search-input")).toBeVisible();
  await expect(page.locator(".topbar").getByTestId("search-input")).toHaveCount(
    0,
  );
  await search.fill("不存在的合成标题");
  await expect(page.getByTestId("library-grid")).toHaveCount(0);
  await expect(
    page.getByText("没有匹配的电脑作品。", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("nav-settings").click();
  await expect(page.locator("main").getByTestId("search-input")).toBeVisible();
  await search.fill("记住会话");
  await expect(page.getByTestId("settings-accounts")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("nav-library").click();
  await expect(search).toHaveValue("不存在的合成标题");
  await search.fill("");
  await expect(page.getByTestId("library-grid").locator("article")).toHaveCount(
    1,
  );
  await page.getByTestId("nav-favorites").click();
  await expect(
    page.locator("main").getByTestId("source-search-control"),
  ).toBeVisible();
  await expect(
    page.locator(".topbar").getByTestId("source-search-control"),
  ).toHaveCount(0);
});

test("the refined cover menu preserves all three reader choices and a reversible details path", async ({
  page,
}) => {
  const cover = page.getByRole("button", {
    name: "打开《已保存作品》",
    exact: true,
  });
  await cover.click();
  const menu = page.getByTestId("reader-cover-actions");
  for (const name of ["漫画详细", "程序内阅读", "手机小框阅读"]) {
    await expect(menu.getByRole("button", { name, exact: true })).toBeVisible();
    await expect(menu.getByRole("button", { name, exact: true })).toBeEnabled();
  }
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(cover).toBeFocused();
  await cover.click();
  await menu.getByRole("button", { name: "漫画详细", exact: true }).click();
  await expect(page.getByTestId("library-detail")).toContainText("已保存作品");
  await page.getByTestId("library-detail-back").click();
  await expect(cover).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        command.startsWith("reader_"),
      ),
    ),
  ).toEqual([]);
});

test("deep library browsing keeps its book and usable opaque toolbar after both sidebar directions", async ({
  page,
}) => {
  await page.evaluate(() => {
    const library = window.workflowTest.library;
    const sample = library.items[0];
    library.items = Array.from({ length: 320 }, (_, index) => ({
      ...sample,
      id: (10000 + index).toString(16).padStart(64, "0"),
      title: `合成漫画 ${String(index).padStart(4, "0")}`,
      fileName: `合成漫画 ${index}.zip`,
      relativePath: `合成漫画 ${index}.zip`,
      addedAt: 1800000000000 + index,
      sourceRef: null,
      identityEvidence: null,
    }));
    library.visited = library.items.length;
  });
  await page.getByTestId("library-refresh").click();
  await expect(page.getByTestId("library-progress")).toContainText("320");
  const main = page.locator("main");
  await main.evaluate((element) => {
    element.scrollTop = 6000;
  });
  await expect
    .poll(() => main.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(1000);
  const firstVisible = async () =>
    main.evaluate((element) => {
      const viewport = element.getBoundingClientRect();
      return (
        Array.from(
          element.querySelectorAll<HTMLElement>("[data-library-id]"),
        ).find((card) => {
          const box = card.getBoundingClientRect();
          return (
            box.bottom > viewport.top + 30 && box.top < viewport.bottom - 30
          );
        })?.dataset.libraryId ?? null
      );
    });
  await expect.poll(firstVisible).not.toBeNull();
  expect(
    await page.getByTestId("library-grid").locator("article").count(),
  ).toBeLessThan(100);
  const anchor = await firstVisible();
  for (const expanded of ["false", "true"]) {
    await page.getByTestId("sidebar-toggle").click();
    await expect(page.getByTestId("sidebar-toggle")).toHaveAttribute(
      "aria-expanded",
      expanded,
    );
    await expect
      .poll(() =>
        main.evaluate((element, id) => {
          const viewport = element.getBoundingClientRect();
          const card = element.querySelector<HTMLElement>(
            `[data-library-id="${id}"]`,
          );
          if (!card) return false;
          const box = card.getBoundingClientRect();
          return (
            box.bottom > viewport.top &&
            box.top < viewport.bottom &&
            box.left >= viewport.left &&
            box.right <= viewport.right + 1 &&
            element.scrollWidth <= element.clientWidth + 1
          );
        }, anchor),
      )
      .toBe(true);
    await expect(page.locator(".app-shell")).not.toHaveClass(
      /ui-sidebar-moving/,
    );
    const toolbar = page
      .getByTestId("library-workbench")
      .locator(".library-toolbar");
    await expect(toolbar).toHaveClass(/is-stuck/);
    const surface = await toolbar.evaluate((element) => {
      const style = getComputedStyle(element);
      const channels = style.backgroundColor.match(/[\d.]+/g) ?? [];
      return {
        position: style.position,
        zIndex: Number(style.zIndex),
        backgroundAlpha: channels.length === 4 ? Number(channels[3]) : 1,
      };
    });
    expect(surface.position).toBe("sticky");
    expect(surface.zIndex).toBeGreaterThan(0);
    expect(surface.backgroundAlpha).toBeGreaterThanOrEqual(0.9);
    for (const testId of ["search-input", "library-density-7"]) {
      const control = toolbar.getByTestId(testId);
      await expect(control).toBeVisible();
      await expect
        .poll(() =>
          control.evaluate((element) => {
            const bounds = element.getBoundingClientRect();
            const viewport = element.closest("main")!.getBoundingClientRect();
            const hit = document.elementFromPoint(
              (bounds.left + bounds.right) / 2,
              (bounds.top + bounds.bottom) / 2,
            );
            return (
              bounds.top >= viewport.top - 1 &&
              bounds.bottom <= viewport.bottom &&
              hit !== null &&
              element.contains(hit)
            );
          }),
        )
        .toBe(true);
    }
  }
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/refined-library-deep.png",
    animations: "disabled",
  });
});

test("new appearance controls accept old preferences, preview reversibly, and save only the intended settings", async ({
  page,
}) => {
  await page.evaluate(async () => {
    const bridge = (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke(
            command: string,
            args?: Record<string, unknown>,
          ): Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__;
    const original = bridge.invoke;
    window.refinedPreferences = structuredClone(
      await original("read_preferences"),
    ) as Window["refinedPreferences"];
    bridge.invoke = async (command, args = {}) => {
      if (command === "read_preferences") {
        window.workflowTest.calls.push({
          command,
          args: structuredClone(args),
        });
        return structuredClone(window.refinedPreferences);
      }
      if (command === "write_preferences") {
        window.workflowTest.calls.push({
          command,
          args: structuredClone(args),
        });
        if (args.expectedRevision !== window.refinedPreferences.revision)
          throw { code: "CONFLICT" };
        window.refinedPreferences = {
          revision: window.refinedPreferences.revision + 1,
          value: structuredClone(args.value as WorkbenchPreferences),
        };
        return structuredClone(window.refinedPreferences);
      }
      return original(command, args);
    };
  });
  await appearance(page);
  await expect(page.getByTestId("background-shade")).toHaveValue("84");
  await expect(page.getByTestId("background-blur")).toHaveValue("0");
  await expect(page.getByTestId("reduced-motion")).not.toBeChecked();
  await page.getByTestId("background-tone-forest").click();
  await page.getByTestId("reduced-motion").check();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-tone",
    "forest",
  );
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-reduced-motion",
    "true",
  );
  await page.getByTestId("nav-library").click();
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-background-tone",
    "night",
  );
  await expect(page.locator(".app-shell")).toHaveAttribute(
    "data-reduced-motion",
    "false",
  );
  await appearance(page);
  await expect(page.getByTestId("background-tone-night")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("reduced-motion")).not.toBeChecked();
  expect(await page.evaluate(() => window.refinedPreferences.revision)).toBe(0);
  await page.getByTestId("background-tone-forest").click();
  await page.getByTestId("background-shade").focus();
  await page.keyboard.press("End");
  for (let step = 0; step < 5; step++) await page.keyboard.press("ArrowLeft");
  await page.getByTestId("background-blur").focus();
  await page.keyboard.press("Home");
  for (let step = 0; step < 4; step++) await page.keyboard.press("ArrowRight");
  await page.getByTestId("reduced-motion").check();
  await page.getByTestId("save-settings-page").click();
  await expect(page.locator(".settings-save-message")).toContainText(
    "外观已保存到",
  );
  await expect(page.getByTestId("save-settings-page")).toBeDisabled();
  expect(await page.evaluate(() => window.refinedPreferences)).toEqual({
    revision: 1,
    value: {
      version: 1,
      appearance: {
        backgroundMode: "B",
        density: 7,
        backgroundImage: null,
        backgroundName: null,
        refinement: { tone: "forest", shade: 90, blur: 4, reducedMotion: true },
      },
      resources: {
        profile: "balanced",
        simultaneousWorks: 2,
        imageRequests: 4,
      },
    },
  });
  await page.getByTestId("restore-settings-page").click();
  await expect(page.getByTestId("background-tone-night")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("background-shade")).toHaveValue("84");
  await page.getByTestId("nav-library").click();
  await appearance(page);
  await expect(page.getByTestId("background-tone-forest")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("background-shade")).toHaveValue("90");
  await expect(page.getByTestId("background-blur")).toHaveValue("4");
  await expect(page.getByTestId("reduced-motion")).toBeChecked();
  expect(await page.evaluate(() => window.refinedPreferences.revision)).toBe(1);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/refined-appearance-saved.png",
  });
});

test("scope details can be disclosed without starting a check and narrow pages keep actionable controls reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 720 });
  await page.getByTestId("nav-completion").click();
  const scope = page.getByTestId("completion-scope-details");
  await expect(scope.locator("summary")).toContainText("JM 不含 English Manga");
  await expect(page.getByTestId("completion-query-scope")).not.toBeVisible();
  await scope.locator("summary").click();
  await expect(page.getByTestId("completion-query-scope")).toBeVisible();
  await expect(page.getByTestId("completion-check-mode")).toBeVisible();
  await scope.locator("summary").click();
  await expect(page.getByTestId("completion-query-scope")).not.toBeVisible();
  await page.getByTestId("nav-settings").click();
  await expect(page.getByTestId("search-input")).toBeVisible();
  expect(
    await page
      .locator("main")
      .evaluate((element) => element.scrollWidth <= element.clientWidth + 1),
  ).toBe(true);
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-grid")).toBeVisible();
  expect(
    await page
      .locator("main")
      .evaluate((element) => element.scrollWidth <= element.clientWidth + 1),
  ).toBe(true);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/refined-library-narrow.png" });
});
