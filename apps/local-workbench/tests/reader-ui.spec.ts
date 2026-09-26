import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { installWorkflow } from "./workflow-fixture.ts";
import type { ReaderPosition } from "../src/reader/types.ts";

declare global {
  interface Window {
    readerTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      saved: ReaderPosition | null;
      fail: number | null;
      failChapter: string | null;
      firstChapterPages: number;
      imageHeight: number;
      hold: boolean;
      release?: () => void;
      holdOpen: boolean;
      releaseOpen?: () => void;
    };
  }
}
const faults = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  faults.set(page, errors);
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/*", (route) =>
    new URL(route.request().url()).hostname === "127.0.0.1"
      ? route.continue()
      : route.abort(),
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await installWorkflow(page);
  await page.evaluate(() => {
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
    const state = (window.readerTest = {
      calls: [],
      saved: null,
      fail: null,
      failChapter: null,
      firstChapterPages: 10000,
      imageHeight: 1000,
      hold: false,
      holdOpen: false,
    } as Window["readerTest"]);
    let sequence = 0;
    bridge.invoke = async (command, args = {}) => {
      if (!command.startsWith("reader_")) return original(command, args);
      state.calls.push({ command, args: structuredClone(args) });
      switch (command) {
        case "reader_open": {
          if (state.holdOpen)
            await new Promise<void>((resolve) => {
              state.releaseOpen = resolve;
            });
          const request = args.request as {
            kind: string;
            source?: string;
            workId?: string;
          };
          return {
            readerId: `reader-${++sequence}`,
            title: "合成测试漫画",
            origin: request.kind === "library" ? "library" : request.source,
            sourceRef: {
              source: request.source ?? "JM",
              workId: request.workId ?? "101",
            },
            chapters: [
              { id: "one", title: "第一章", pageCount: null },
              { id: "two", title: "第二章", pageCount: null },
            ],
            position: state.saved,
          };
        }
        case "reader_chapter":
          if (state.failChapter === args.chapterId)
            throw { code: "SOURCE_UNAVAILABLE" };
          return {
            readerId: args.readerId,
            chapterId: args.chapterId,
            pageCount: args.chapterId === "one" ? state.firstChapterPages : 3,
          };
        case "reader_page": {
          if (state.hold && args.chapterId === "one" && args.pageIndex === 0)
            await new Promise<void>((resolve) => {
              state.release = resolve;
            });
          if (state.fail === args.pageIndex)
            throw { code: "READER_IMAGE_DECODE" };
          const canvas = document.createElement("canvas");
          canvas.width = 720;
          canvas.height = state.imageHeight;
          const context = canvas.getContext("2d")!;
          context.fillStyle = args.chapterId === "one" ? "#593982" : "#245e47";
          context.fillRect(0, 0, 720, state.imageHeight);
          context.fillStyle = "#fff";
          context.font = "50px sans-serif";
          context.fillText(
            `Synthetic ${args.chapterId} / ${args.pageIndex}`,
            25,
            120,
          );
          return {
            readerId: args.readerId,
            chapterId: args.chapterId,
            pageIndex: args.pageIndex,
            dataUrl: canvas.toDataURL("image/png"),
            width: 720,
            height: state.imageHeight,
          };
        }
        case "reader_save_position":
          state.saved = structuredClone(args.position as ReaderPosition);
          return null;
        case "reader_close":
        case "reader_fullscreen":
        case "reader_cancel_open":
          return null;
        default:
          throw new Error("Unexpected synthetic reader command " + command);
      }
    };
  });
});
test.afterEach(async ({ page }) => {
  expect(faults.get(page)).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /confirm|favorite|follow$|delete|promote|replace/.test(command),
      ),
    ),
  ).toEqual([]);
});
async function showToolbar(page: Page) {
  const viewport = page.viewportSize()!;
  await page.mouse.move(viewport.width / 2, viewport.height - 3);
  await expect(page.getByRole("toolbar", { name: "阅读工具" })).toHaveCSS(
    "opacity",
    "1",
  );
}
async function openLibrary(page: Page, pageCount = 10000) {
  await page.getByTestId("nav-library").click();
  await page
    .getByRole("button", { name: "打开《已保存作品》", exact: true })
    .click();
  await page.getByRole("button", { name: "直接阅读", exact: true }).click();
  await expect(page.getByTestId("comic-reader")).toBeVisible();
  await expect(page.getByLabel("当前页码")).toContainText(String(pageCount));
}
async function openOnline(page: Page) {
  await page.getByTestId("nav-completion").click();
  await page.getByTestId("completion-start").click();
  await expect(page.getByTestId("completion-progress")).toBeVisible();
  await page.evaluate(() => window.workflowTest.finishCheck());
  await page
    .getByTestId("author-update-JM:102")
    .getByRole("button", { name: /打开/ })
    .click();
  await page.getByRole("button", { name: "直接阅读", exact: true }).click();
  await expect(page.getByTestId("comic-reader")).toBeVisible();
}
async function jump(page: Page, number: number) {
  await showToolbar(page);
  await page
    .getByRole("slider", { name: "阅读进度" })
    .evaluate((input, value) => {
      Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )!.set!.call(input, String(value));
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }, number);
  await expect(page.getByLabel("当前页码")).toHaveText(`${number} / 10000`);
}

test("local cover offers reading and details; large chapters stay virtual and reopening restores only position", async ({
  page,
}) => {
  await page.getByTestId("nav-library").click();
  await page.getByRole("button", { name: "打开《已保存作品》" }).click();
  await page.getByRole("button", { name: "作品详情", exact: true }).click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await openLibrary(page);
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/reader-vertical.png" });
  await expect(
    page.getByRole("button", { name: "下载这本", exact: true }),
  ).toHaveCount(0);
  expect(await page.locator("[data-reader-page]").count()).toBeLessThanOrEqual(
    12,
  );
  expect(
    await page.evaluate(
      () =>
        window.readerTest.calls.filter(
          ({ command }) => command === "reader_page",
        ).length,
    ),
  ).toBeLessThanOrEqual(6);
  await jump(page, 5000);
  await expect(
    page.getByRole("img", { name: "第 5000 页", exact: true }),
  ).toBeVisible();
  await page.getByLabel("阅读模式").selectOption("single");
  await expect(page.getByTestId("reader-viewport")).toHaveAttribute(
    "data-mode",
    "single",
  );
  await page
    .getByRole("toolbar")
    .getByRole("button", { name: "返回", exact: true })
    .click();
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  expect(await page.evaluate(() => window.readerTest.saved)).toEqual({
    chapterId: "one",
    pageIndex: 4999,
    offset: 0,
  });
  await openLibrary(page);
  await expect(page.getByTestId("reader-viewport")).toHaveAttribute(
    "data-mode",
    "vertical",
  );
  await expect(page.getByTestId("reader-viewport")).toHaveAttribute(
    "data-zoom",
    "1",
  );
  await expect(page.getByLabel("当前页码")).toHaveText("5000 / 10000");
  await page.getByTestId("reader-viewport").focus();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  const commands = await page.evaluate(() =>
    window.readerTest.calls.map(({ command }) => command),
  );
  expect(commands.filter((command) => command === "reader_close")).toHaveLength(
    2,
  );
});

test("single-page clicks and arrows advance, ordinary wheel only scrolls, zoom dragging never clicks through, toolbar remains focused", async ({
  page,
}) => {
  await openLibrary(page);
  await showToolbar(page);
  await page.getByLabel("阅读模式").selectOption("single");
  const viewport = page.getByTestId("reader-viewport");
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/reader-single.png" });
  await viewport.click({ position: { x: 1100, y: 300 } });
  await expect(page.getByLabel("当前页码")).toHaveText("2 / 10000");
  await viewport.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByLabel("当前页码")).toHaveText("3 / 10000");
  await page.mouse.move(750, 450);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -500);
  await page.mouse.wheel(0, -500);
  await page.mouse.wheel(0, -500);
  await page.keyboard.up("Control");
  await expect
    .poll(async () => Number(await viewport.getAttribute("data-zoom")))
    .toBeGreaterThan(1);
  await page.mouse.move(650, 650);
  await page.mouse.down();
  await page.mouse.move(500, 250, { steps: 8 });
  await page.mouse.up();
  await expect(page.getByLabel("当前页码")).toHaveText("3 / 10000");
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);
  await page.mouse.wheel(0, 300);
  await expect(page.getByLabel("当前页码")).toHaveText("3 / 10000");
  await page.keyboard.press("F11");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.readerTest.calls
            .filter(({ command }) => command === "reader_fullscreen")
            .at(-1)?.args,
      ),
    )
    .toEqual({ fullscreen: true });
  await expect(
    page.getByRole("button", { name: "退出全屏", exact: true }),
  ).toHaveCount(1);
  await showToolbar(page);
  await page.getByLabel("选择章节").focus();
  await page.mouse.move(640, 250);
  await expect(page.getByRole("toolbar")).toHaveCSS("opacity", "1");
  await viewport.focus();
  await page.mouse.move(640, 200);
  await expect(page.getByRole("toolbar")).toHaveCSS("opacity", "0");
  await page.setViewportSize({ width: 760, height: 900 });
  await showToolbar(page);
  await page.screenshot({ path: "visual-evidence/reader-toolbar-narrow.png" });
  await viewport.focus();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("comic-reader")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.readerTest.calls
            .filter(({ command }) => command === "reader_fullscreen")
            .at(-1)?.args,
      ),
    )
    .toEqual({ fullscreen: false });
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
});

test("chapter changes discard delayed images, retry stays page-specific, and chapter end waits for an explicit choice", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.readerTest.hold = true;
  });
  await openLibrary(page);
  await showToolbar(page);
  await expect
    .poll(() => page.evaluate(() => typeof window.readerTest.release))
    .toBe("function");
  await page.getByLabel("选择章节").selectOption("two");
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 3");
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
  const chapterImage = await page
    .getByRole("img", { name: "第 1 页", exact: true })
    .getAttribute("src");
  await page.evaluate(() => window.readerTest.release?.());
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toHaveAttribute("src", chapterImage!);
  await page.evaluate(() => {
    window.readerTest.hold = false;
    window.readerTest.fail = 9999;
  });
  await page.getByLabel("选择章节").selectOption("one");
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 10000");
  await jump(page, 10000);
  await expect(page.getByRole("button", { name: "重试此页" })).toBeVisible();
  await expect(page.getByRole("button", { name: "重试此页" })).toBeInViewport();
  await expect(page.getByLabel("当前页码")).toHaveText("10000 / 10000");
  await page.evaluate(() => {
    window.readerTest.fail = null;
  });
  await page.getByRole("button", { name: "重试此页" }).click();
  await expect(
    page.getByRole("img", { name: "第 10000 页", exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("当前页码")).toHaveText("10000 / 10000");
  const viewport = page.getByTestId("reader-viewport");
  await viewport.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(
    page.getByRole("button", { name: "下一章", exact: true }),
  ).toBeVisible();
  await page.mouse.move(600, 400);
  await page.mouse.wheel(0, 600);
  await expect(page.getByLabel("选择章节")).toHaveValue("one");
  await page.getByRole("button", { name: "下一章", exact: true }).click();
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 3");
  await page.mouse.move(600, 400);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -500);
  await page.keyboard.up("Control");
  await expect
    .poll(async () => Number(await viewport.getAttribute("data-zoom")))
    .toBeGreaterThan(1);
  await page.mouse.wheel(0, 400);
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);
  await showToolbar(page);
  await page
    .getByRole("toolbar")
    .getByRole("button", { name: "返回", exact: true })
    .click();
  expect(await page.evaluate(() => window.readerTest.saved?.chapterId)).toBe(
    "two",
  );
});

test("fifty-thousand-page chapters cross scroll bands while dragging and reach their last page through the real slider", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.readerTest.firstChapterPages = 50000;
    window.readerTest.imageHeight = 1044;
  });
  await openLibrary(page, 50000);
  const viewport = page.getByTestId("reader-viewport");
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
  await page.mouse.move(600, 400);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -500);
  await page.keyboard.up("Control");
  await expect
    .poll(async () => Number(await viewport.getAttribute("data-zoom")))
    .toBeGreaterThan(1);
  await viewport.evaluate((element) => {
    element.scrollTop = element.scrollHeight - element.clientHeight * 3.5;
  });
  await expect
    .poll(() => viewport.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(100000);
  await expect
    .poll(async () =>
      Number((await page.getByLabel("当前页码").innerText()).split(" / ")[0]),
    )
    .toBeGreaterThan(1);
  const before = Number(
    (await page.getByLabel("当前页码").innerText()).split(" / ")[0],
  );
  const oldFirst = await page
    .locator(".reader-pages")
    .getAttribute("data-first-page");
  await page.mouse.move(640, 730);
  await page.mouse.down();
  await page.mouse.move(640, 200, { steps: 12 });
  await expect(page.locator(".reader-pages")).not.toHaveAttribute(
    "data-first-page",
    oldFirst!,
  );
  await page.mouse.move(640, 100, { steps: 5 });
  await page.mouse.up();
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const after = Number(
    (await page.getByLabel("当前页码").innerText()).split(" / ")[0],
  );
  expect(after).toBeGreaterThanOrEqual(before);
  expect(after - before).toBeLessThanOrEqual(2);
  expect(
    await viewport.evaluate((element) => element.scrollHeight),
  ).toBeLessThanOrEqual(1_000_001);
  await showToolbar(page);
  await page.getByRole("slider", { name: "阅读进度" }).focus();
  await page.keyboard.press("End");
  await expect(page.getByLabel("当前页码")).toHaveText("50000 / 50000");
  await expect(
    page.getByRole("img", { name: "第 50000 页", exact: true }),
  ).toBeInViewport();
  await expect(page.getByLabel("当前页码")).toHaveText("50000 / 50000");
  expect(await page.locator("[data-reader-page]").count()).toBeLessThanOrEqual(
    12,
  );
  await page.keyboard.press("Home");
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeInViewport();
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 50000");
});

test("closing during open cancels its token and closes a late obsolete book without reopening the reader", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.readerTest.holdOpen = true;
  });
  await page.getByTestId("nav-library").click();
  await page
    .getByRole("button", { name: "打开《已保存作品》", exact: true })
    .click();
  await page.getByRole("button", { name: "直接阅读", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => typeof window.readerTest.releaseOpen))
    .toBe("function");
  await expect(page.locator(".app-shell")).toHaveAttribute("inert", "");
  await page
    .getByTestId("nav-library")
    .evaluate((element: HTMLElement) => element.focus());
  await expect(page.getByTestId("nav-library")).not.toBeFocused();
  await page
    .getByTestId("comic-reader")
    .getByRole("button", { name: "返回", exact: true })
    .click();
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  const tokens = await page.evaluate(() =>
    window.readerTest.calls
      .filter(
        ({ command }) =>
          command === "reader_open" || command === "reader_cancel_open",
      )
      .map(({ args }) => args.requestId),
  );
  expect(tokens).toHaveLength(2);
  expect(tokens[0]).toBe(tokens[1]);
  await page.evaluate(() => {
    window.readerTest.holdOpen = false;
    window.readerTest.releaseOpen?.();
  });
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.readerTest.calls.filter(
            ({ command }) => command === "reader_close",
          ).length,
      ),
    )
    .toBe(1);
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  await openLibrary(page);
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
});

test("a temporarily unavailable saved online chapter keeps the directory usable so another chapter can be read", async ({
  page,
}) => {
  await page.evaluate(() => {
    window.readerTest.saved = {
      chapterId: "one",
      pageIndex: 50,
      offset: 0.4,
    };
    window.readerTest.failChapter = "one";
  });
  await openOnline(page);
  await expect(page.getByRole("button", { name: "重试章节" })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.readerTest.calls.filter(
        ({ command }) => command === "reader_page",
      ),
    ),
  ).toEqual([]);
  await showToolbar(page);
  await expect(page.getByLabel("选择章节").locator("option")).toHaveCount(2);
  await page.getByLabel("选择章节").selectOption("two");
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 3");
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "重试章节" })).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.readerTest.calls
        .filter(({ command }) => command === "reader_page")
        .every(({ args }) => args.chapterId === "two"),
    ),
  ).toBe(true);
});

test("online read does not download; download button uses the existing confirmation and reader keys leave that dialog alone", async ({
  page,
}) => {
  await openOnline(page);
  await expect(
    page.getByRole("img", { name: "第 1 页", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        ({ command }) =>
          command.startsWith("jm_download_") && command !== "jm_download_read",
      ),
    ),
  ).toEqual([]);
  await showToolbar(page);
  await page.getByRole("button", { name: "下载这本", exact: true }).click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("F11");
  expect(
    await page.evaluate(() =>
      window.readerTest.calls.filter(
        ({ command }) => command === "reader_fullscreen",
      ),
    ),
  ).toEqual([]);
  await expect(page.getByLabel("当前页码")).toHaveText("1 / 10000");
  await page
    .getByTestId("download-confirmation")
    .getByRole("button", { name: "关闭下载确认", exact: true })
    .click();
  await expect(page.getByTestId("comic-reader")).toBeVisible();
  expect(await page.evaluate(() => window.workflowTest.queue.tasks)).toEqual(
    [],
  );
});
