import { expect, test, type Page } from "@playwright/test";
import { installWorkflow } from "./workflow-fixture.ts";
import type { HistoryEntry } from "../src/history-runtime.ts";

declare global {
  interface Window {
    historyCoverTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      active: number;
      maximumActive: number;
    };
  }
}

async function installHistoryCovers(
  page: Page,
  entries: HistoryEntry[],
  failSourceCover = false,
) {
  await installWorkflow(page);
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-refresh")).toBeEnabled();
  await page.evaluate(
    ({ entries, failSourceCover }) => {
      const canvas = document.createElement("canvas");
      canvas.width = 10;
      canvas.height = 14;
      const context = canvas.getContext("2d")!;
      context.fillStyle = "#7d69a2";
      context.fillRect(0, 0, 10, 14);
      const image = canvas.toDataURL("image/jpeg");
      const original = window.__TAURI_INTERNALS__!.invoke;
      const library = window.workflowTest.library;
      library.items[0].coverAvailable = entries.some(
        (entry) =>
          entry.identity.kind === "library" &&
          entry.identity.rootId === library.rootId &&
          entry.identity.entryId === library.items[0].id,
      );
      const hooks = {
        calls: [] as { command: string; args: Record<string, unknown> }[],
        active: 0,
        maximumActive: 0,
      };
      window.historyCoverTest = hooks;
      window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
        hooks.calls.push({ command, args: structuredClone(args) });
        if (command === "history_read")
          return {
            revision: 1,
            value: {
              version: 1,
              enabled: true,
              entries: structuredClone(entries),
            },
          };
        if (command === "source_cover" || command === "library_cover") {
          hooks.active++;
          hooks.maximumActive = Math.max(hooks.maximumActive, hooks.active);
          try {
            await new Promise((resolve) => setTimeout(resolve, 60));
            if (command === "source_cover" && failSourceCover)
              throw { code: "SOURCE_COVER_ACCESS_DENIED" };
            return command === "source_cover"
              ? {
                  source: args.source,
                  sessionId: args.sessionId,
                  workId: args.workId,
                  dataUrl: image,
                }
              : {
                  rootId: args.rootId,
                  generation: args.generation,
                  entryId: args.entryId,
                  dataUrl: image,
                };
          } finally {
            hooks.active--;
          }
        }
        return original(command, args);
      };
    },
    { entries, failSourceCover },
  );
  await page.getByTestId("nav-library").click();
  await page.getByTestId("library-refresh").click();
  await expect(page.getByTestId("library-refresh")).toBeEnabled();
  await page.getByTestId("nav-history").click();
}

const historySourceEntry = (
  source: "JM" | "Pica",
  workId: string,
): HistoryEntry => ({
  identity: { kind: "source", source, workId },
  title: `合成历史 ${source} ${workId}`,
  visitedAt: 1800000000000,
});

test("history renders both source and local covers without recording visits or querying list details", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 1000 });
  const entries: HistoryEntry[] = [
    historySourceEntry("JM", "701"),
    historySourceEntry("Pica", "000000000000000000000702"),
    {
      identity: {
        kind: "library",
        rootId: "a".repeat(64),
        entryId: (100).toString(16).padStart(64, "0"),
      },
      title: "本地合成浏览记录",
      visitedAt: 1800000000000,
    },
  ];
  await installHistoryCovers(page, entries);
  const history = page.getByTestId("viewing-history");
  await expect(history.locator("li")).toHaveCount(3);
  await expect(history.locator("img")).toHaveCount(3);
  const loaded = await page.evaluate(() =>
    window.historyCoverTest.calls.filter(({ command }) =>
      command.endsWith("_cover"),
    ),
  );
  expect(
    loaded
      .filter(({ command }) => command === "source_cover")
      .map(({ args }) => [args.source, args.workId, args.refreshMetadata])
      .sort(),
  ).toEqual([
    ["JM", "701", false],
    ["Pica", "000000000000000000000702", false],
  ]);
  expect(
    loaded.filter(({ command }) => command === "library_cover"),
  ).toHaveLength(1);
  // Reopening the list keeps the existing scoped cache; neither thumbnail work
  // nor revisiting the history page is itself a deliberate manga open.
  await page.getByTestId("nav-library").click();
  await page.getByTestId("nav-history").click();
  await expect(history.locator("img")).toHaveCount(3);
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls.filter(({ command }) =>
        command.endsWith("_cover"),
      ),
    ),
  ).toEqual(loaded);
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls.filter(
        ({ command, args }) =>
          command === "history_record" ||
          (command === "source_query" && args.kind === "detail"),
      ),
    ),
  ).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
});

test("history explains missing library files and failed source covers while retaining usable history controls", async ({
  page,
}) => {
  await page.setViewportSize({ width: 920, height: 1000 });
  const entries: HistoryEntry[] = [
    historySourceEntry("JM", "778"),
    {
      identity: {
        kind: "library",
        rootId: "a".repeat(64),
        entryId: "d".repeat(64),
      },
      title: "当前目录缺失的合成文件",
      visitedAt: 1800000000000,
    },
    {
      identity: {
        kind: "library",
        rootId: "c".repeat(64),
        entryId: "d".repeat(64),
      },
      title: "其他目录的合成文件",
      visitedAt: 1800000000000,
    },
  ];
  await installHistoryCovers(page, entries, true);
  const history = page.getByTestId("viewing-history");
  await expect(history.locator("li")).toHaveCount(3);
  await expect(
    history.getByText("当前漫画库中未找到此文件", { exact: true }),
  ).toBeVisible();
  await expect(
    history.getByText("此记录来自其他漫画库目录", { exact: true }),
  ).toBeVisible();
  const source = history.getByTestId("source-cover-JM:778");
  await expect(
    source.locator('[data-error-code="SOURCE_COVER_ACCESS_DENIED"]'),
  ).toContainText("账号会话未因此失效");
  await source.getByRole("button", { name: "重试读取", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.historyCoverTest.calls.filter(
            ({ command }) => command === "source_cover",
          ).length,
      ),
    )
    .toBe(2);
  await expect(
    source.locator('[data-error-code="SOURCE_COVER_ACCESS_DENIED"]'),
  ).toBeVisible();
  expect(
    await source.evaluate((element) => {
      const message = element.querySelector<HTMLElement>("[data-error-code]")!;
      const retry = element.querySelector<HTMLButtonElement>("button")!;
      return (
        message.getBoundingClientRect().bottom <=
          retry.getBoundingClientRect().top &&
        message.scrollHeight <= message.clientHeight
      );
    }),
  ).toBe(true);
  await expect(history.getByLabel("记录浏览历史")).toBeChecked();
  await expect(
    history.getByRole("button", { name: "清空历史", exact: true }),
  ).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls
        .filter(({ command }) => command === "source_cover")
        .map(({ args }) => args.refreshMetadata),
    ),
  ).toEqual([false, true]);
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls.filter(
        ({ command, args }) =>
          command === "history_record" ||
          command === "library_cover" ||
          (command === "source_query" && args.kind === "detail"),
      ),
    ),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.accounts.map(({ state }) => state),
    ),
  ).toEqual(["connected", "connected"]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
});

test("one hundred history rows bound offscreen cover work and retain the scrolled position with cache reuse", async ({
  page,
}) => {
  await page.setViewportSize({ width: 920, height: 600 });
  const entries = Array.from({ length: 100 }, (_, index) =>
    historySourceEntry("JM", String(10001 + index)),
  );
  await installHistoryCovers(page, entries);
  const history = page.getByTestId("viewing-history");
  await expect(history.locator("li")).toHaveCount(100);
  await expect(
    history.getByTestId("source-cover-JM:10001").locator("img"),
  ).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => window.historyCoverTest.active))
    .toBe(0);
  const initial = await page.evaluate(() =>
    window.historyCoverTest.calls.filter(
      ({ command }) => command === "source_cover",
    ),
  );
  expect(initial.length).toBeGreaterThan(0);
  expect(initial.length).toBeLessThan(20);
  expect(initial.some(({ args }) => args.workId === "10100")).toBe(false);
  const lastRow = history.locator("li").last();
  await lastRow.scrollIntoViewIfNeeded();
  await expect(
    history.getByTestId("source-cover-JM:10100").locator("img"),
  ).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => window.historyCoverTest.active))
    .toBe(0);
  const atEnd = await page.evaluate(() =>
    window.historyCoverTest.calls.filter(
      ({ command }) => command === "source_cover",
    ),
  );
  expect(atEnd.length).toBeGreaterThan(initial.length);
  expect(atEnd.length).toBeLessThan(40);
  expect(new Set(atEnd.map(({ args }) => args.workId)).size).toBe(atEnd.length);
  expect(
    await page.evaluate(() => window.historyCoverTest.maximumActive),
  ).toBeLessThanOrEqual(4);
  await page.getByTestId("nav-library").click();
  await page.getByTestId("nav-history").click();
  await expect(lastRow).toBeInViewport();
  await expect(
    history.getByTestId("source-cover-JM:10100").locator("img"),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls.filter(
        ({ command }) => command === "source_cover",
      ),
    ),
  ).toEqual(atEnd);
  expect(
    await page.evaluate(() =>
      window.historyCoverTest.calls.filter(
        ({ command, args }) =>
          command === "history_record" ||
          (command === "source_query" && args.kind === "detail"),
      ),
    ),
  ).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
});

test("history recording switch exposes pending storage and restores the saved value on failure", async ({
  page,
}) => {
  await installWorkflow(page);
  await page.getByTestId("nav-history").click();
  await expect(page.getByLabel("记录浏览历史")).toBeEnabled();
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__!.invoke;
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      if (command === "history_set_enabled") {
        await new Promise((resolve) => setTimeout(resolve, 400));
        throw { code: "STORE_UNAVAILABLE" };
      }
      return original(command, args);
    };
  });
  await page.getByLabel("记录浏览历史").uncheck();
  await expect(page.getByText("正在保存记录设置…")).toBeVisible();
  await expect(page.getByLabel("记录浏览历史")).toBeDisabled();
  await expect(
    page.getByText("浏览历史暂时无法更新，已有记录保留；可重新读取。"),
  ).toBeVisible();
  await expect(page.getByLabel("记录浏览历史")).toBeChecked();
  await expect(page.getByLabel("记录浏览历史")).toBeEnabled();
});

test("history records only deliberate opens, persists, can be disabled and clears no other state", async ({
  page,
}) => {
  await installWorkflow(page);
  await page.getByTestId("nav-history").click();
  await expect(page.getByText("暂无浏览记录。", { exact: true })).toBeVisible();
  await page.getByTestId("nav-library").click();
  await page
    .getByRole("button", { name: "打开《已保存作品》" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "作品详细", exact: true }).click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("nav-history").click();
  await expect(page.locator(".viewing-history-list li")).toHaveCount(1);
  await page.locator(".viewing-history-title").click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.reload();
  await page.getByTestId("nav-history").click();
  await expect(page.locator(".viewing-history-list li")).toHaveCount(1);
  await page.getByLabel("记录浏览历史").uncheck();
  await expect(page.getByText("记录已关闭，现有历史仍可查看。")).toBeVisible();
  const original = await page.evaluate(() => ({
    library: window.workflowTest.library,
    queue: window.workflowTest.queue,
  }));
  await page.getByRole("button", { name: "清空历史", exact: true }).click();
  await page.getByRole("button", { name: "确认清空", exact: true }).click();
  await expect(page.locator(".viewing-history-list li")).toHaveCount(0);
  await page.getByTestId("nav-library").click();
  const back = page.getByTestId("library-detail-back");
  if (await back.isVisible()) await back.click();
  await page
    .getByRole("button", { name: "打开《已保存作品》" })
    .click({ button: "right" });
  await page.getByRole("menuitem", { name: "作品详细", exact: true }).click();
  await page.getByTestId("nav-history").click();
  await expect(page.locator(".viewing-history-list li")).toHaveCount(0);
  expect(
    await page.evaluate(() => ({
      library: window.workflowTest.library,
      queue: window.workflowTest.queue,
    })),
  ).toEqual(original);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
});
