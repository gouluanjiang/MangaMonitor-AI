import { expect, test } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { installWorkflow } from "./workflow-fixture.ts";
import { openUnifiedSearch } from "./browse-ui-helpers.ts";
import { browseCacheLimits } from "../src/browse-session.ts";

test("many author tabs wrap at narrow widths with complete names, visible close buttons and retained keyboard and tab state", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 880, height: 900 });
  await installWorkflow(page);
  const longName =
    "完整合成作者名（" + "LongUnbrokenAuthorName".repeat(6) + "）";
  const names = [
    ...Array.from(
      { length: 12 },
      (_, index) => `合成作者标签 ${String(index + 1).padStart(2, "0")}`,
    ),
    longName,
  ];
  await page.evaluate((names) => {
    const original = window.__TAURI_INTERNALS__!.invoke;
    const calls: Record<string, unknown>[] = [];
    Object.assign(window, { wrappedAuthorTest: { calls } });
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      if (command === "source_author_known_works")
        return {
          ...args,
          items: [],
          checkedAt: 1000,
          discoveryRevision: 0,
          historyComplete: true,
        };
      if (command === "source_query" && args.kind === "author") {
        calls.push(structuredClone(args));
        const name = String(args.query),
          source = String(args.source);
        const offset = (names.indexOf(name) + 1) * 1000;
        return {
          ...args,
          items: Array.from({ length: 20 }, (_, index) => ({
            source,
            workId:
              source === "JM"
                ? String(offset + index + 1)
                : (offset + index + 1).toString(16).padStart(24, "0"),
            title: `${name} 的合成作品 ${index + 1}`,
            authors: [name],
            tags: index === 0 ? ["AI生成"] : [],
            description: null,
            favorite: null,
            chapterCount: null,
            pageCount: null,
            coverAvailable: false,
            sourceUpdatedAt: "2026-09-01T00:00:00Z",
          })),
          page: 1,
          pages: 1,
          total: 20,
          hasMore: false,
          folders: [],
        };
      }
      return original(command, args);
    };
  }, names);
  await openUnifiedSearch(page);
  for (const name of names) {
    await page.getByLabel("搜索作者名").fill(name);
    await page.getByTestId("completion-start").click();
    await expect(page.getByRole("tab", { name, exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByTestId("completion-counts")).toContainText(
      "当前检查范围已读完",
    );
  }
  const tablist = page.getByRole("tablist", { name: "已打开的作者" });
  await expect(tablist.getByRole("tab")).toHaveCount(names.length);
  await expect(tablist.getByRole("tab")).toHaveText(names);
  for (const width of [880, 560]) {
    await page.setViewportSize({ width, height: 900 });
    const geometry = await tablist.evaluate((element) => {
      const bounds = element.getBoundingClientRect();
      const tabs = Array.from(
        element.querySelectorAll<HTMLElement>(".author-tab"),
      );
      return {
        rows: new Set(
          tabs.map((tab) => Math.round(tab.getBoundingClientRect().top)),
        ).size,
        horizontalOverflow: element.scrollWidth > element.clientWidth + 1,
        tabBounds: tabs.map((tab) => {
          const rect = tab.getBoundingClientRect();
          const label = tab.querySelector<HTMLElement>('[role="tab"]')!;
          const close = tab.querySelector<HTMLElement>(".author-tab-close")!;
          const closeRect = close.getBoundingClientRect();
          return {
            withinList:
              rect.left >= bounds.left - 1 && rect.right <= bounds.right + 1,
            completeLabel:
              label.scrollWidth <= label.clientWidth + 1 &&
              label.scrollHeight <= label.clientHeight + 1,
            closeVisible:
              closeRect.width >= 32 &&
              closeRect.height >= 32 &&
              getComputedStyle(close).visibility === "visible",
            closeWithinTab:
              closeRect.left >= rect.left &&
              closeRect.right <= rect.right &&
              closeRect.top >= rect.top &&
              closeRect.bottom <= rect.bottom,
          };
        }),
      };
    });
    expect(geometry.rows).toBeGreaterThanOrEqual(3);
    expect(geometry.horizontalOverflow).toBe(false);
    expect(geometry.tabBounds).toEqual(
      names.map(() => ({
        withinList: true,
        completeLabel: true,
        closeVisible: true,
        closeWithinTab: true,
      })),
    );
  }
  const firstTab = tablist.getByRole("tab", { name: names[0], exact: true });
  const longTab = tablist.getByRole("tab", { name: longName, exact: true });
  expect(
    await longTab.evaluate((element) => {
      const style = getComputedStyle(element);
      return (
        style.whiteSpace === "normal" &&
        style.textOverflow !== "ellipsis" &&
        element.getBoundingClientRect().height >
          Number.parseFloat(style.lineHeight) * 2
      );
    }),
  ).toBe(true);
  await longTab.focus();
  await longTab.press("Home");
  await expect(firstTab).toBeFocused();
  await expect(firstTab).toHaveAttribute("aria-selected", "true");
  await firstTab.press("End");
  await expect(longTab).toBeFocused();
  await longTab.press("ArrowRight");
  await expect(firstTab).toBeFocused();
  await firstTab.press("ArrowLeft");
  await expect(longTab).toBeFocused();
  // Filters and result ownership remain per tab, including an explicitly tagged
  // result that is removed immediately without any detail verification query.
  await firstTab.click();
  await page.getByLabel("更新来源").selectOption("JM");
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  await page.getByTestId("author-update-JM:1002").scrollIntoViewIfNeeded();
  await expect(page.getByTestId("author-update-JM:1002")).toBeVisible();
  await expect(page.getByTestId("author-update-JM:1001")).toHaveCount(0);
  await longTab.click();
  await expect(page.getByLabel("更新来源")).toHaveValue("all");
  await page.getByLabel("更新来源").selectOption("Pica");
  await firstTab.click();
  await expect(page.getByLabel("更新来源")).toHaveValue("JM");
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-asc");
  await longTab.click();
  await expect(page.getByLabel("更新来源")).toHaveValue("Pica");
  // A tab switch restores its grid anchor across frames. Real wheel input
  // interrupts that restore; assigning scrollTop bypasses the user-input path
  // and lets a pending restore overwrite the test's scroll on slower runners.
  const main = page.getByRole("main");
  await main.hover();
  await page.mouse.wheel(
    0,
    await main.evaluate((element) => element.scrollHeight),
  );
  const lastWork = page.getByTestId(
    "author-update-Pica:" + (13020).toString(16).padStart(24, "0"),
  );
  await expect(lastWork).toBeInViewport();
  const scroll = await page
    .locator("main")
    .evaluate((element) => element.scrollTop);
  await page.getByTestId("nav-library").click();
  await page.getByTestId("nav-discovery").click();
  await expect(longTab).toHaveAttribute("aria-selected", "true");
  await expect(lastWork).toBeInViewport();
  await expect
    .poll(() => page.locator("main").evaluate((element) => element.scrollTop))
    .toBeGreaterThan(scroll - 20);
  const closeLong = tablist.getByRole("button", {
    name: `关闭作者标签 ${longName}`,
    exact: true,
  });
  await closeLong.focus();
  await closeLong.press("Enter");
  await expect(longTab).toHaveCount(0);
  const previousTab = tablist.getByRole("tab", {
    name: names[11],
    exact: true,
  });
  await expect(previousTab).toHaveAttribute("aria-selected", "true");
  await expect(previousTab).toBeFocused();
  await expect(page.getByLabel("搜索作者名")).toHaveValue(names[11]);
  await tablist
    .getByRole("button", { name: `关闭作者标签 ${names[0]}`, exact: true })
    .click();
  await expect(firstTab).toHaveCount(0);
  await expect(previousTab).toHaveAttribute("aria-selected", "true");
  await expect(tablist.getByRole("tab")).toHaveCount(names.length - 2);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { wrappedAuthorTest: { calls: unknown[] } })
          .wrappedAuthorTest.calls.length,
    ),
  ).toBe(names.length * 2);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        ({ command, args }) =>
          command === "source_query" && args.kind === "detail",
      ),
    ),
  ).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/author-workspace-wrapped-tabs.png",
  });
  expect(errors).toEqual([]);
});

test("author tabs stream cached results, isolate state, retain positions and ignore closed-tab replies", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1672, height: 620 });
  await installWorkflow(page);
  await page.evaluate(() => {
    const previous = window.__TAURI_INTERNALS__!.invoke;
    const held: Record<string, () => void> = {};
    Object.assign(window, { authorTabTest: { held, calls: [] } });
    const make = (source: string, n: number, author: string) => ({
      source,
      workId: source === "JM" ? String(n) : n.toString(16).padStart(24, "0"),
      title: `${author} ${n}`,
      authors: [author],
      tags: [],
      description: null,
      favorite: null,
      chapterCount: null,
      pageCount: null,
      coverAvailable: false,
      sourceUpdatedAt: "2026-09-01T00:00:00Z",
    });
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      if (command === "source_author_known_works")
        return {
          ...args,
          items: [make(String(args.source), 1, String(args.author))],
          checkedAt: 1000,
          discoveryRevision: 0,
          historyComplete: true,
        };
      if (command === "source_query" && args.kind === "author") {
        const hooks = (
          window as unknown as { authorTabTest: { calls: unknown[] } }
        ).authorTabTest;
        hooks.calls.push(args);
        const name = String(args.query),
          source = String(args.source);
        if (name === "Beta" && source === "JM")
          await new Promise<void>((resolve) => {
            held.Beta = resolve;
          });
        else
          await new Promise((resolve) =>
            setTimeout(resolve, source === "Pica" ? 600 : 150),
          );
        return {
          ...args,
          items: Array.from({ length: 60 }, (_, i) =>
            make(source, i + 1, name),
          ),
          page: 1,
          pages: 1,
          total: 60,
          hasMore: false,
          folders: [],
          timing: {
            queueMs: 0,
            sourceOperationMs: source === "Pica" ? 600 : 150,
            localCommitMs: 0,
          },
        };
      }
      return previous(command, args);
    };
  });
  await openUnifiedSearch(page);
  await page.getByLabel("搜索作者名").fill("Alpha");
  await page.getByTestId("completion-start").click();
  await expect(
    page.getByRole("tab", { name: "Alpha", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("author-update-JM:1")).toBeVisible();
  await expect(page.getByTestId("author-search-cache-status")).toContainText(
    "尚未读取完整",
  );
  await expect(page.getByTestId("completion-counts")).toContainText(
    "当前检查范围已读完",
  );
  await page.getByTestId("author-search-timings").locator("summary").click();
  await expect(page.getByTestId("author-search-timings")).toContainText(
    "首次可见：尚未记录",
  );
  // Skip the initially mounted rows, as when dragging the scrollbar to the end.
  await page.locator("main").evaluate((el) => {
    el.scrollTop = el.scrollHeight;
    el.dispatchEvent(new Event("scroll"));
  });
  await expect(
    page.getByTestId("author-update-Pica:00000000000000000000003c"),
  ).toBeVisible();
  await expect(page.getByTestId("author-search-timings")).not.toContainText(
    "首次可见：尚未记录",
  );
  await page.setViewportSize({ width: 1672, height: 1020 });
  await page.locator("main").evaluate((el) => {
    el.scrollTop = 0;
    el.dispatchEvent(new Event("scroll"));
  });
  await expect(page.getByTestId("author-update-JM:1")).toBeVisible();
  await mkdir("visual-evidence", { recursive: true });
  await writeFile(
    "visual-evidence/author-search-synthetic-timings.txt",
    await page.getByTestId("author-search-timings").innerText(),
  );
  await page.getByLabel("更新来源").selectOption("JM");
  await page.getByTestId("completion-sort").selectOption("updated-asc");
  await page.getByRole("button", { name: "多选", exact: true }).click();
  await page.getByTestId("author-update-JM:1").getByRole("checkbox").check();
  await page.locator("main").evaluate((el) => {
    el.scrollTop = 800;
    el.dispatchEvent(new Event("scroll"));
  });
  await page.waitForTimeout(160);
  const scroll = await page.locator("main").evaluate((el) => el.scrollTop);
  // Open a second author while the first tab retains its own controls and selection.
  await page.getByLabel("搜索作者名").fill("Beta");
  await page.getByTestId("completion-start").click();
  await expect(
    page.getByRole("tab", { name: "Beta", exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("更新来源")).toHaveValue("all");
  await expect(page.getByRole("toolbar", { name: "批量下载操作" })).toHaveCount(
    0,
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          !!(
            window as unknown as {
              authorTabTest: { held: Record<string, unknown> };
            }
          ).authorTabTest.held.Beta,
      ),
    )
    .toBe(true);
  await page.getByRole("button", { name: "关闭作者标签 Beta" }).click();
  await page.evaluate(() =>
    (
      window as unknown as { authorTabTest: { held: { Beta: () => void } } }
    ).authorTabTest.held.Beta(),
  );
  await expect(
    page.getByRole("tab", { name: "Beta", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByLabel("更新来源")).toHaveValue("JM");
  await expect(page.getByTestId("completion-sort")).toHaveValue("updated-asc");
  await expect(
    page.getByRole("toolbar", { name: "批量下载操作" }),
  ).toContainText("已选 1 本");
  // A source-specific author click with the same complete name reuses this tab.
  const before = await page.evaluate(
    () =>
      (window as unknown as { authorTabTest: { calls: unknown[] } })
        .authorTabTest.calls.length,
  );
  await page.getByLabel("搜索作者名").fill("Alpha");
  await page.getByTestId("completion-start").click();
  await expect(
    page.getByRole("tab", { name: "Alpha", exact: true }),
  ).toHaveCount(1);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { authorTabTest: { calls: unknown[] } })
          .authorTabTest.calls.length,
    ),
  ).toBe(before);
  // Position is checked after navigation, not after deliberately focusing a top form.
  await page.locator("main").evaluate((el, top) => {
    el.scrollTop = top;
    el.dispatchEvent(new Event("scroll"));
  }, scroll);
  await page.waitForTimeout(160);
  await page.getByTestId("nav-library").click();
  // The sidebar restores the retained search mode. Clicking the already active
  // mode control would intentionally scroll that top-of-page control into view.
  await page.getByTestId("nav-discovery").click();
  await expect
    .poll(() => page.locator("main").evaluate((el) => el.scrollTop))
    .toBeGreaterThan(scroll - 20);
  await page.screenshot({ path: "visual-evidence/author-workspace-tabs.png" });
  expect(errors).toEqual([]);
});

test("a hidden open author's current position survives eviction of old query variants", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await installWorkflow(page);
  await page.evaluate(() => {
    const previous = window.__TAURI_INTERNALS__!.invoke;
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      if (command === "source_author_known_works")
        return {
          ...args,
          items: [],
          checkedAt: 1000,
          discoveryRevision: 0,
          historyComplete: true,
        };
      if (command === "source_query" && args.kind === "author") {
        const source = String(args.source),
          author = String(args.query);
        return {
          ...args,
          items: Array.from({ length: 60 }, (_, i) => ({
            source,
            workId:
              source === "JM"
                ? String(i + 1)
                : (i + 1).toString(16).padStart(24, "0"),
            title: `${author} ${i + 1}`,
            authors: [author],
            tags: [],
            description: null,
            favorite: null,
            chapterCount: null,
            pageCount: null,
            coverAvailable: false,
            sourceUpdatedAt: "2026-09-01T00:00:00Z",
          })),
          page: 1,
          pages: 1,
          total: 60,
          hasMore: false,
          folders: [],
        };
      }
      return previous(command, args);
    };
  });
  await openUnifiedSearch(page);
  for (const name of ["Retained", "Query pressure"]) {
    await page.getByLabel("搜索作者名").fill(name);
    await page.getByTestId("completion-start").click();
    await expect(page.getByTestId("completion-counts")).toContainText(
      "当前检查范围已读完",
    );
  }
  const retained = page.getByRole("tab", { name: "Retained", exact: true });
  const pressure = page.getByRole("tab", {
    name: "Query pressure",
    exact: true,
  });
  // Programmatic tab activation avoids scrolling the top toolbar into view.
  await retained.evaluate((button: HTMLButtonElement) => button.click());
  await expect(retained).toHaveAttribute("aria-selected", "true");
  await page.getByRole("main").hover();
  await page.mouse.wheel(0, 800);
  await expect
    .poll(() => page.getByRole("main").evaluate((main) => main.scrollTop))
    .toBeGreaterThan(700);
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const position = await page
    .getByRole("main")
    .evaluate((main) => main.scrollTop);
  await pressure.evaluate((button: HTMLButtonElement) => button.click());
  await expect(pressure).toHaveAttribute("aria-selected", "true");
  const variants = browseCacheLimits.positions + 8;
  await page
    .getByLabel("筛选作者更新")
    .evaluate(async (input: HTMLInputElement, count) => {
      const setValue = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )!.set!;
      for (let i = 1; i <= count; i++) {
        // Distinct controls/scopes, identical matching rows; no extra source calls.
        setValue.call(input, " ".repeat(i));
        input.dispatchEvent(new Event("input", { bubbles: true }));
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        );
      }
    }, variants);
  await expect(page.getByLabel("筛选作者更新")).toHaveValue(
    " ".repeat(variants),
  );
  await retained.evaluate((button: HTMLButtonElement) => button.click());
  await expect
    .poll(async () =>
      Math.abs(
        (await page.getByRole("main").evaluate((main) => main.scrollTop)) -
          position,
      ),
    )
    .toBeLessThanOrEqual(2);
  await page
    .getByRole("button", { name: "关闭作者标签 Query pressure", exact: true })
    .click();
  await expect(pressure).toHaveCount(0);
  await expect(retained).toHaveAttribute("aria-selected", "true");
});

for (const interrupt of [false, true]) {
  test(
    interrupt
      ? "user scrolling cancels an author's pending restore before delayed rows are measured"
      : "an author's bottom anchor survives delayed row measurement and a second page round trip",
    async ({ page }) => {
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.setViewportSize({ width: 560, height: 900 });
      await installWorkflow(page);
      const author =
        "完整合成作者名（" + "LongUnbrokenAuthorName".repeat(6) + "）";
      await page.evaluate((author) => {
        const original = window.__TAURI_INTERNALS__!.invoke;
        window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
          if (command === "source_author_known_works")
            return {
              ...args,
              items: [],
              checkedAt: 1000,
              discoveryRevision: 0,
              historyComplete: true,
            };
          if (command === "source_query" && args.kind === "author") {
            const source = String(args.source);
            return {
              ...args,
              items: Array.from({ length: 20 }, (_, index) => ({
                source,
                workId:
                  source === "JM"
                    ? String(1001 + index)
                    : (1001 + index).toString(16).padStart(24, "0"),
                title: `${author} 的合成作品 ${index + 1}`,
                authors: [author],
                tags: [],
                description: null,
                favorite: null,
                chapterCount: null,
                pageCount: null,
                coverAvailable: false,
                sourceUpdatedAt: "2026-09-01T00:00:00Z",
              })),
              page: 1,
              pages: 1,
              total: 20,
              hasMore: false,
              folders: [],
            };
          }
          return original(command, args);
        };
      }, author);
      await openUnifiedSearch(page);
      await page.getByLabel("搜索作者名").fill(author);
      await page.getByTestId("completion-start").click();
      await expect(page.getByTestId("completion-counts")).toContainText(
        "当前检查范围已读完",
      );
      await page.getByLabel("更新来源").selectOption("Pica");
      const panel = page.getByRole("tabpanel");
      const grid = panel.locator(".source-virtual-grid");
      const main = page.getByRole("main");
      const lastWork = page.getByTestId(
        "author-update-Pica:" + (1020).toString(16).padStart(24, "0"),
      );
      const sampleFrames = (count = 8) =>
        grid.evaluate(
          (element, count) =>
            new Promise<
              {
                scroll: number;
                scrollHeight: number;
                viewport: number;
                gridHeight: number;
                columns: number;
                fallbackStride: number;
                measuredStride: number;
              }[]
            >((resolve) => {
              const values: {
                scroll: number;
                scrollHeight: number;
                viewport: number;
                gridHeight: number;
                columns: number;
                fallbackStride: number;
                measuredStride: number;
              }[] = [];
              const sample = () => {
                const main = element.closest("main")!;
                const css = getComputedStyle(element);
                const columns = css.gridTemplateColumns
                  .split(" ")
                  .filter(Boolean).length;
                const gap = Number.parseFloat(css.rowGap);
                const width =
                  (element.clientWidth -
                    Number.parseFloat(css.columnGap) * (columns - 1)) /
                  columns;
                const measured = Math.max(
                  0,
                  ...Array.from(
                    element.querySelectorAll<HTMLElement>(
                      ".source-virtual-row",
                    ),
                    (row) =>
                      row.dataset.columns === String(columns)
                        ? row.getBoundingClientRect().height
                        : 0,
                  ),
                );
                values.push({
                  scroll: main.scrollTop,
                  scrollHeight: main.scrollHeight,
                  viewport: main.clientHeight,
                  gridHeight: element.getBoundingClientRect().height,
                  columns,
                  fallbackStride: (width * 7) / 5 + 90 + gap,
                  measuredStride: measured ? measured + gap : 0,
                });
                if (values.length === count) resolve(values);
                else requestAnimationFrame(sample);
              };
              requestAnimationFrame(sample);
            }),
          count,
        );
      const evidence: { phase: string; frames: unknown[] }[] = [];
      const record = async (phase: string, count = 8) => {
        const frames = await sampleFrames(count);
        evidence.push({ phase, frames });
        return frames;
      };
      const artifact = interrupt
        ? "author-restore-user-cancellation"
        : "author-restore-delayed-measurement";
      try {
        await expect(grid).toHaveAttribute("data-total-items", "20");
        const ready = (await record("initial-measurement", 16)).at(-1)!;
        expect(ready.columns).toBe(2);
        expect(ready.measuredStride).toBeGreaterThan(ready.fallbackStride + 50);
        await main.hover();
        await page.mouse.wheel(
          0,
          await main.evaluate((element) => element.scrollHeight),
        );
        await expect(lastWork).toBeInViewport();
        const before = await record("before-navigation", 16);
        const scroll = before.at(-1)!.scroll;
        expect(before.slice(-8).every((frame) => frame.scroll === scroll)).toBe(
          true,
        );
        expect(scroll).toBeGreaterThan(1000);
        const panelSelector = await panel.evaluate(
          (element) => "#" + CSS.escape(element.id),
        );
        await page.getByTestId("nav-library").click();
        // Hold only the first real row measurement across the remount. The
        // grid, CSS columns, browser scroll clamping and ResizeObserver remain
        // real; no production state or browser measurement API is replaced.
        const heldRows = await page.addStyleTag({
          content: `${panelSelector} .source-virtual-row { display: none !important; }`,
        });
        await page.getByTestId("nav-discovery").click();
        const held = await record("measurement-held", 8);
        const estimated = held.at(-1)!;
        expect(estimated.columns).toBe(2);
        expect(held.every((frame) => frame.measuredStride === 0)).toBe(true);
        expect(
          Math.abs(estimated.gridHeight - 10 * estimated.fallbackStride),
        ).toBeLessThanOrEqual(2);
        expect(estimated.scroll).toBeLessThan(scroll - 100);
        if (interrupt) {
          // Real upward input must supersede the saved bottom anchor, including
          // while its rows are still awaiting their first usable measurement.
          await main.hover();
          await page.mouse.wheel(0, -estimated.scrollHeight);
          await expect
            .poll(() => main.evaluate((element) => element.scrollTop))
            .toBe(0);
          await record("user-returned-to-origin");
        }
        await heldRows.evaluate((style) => style.remove());
        const expected = interrupt ? 0 : scroll;
        await record("measurement-released", 16);
        if (!interrupt) await expect(lastWork).toBeInViewport();
        await expect
          .poll(async () =>
            Math.abs(
              (await main.evaluate((element) => element.scrollTop)) - expected,
            ),
          )
          .toBeLessThanOrEqual(2);
        // A visually successful first restore must also retain the right
        // cached position after the hook's own settling period.
        await page.getByTestId("nav-library").click();
        await page.getByTestId("nav-discovery").click();
        await record("second-round-trip", 16);
        if (!interrupt) await expect(lastWork).toBeInViewport();
        await expect
          .poll(async () =>
            Math.abs(
              (await main.evaluate((element) => element.scrollTop)) - expected,
            ),
          )
          .toBeLessThanOrEqual(2);
        expect(
          await page.evaluate(() => window.workflowTest.unexpectedCommands),
        ).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await mkdir("visual-evidence", { recursive: true });
        await writeFile(
          `visual-evidence/${artifact}.json`,
          JSON.stringify(evidence, null, 2) + "\n",
        );
        await page.screenshot({ path: `visual-evidence/${artifact}.png` });
      }
    },
  );
}
