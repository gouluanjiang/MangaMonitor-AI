import { expect, test } from "@playwright/test";
import { installWorkflow } from "./workflow-fixture.ts";

test("metadata preparation failure remains diagnosable after navigation and queue refresh without a task", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/*", (route) =>
    new URL(route.request().url()).hostname === "127.0.0.1"
      ? route.continue()
      : route.abort(),
  );
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
    const invoke = bridge.invoke;
    bridge.invoke = (command, args) =>
      command === "jm_download_prepare"
        ? Promise.reject({
            code: "DOWNLOAD_METADATA_TITLE_CONTROL",
            title: "private-title",
            path: "C:/private-path",
          })
        : invoke(command, args);
  });
  await page.getByTestId("nav-completion").click();
  // The initial saved catalog contains 103; 102 exists only after a mock scan.
  // Exercise preparation from the saved record without starting any scan.
  await expect(page.getByTestId("author-update-JM:103")).toBeVisible();
  await page
    .getByTestId("author-update-JM:103")
    .getByRole("button", { name: "下载到漫画库", exact: true })
    .click();
  await expect(page.getByTestId("download-attention-toast")).toContainText(
    "来源作品标题含不支持的控制字符",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-network").click();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(
    /准备下载 · JM.*DOWNLOAD_METADATA_TITLE_CONTROL/,
  );
  const summary = await page.getByTestId("diagnostic-summary").inputValue();
  await page.getByTestId("nav-downloads").click();
  await page.getByRole("button", { name: "重新读取队列", exact: true }).click();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-network").click();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(
    /准备下载 · JM.*DOWNLOAD_METADATA_TITLE_CONTROL/,
  );
  expect(
    summary + (await page.getByTestId("diagnostic-summary").inputValue()),
  ).not.toMatch(/private-title|private-path|synthetic-JM/);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /jm_download_confirm|source_login|delete|promote|replace|library_scan/.test(
          command,
        ),
      ),
    ),
  ).toEqual([]);
  expect(errors).toEqual([]);
});

test("copy receipts survive live summary changes and failed copying preserves selectable text", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/*", (route) =>
    new URL(route.request().url()).hostname === "127.0.0.1"
      ? route.continue()
      : route.abort(),
  );
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
    bridge.invoke = (command, args) =>
      command === "workbench_info"
        ? Promise.resolve({
            version: "1.0.1",
            revision: "a".repeat(40),
            platform: "windows",
          })
        : original(command, args);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async () => {} },
    });
  });
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-network").click();
  await expect(page.getByTestId("diagnostics-version")).toContainText("1.0.1");
  await page.getByRole("button", { name: "复制诊断摘要", exact: true }).click();
  await expect(
    page.getByText("诊断摘要已复制。", { exact: true }),
  ).toBeVisible();
  await page.evaluate(() => {
    window.workflowTest.accounts[0].state = "expired";
    window.workflowTest.accounts[0].errorCode = "SOURCE_SESSION_EXPIRED";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(
    /JM 会话：需要重新登录/,
  );
  await expect(
    page.getByText("诊断摘要已复制。", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("状态已有变化，刚才复制的是点击时的摘要。", { exact: true }),
  ).toBeVisible();

  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("private-clipboard-detail");
        },
      },
    });
  });
  const captured = await page.getByTestId("diagnostic-summary").inputValue();
  await page.getByRole("button", { name: "复制诊断摘要", exact: true }).click();
  await expect(
    page.getByText("无法自动复制，请复制下方已选中的文字。", { exact: true }),
  ).toBeVisible();
  await page.evaluate(() => {
    window.workflowTest.accounts[1].state = "expired";
    window.workflowTest.accounts[1].errorCode = "SOURCE_SESSION_EXPIRED";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "显示最新摘要", exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(captured);
  await page.getByRole("button", { name: "显示最新摘要", exact: true }).click();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(
    /Pica 会话：需要重新登录/,
  );
  expect(await page.getByTestId("diagnostic-summary").inputValue()).not.toMatch(
    /private-|合成账号|C:\\Synthetic|sessionId|account-JM/,
  );
  expect(errors).toEqual([]);
  expect(
    await page.evaluate(() => window.workflowTest.unexpectedCommands),
  ).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(({ command }) =>
        /source_login|source_follow$|source_favorite$|jm_download_confirm|library_scan|delete|promote|replace/.test(
          command,
        ),
      ),
    ),
  ).toEqual([]);
});
