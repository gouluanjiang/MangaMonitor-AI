import { expect, test } from "@playwright/test";
import { installWorkflow } from "./workflow-fixture.ts";

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
  await page.locator(".viewing-history-list button").click();
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
