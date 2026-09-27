import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import {
  installReaderWindows,
  type ReaderWindowHarness,
} from "./reader-window-fixture.ts";

const harnesses = new WeakMap<Page, ReaderWindowHarness>();
test.beforeEach(async ({ context, page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  harnesses.set(page, await installReaderWindows(context, page));
});
test.afterEach(async ({ page }) => {
  const harness = harnesses.get(page)!;
  expect(harness.errors).toEqual([]);
  expect(harness.violations).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(await page.evaluate(() => window.workflowTest.queue.tasks)).toEqual(
    [],
  );
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /confirm|source_(follow|favorite)$|delete|promote|replace/.test(
          command,
        ),
      ),
    ),
  ).toEqual([]);
});

async function localCover(main: Page) {
  await main.getByTestId("nav-library").click();
  await main
    .getByRole("button", { name: "打开《已保存作品》", exact: true })
    .click();
  await expect(main.getByTestId("reader-cover-actions")).toBeVisible();
}
async function openLocalWindow(main: Page) {
  await localCover(main);
  await main.getByRole("button", { name: "手机小框阅读", exact: true }).click();
  await expect(main.getByTestId("reader-cover-actions")).toHaveCount(0);
}
async function openOnlineWindow(main: Page, workId = "102") {
  await main.getByTestId("nav-favorites").click();
  await main
    .getByTestId(`source-card-JM:${workId}`)
    .getByRole("button", { name: /打开/ })
    .click();
  await main.getByRole("button", { name: "手机小框阅读", exact: true }).click();
  await expect(main.getByTestId("reader-cover-actions")).toHaveCount(0);
}
async function toolbar(page: Page) {
  const size = page.viewportSize()!;
  await page.mouse.move(size.width / 2, size.height - 3);
  await expect(page.getByRole("toolbar", { name: "阅读工具" })).toHaveCSS(
    "opacity",
    "1",
  );
}
async function jump(page: Page, number: number, count: number) {
  await toolbar(page);
  const slider = page.getByRole("slider", { name: "阅读进度" });
  await slider.focus();
  await page.keyboard.press("Home");
  for (let i = 1; i < number; i++) await page.keyboard.press("ArrowRight");
  await expect(page.getByLabel("当前页码")).toHaveText(`${number} / ${count}`);
  await expect(
    page.getByRole("img", { name: `第 ${number} 页`, exact: true }),
  ).toBeVisible();
}
function expectPageStart(
  position: unknown,
  chapterId: string,
  pageIndex: number,
) {
  expect(position).toEqual({
    chapterId,
    pageIndex,
    offset: expect.any(Number),
  });
  // Projecting fractional CSS geometry into a page ratio can leave a tiny
  // nonzero remainder; chapter/page identity and all object keys stay exact.
  const offset = (position as { offset: number }).offset;
  expect(offset).toBeGreaterThanOrEqual(0);
  expect(offset).toBeCloseTo(0, 10);
}

test("three cover choices preserve details and cancellation; independent small readers leave the main application usable", async ({
  page,
}) => {
  const harness = harnesses.get(page)!;
  await localCover(page);
  const menu = page.getByTestId("reader-cover-actions");
  for (const name of ["漫画详细", "程序内阅读", "手机小框阅读"])
    await expect(menu.getByRole("button", { name, exact: true })).toBeVisible();
  await menu.getByRole("button", { name: "取消", exact: true }).click();
  expect(
    harness.calls.filter(({ command }) =>
      /reader_(open|window_open)$/.test(command),
    ),
  ).toEqual([]);
  await localCover(page);
  await menu.getByRole("button", { name: "漫画详细", exact: true }).click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await openLocalWindow(page);
  const first = await harness.child(1);
  await expect(page.locator(".app-shell")).not.toHaveAttribute("inert", "");
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  await page.getByTestId("nav-author-search").click();
  await page.getByRole("textbox", { name: "搜索作者名" }).fill("合成新作者");
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toHaveValue(
    "合成新作者",
  );
  await openOnlineWindow(page);
  const second = await harness.child(2);
  for (const child of [first, second]) {
    await expect(child.locator(".app-shell")).toHaveCount(0);
    await expect(child.getByTestId("reader-viewport")).toHaveAttribute(
      "data-mode",
      "vertical",
    );
    await expect(child.getByTestId("reader-viewport")).toHaveAttribute(
      "data-zoom",
      "1",
    );
    await toolbar(child);
    await expect(
      child.getByRole("button", { name: "阅读窗口置顶", exact: true }),
    ).toHaveAttribute("aria-pressed", "false");
  }
  const firstImage = await first
    .getByRole("img", { name: "第 1 页", exact: true })
    .getAttribute("src");
  expect(
    await second
      .getByRole("img", { name: "第 1 页", exact: true })
      .getAttribute("src"),
  ).not.toBe(firstImage);
  await expect(
    first.getByRole("button", { name: "下载这本", exact: true }),
  ).toHaveCount(0);
  await expect(
    second.getByRole("button", { name: "下载这本", exact: true }),
  ).toBeVisible();
  // Reopening an already-open exact book focuses its existing native target;
  // the host must neither open an in-app reader nor construct another child.
  await openLocalWindow(page);
  expect(harness.pages.size).toBe(3);
  expect(
    harness.calls.filter(
      ({ label, command }) => label === "main" && command === "reader_open",
    ),
  ).toEqual([]);
  await page.getByTestId("nav-author-search").click();
  await expect(page.getByRole("textbox", { name: "搜索作者名" })).toHaveValue(
    "合成新作者",
  );
  await expect(first.getByLabel("当前页码")).toHaveText("1 / 6");
  await expect(second.getByLabel("当前页码")).toHaveText("1 / 6");
  await mkdir("visual-evidence", { recursive: true });
  await first.screenshot({
    path: "visual-evidence/reader-window-vertical.png",
  });
});

test("pin failure remains visibly off, windows keep independent positions and native close flushes before closing only its reader", async ({
  page,
}) => {
  const harness = harnesses.get(page)!;
  await openLocalWindow(page);
  const first = await harness.child(1);
  await openOnlineWindow(page);
  const second = await harness.child(2);
  await toolbar(first);
  const firstPin = first.getByRole("button", {
    name: "阅读窗口置顶",
    exact: true,
  });
  harness.failPin.add("reader-window-1");
  await firstPin.click();
  await expect(firstPin).toHaveAttribute("aria-pressed", "false");
  await expect(
    first.getByText("暂时无法更改置顶状态，请重试。", { exact: true }),
  ).toBeVisible();
  expect(harness.pinned.get("reader-window-1")).toBe(false);
  harness.failPin.delete("reader-window-1");
  await firstPin.click();
  await expect(firstPin).toHaveAttribute("aria-pressed", "true");
  await toolbar(second);
  await expect(
    second.getByRole("button", { name: "阅读窗口置顶", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  harness.holdSave.add("reader-window-1");
  const firstCloseStart = harness.calls.length;
  await jump(first, 4, 6);
  await second.getByLabel("选择章节").selectOption("two");
  await jump(second, 2, 3);
  await second.getByLabel("阅读模式").selectOption("single");
  await expect(second.getByTestId("reader-viewport")).toHaveAttribute(
    "data-mode",
    "single",
  );
  await expect(first.getByLabel("当前页码")).toHaveText("4 / 6");
  await expect(first.getByLabel("选择章节")).toHaveValue("one");
  await expect(firstPin).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(() => harness.pendingSave.has("reader-window-1"))
    .toBe(true);
  await harness.emit("reader-window-1", "reader-window-close-requested");
  await first.evaluate(
    () =>
      new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
  );
  expect(harness.closed.has("reader-window-1")).toBe(false);
  expect(
    harness.calls
      .slice(firstCloseStart)
      .filter(
        ({ label, command }) =>
          label === "reader-window-1" && command === "reader_close",
      ),
  ).toEqual([]);
  harness.releaseSave("reader-window-1");
  await expect.poll(() => harness.closed.has("reader-window-1")).toBe(true);
  const firstClose = harness.calls
    .slice(firstCloseStart)
    .filter(({ label }) => label === "reader-window-1");
  const firstSave = firstClose.findIndex(
    ({ command }) => command === "reader_save_position",
  );
  const firstReaderClose = firstClose.findIndex(
    ({ command }) => command === "reader_close",
  );
  const firstWindowClose = firstClose.findIndex(
    ({ command }) => command === "reader_window_close",
  );
  expect(firstSave).toBeGreaterThanOrEqual(0);
  expectPageStart(firstClose[firstSave].args.position, "one", 3);
  expect(firstReaderClose).toBeGreaterThan(firstSave);
  expect(firstWindowClose).toBeGreaterThan(firstReaderClose);
  await expect(second.getByLabel("当前页码")).toHaveText("2 / 3");
  expect(harness.closed.has("reader-window-2")).toBe(false);
  await first.close();
  await openLocalWindow(page);
  const reopened = await harness.child(3, "4 / 6");
  await expect(reopened.getByTestId("reader-viewport")).toHaveAttribute(
    "data-mode",
    "vertical",
  );
  await toolbar(reopened);
  await expect(
    reopened.getByRole("button", { name: "阅读窗口置顶", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  await harness.emit("reader-window-2", "reader-window-close-requested");
  await expect.poll(() => harness.closed.has("reader-window-2")).toBe(true);
  expectPageStart(harness.saved.get("JM:102"), "two", 1);
  expectPageStart(
    harness.saved.get(harness.key(harness.requests.get("reader-window-3")!)),
    "one",
    3,
  );
});

test("small-window controls remain usable; a main-close request does not end readers and download handoff only opens main confirmation", async ({
  page,
}) => {
  const harness = harnesses.get(page)!;
  await openOnlineWindow(page);
  const child = await harness.child(1);
  await child.setViewportSize({ width: 320, height: 640 });
  await toolbar(child);
  for (const label of ["阅读模式", "选择章节", "阅读进度"])
    await expect(child.getByLabel(label, { exact: true })).toBeInViewport();
  for (const name of ["阅读窗口置顶", "显示主界面", "下载这本"])
    await expect(
      child.getByRole("button", { name, exact: true }),
    ).toBeInViewport();
  const viewport = child.getByTestId("reader-viewport");
  const vertical = await child.locator("[data-reader-page='1']").boundingBox();
  const bounds = await viewport.boundingBox();
  expect(vertical).not.toBeNull();
  expect(bounds).not.toBeNull();
  expect(vertical!.width).toBeLessThanOrEqual(bounds!.width + 1);
  expect(vertical!.width).toBeGreaterThan(bounds!.width - 25);
  await child.getByLabel("阅读模式").selectOption("single");
  await expect(viewport).toHaveAttribute("data-mode", "single");
  const single = await child.locator("[data-reader-page='1']").boundingBox();
  expect(single!.width).toBeLessThanOrEqual(bounds!.width + 1);
  expect(single!.height).toBeLessThanOrEqual(bounds!.height + 1);
  await mkdir("visual-evidence", { recursive: true });
  await child.screenshot({
    path: "visual-evidence/reader-window-controls-narrow.png",
  });
  await harness.emit("main", "reader-main-close-requested");
  await expect.poll(() => harness.mainHidden).toBe(true);
  expect(harness.appExited).toBe(false);
  await viewport.focus();
  await child.keyboard.press("ArrowRight");
  await expect(child.getByLabel("当前页码")).toHaveText("2 / 6");
  await toolbar(child);
  await child.getByRole("button", { name: "显示主界面", exact: true }).click();
  await expect.poll(() => harness.mainHidden).toBe(false);
  await expect(child.getByTestId("comic-reader")).toBeVisible();
  await harness.emit("main", "reader-main-close-requested");
  await expect.poll(() => harness.mainHidden).toBe(true);
  await child.getByRole("button", { name: "下载这本", exact: true }).click();
  await expect.poll(() => harness.mainHidden).toBe(false);
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-confirmation")).toContainText(
    "本次选择的 JM 作品",
  );
  await expect(page.getByTestId("download-plan-destination")).toContainText(
    ".zip",
  );
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        ({ command }) => command === "jm_download_prepare",
      ),
    ),
  ).toHaveLength(1);
  await child.getByRole("button", { name: "下载这本", exact: true }).click();
  await expect(
    page.getByText("请先处理主界面中已有的下载确认，再准备其他作品。", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        ({ command }) => command === "jm_download_prepare",
      ),
    ),
  ).toHaveLength(1);
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(child.getByLabel("当前页码")).toHaveText("2 / 6");
  await harness.emit("main", "reader-main-close-requested");
  await expect.poll(() => harness.mainHidden).toBe(true);
  await harness.emit("reader-window-1", "reader-window-close-requested");
  await expect.poll(() => harness.closed.has("reader-window-1")).toBe(true);
  expect(harness.appExited).toBe(true);
  // This flag is only the synthetic native protocol result. Actual last-window
  // process exit is covered separately by the Windows WebView/IPC smoke.
});

test("simultaneous reader download handoffs keep one preparation while the queue recheck is pending", async ({
  page,
}) => {
  const harness = harnesses.get(page)!;
  await openOnlineWindow(page);
  const first = await harness.child(1);
  await openOnlineWindow(page, "103");
  const second = await harness.child(2);
  await toolbar(first);
  await toolbar(second);
  await page.evaluate(() => {
    window.readerWindowHarness.holdQueueRead = true;
  });
  await first.getByRole("button", { name: "下载这本", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() => window.readerWindowHarness.waitingQueueRead),
    )
    .toBe(true);
  await second.getByRole("button", { name: "下载这本", exact: true }).click();
  await expect(
    page.getByText("其他下载正在准备，请稍后再试。", { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        ({ command }) => command === "jm_download_prepare",
      ),
    ),
  ).toEqual([]);
  await page.evaluate(() => window.readerWindowHarness.releaseQueueRead());
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-confirmation")).toContainText(
    "本次选择的 JM 作品",
  );
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.workflowTest.calls
          .filter(({ command }) => command === "jm_download_prepare")
          .map(({ args }) => args.input),
      ),
    )
    .toEqual(["102"]);
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(first.getByLabel("当前页码")).toHaveText("1 / 6");
  await expect(second.getByLabel("当前页码")).toHaveText("1 / 6");
});
