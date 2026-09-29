import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { installWorkflow } from "./workflow-fixture.ts";

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const collected: string[] = [];
  errors.set(page, collected);
  page.on("pageerror", (error) => collected.push(error.message));
  await page.route("**/*", (route) => {
    if (new URL(route.request().url()).hostname === "127.0.0.1")
      return route.continue();
    collected.push("Unexpected external request");
    return route.abort();
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  await installWorkflow(page);
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /jm_download_(prepare|confirm)|reader_(open|window_open)|discovery_start|source_(favorite|follow)$|library_scan|delete|promote|replace/.test(
          command,
        ),
      ),
    ),
  ).toEqual([]);
});

async function expectReservedDock(page: Page) {
  const dock = page.getByTestId("browse-selection-dock");
  await expect(dock).toHaveCount(1);
  await expect(dock).toBeInViewport({ ratio: 1 });
  await expect
    .poll(async () =>
      dock.evaluate((element) => {
        const main = document.querySelector("main")!.getBoundingClientRect();
        const status = document
          .querySelector(".statusbar")!
          .getBoundingClientRect();
        const bounds = element.getBoundingClientRect();
        return (
          bounds.top >= main.bottom - 1 &&
          bounds.bottom <= status.top + 1 &&
          bounds.left >= main.left - 1 &&
          bounds.right <= main.right + 1
        );
      }),
    )
    .toBe(true);
  await expect(dock).toHaveCSS("background-color", "rgb(27, 28, 32)");
}

test("selection stays in an opaque reserved band at wide, intermediate and narrow widths without covering the manga canvas", async ({
  page,
}) => {
  await page.getByTestId("nav-favorites").click();
  for (const width of [1280, 700, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.getByRole("main").evaluate((main) => {
      main.scrollTop = main.scrollHeight;
    });
    await expectReservedDock(page);
    const entry = page.getByRole("button", { name: "多选", exact: true });
    await expect(entry).toHaveCSS("background-color", /^rgb\(/);
    await entry.click();
    await page.getByTestId("source-open-JM:102").click();
    const bar = page.getByRole("toolbar", { name: "批量下载操作" });
    await expect(bar).toContainText("已选 1 本");
    await expectReservedDock(page);
    await expect(
      bar.getByRole("button", { name: "下载", exact: true }),
    ).toBeInViewport({ ratio: 1 });
    await mkdir("visual-evidence", { recursive: true });
    await page.screenshot({
      path: `visual-evidence/selection-dock-${width}.png`,
    });
    await bar.getByRole("button", { name: "取消", exact: true }).click();
  }
  await page.getByTestId("nav-completion").click();
  await expectReservedDock(page);
  await page.getByTestId("nav-recent").click();
  await expectReservedDock(page);
  await page.getByTestId("nav-settings").click();
  await expect(page.getByTestId("browse-selection-dock")).toHaveCount(0);
  await page.getByTestId("nav-favorites").click();
  await expectReservedDock(page);
});

test("a long cover-menu title wraps in full and its final characters remain readable inside a small viewport", async ({
  page,
}) => {
  const title =
    "完整漫画标题、作者与版本信息。".repeat(50) +
    "UnbrokenSourceTitle".repeat(10) +
    "标题结尾";
  // Only the shared in-memory synthetic fixture is replaced, never an actual ZIP.
  await page.evaluate((title) => {
    const library = structuredClone(window.workflowTest.library);
    library.items[0].title = title;
    sessionStorage.setItem("synthetic.workflow", JSON.stringify({ library }));
  }, title);
  await page.reload();
  await page.setViewportSize({ width: 390, height: 640 });
  await page.getByTestId("nav-library").click();
  const cover = page.getByRole("button", {
    name: `打开《${title}》`,
    exact: true,
  });
  await cover.evaluate((element) => {
    // A generic contextmenu Event has no mouse coordinates. Use a MouseEvent
    // to exercise the same viewport-edge anchor as a real right click.
    element.dispatchEvent(
      new MouseEvent("contextmenu", {
        clientX: 380,
        clientY: 630,
        button: 2,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  const menu = page.getByRole("menu", { name: "打开漫画" });
  await expect(menu).toBeVisible();
  await expect(menu).toHaveAttribute("style", /left:.*top:/);
  await expect(menu.locator("p").first()).toHaveText(title);
  await expect
    .poll(async () =>
      menu.evaluate((element) => {
        const bounds = element.getBoundingClientRect();
        return {
          leftOverflow: Math.max(0, 7 - bounds.left),
          topOverflow: Math.max(0, 7 - bounds.top),
          rightOverflow: Math.max(0, bounds.right - innerWidth + 7),
          bottomOverflow: Math.max(0, bounds.bottom - innerHeight + 7),
          horizontalOverflow: Math.max(
            0,
            element.scrollWidth - element.clientWidth - 1,
          ),
        };
      }),
    )
    .toEqual({
      leftOverflow: 0,
      topOverflow: 0,
      rightOverflow: 0,
      bottomOverflow: 0,
      horizontalOverflow: 0,
    });
  await expect
    .poll(async () =>
      menu.evaluate((element) => {
        element.scrollTop = element.scrollHeight;
        const text = element.querySelector("p")!.firstChild!;
        const range = document.createRange();
        range.setStart(text, text.textContent!.length - 4);
        range.setEnd(text, text.textContent!.length);
        const tail = range.getBoundingClientRect();
        const bounds = element.getBoundingClientRect();
        return (
          tail.top >= bounds.top &&
          tail.bottom <= bounds.bottom &&
          tail.left >= bounds.left &&
          tail.right <= bounds.right
        );
      }),
    )
    .toBe(true);
  await expect(
    menu.getByRole("menuitem", { name: "小窗阅读", exact: true }),
  ).toBeInViewport({ ratio: 1 });
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/context-menu-long-title.png",
  });
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
});
