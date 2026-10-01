import { expect, test } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { installWorkflow } from "./workflow-fixture.ts";
import { openUnifiedSearch } from "./browse-ui-helpers.ts";

test("author tabs stream cached results, isolate state, retain positions and ignore closed-tab replies", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1672, height: 1020 });
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
  await expect(page.getByTestId("author-search-timings")).not.toContainText(
    "首次可见：尚未记录",
  );
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
  await openUnifiedSearch(page);
  await expect
    .poll(() => page.locator("main").evaluate((el) => el.scrollTop))
    .toBeGreaterThan(scroll - 20);
  await page.screenshot({ path: "visual-evidence/author-workspace-tabs.png" });
  expect(errors).toEqual([]);
});
