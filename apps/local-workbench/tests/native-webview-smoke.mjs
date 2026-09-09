// This runs the built Windows application and its real WebView2/IPC bridge.
// It is deliberately restricted to an ephemeral CI runner, never a user's data.
import { chromium, expect } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { readFile, mkdir } from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";

if (process.platform !== "win32" || process.env.CI !== "true")
  throw new Error(
    "Native WebView smoke is restricted to disposable Windows CI.",
  );
const executable = path.resolve(
  "src-tauri/target/x86_64-pc-windows-msvc/release/mangamonitor-workbench-preview.exe",
);
const documents = path.join(
  process.env.APPDATA,
  "com.mangamonitor.workbench.preview",
  "workbench-preview-v1",
);
const output = path.resolve("native-smoke-results");
await mkdir(output, { recursive: true });

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function port() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const value = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return value;
}
async function launch() {
  const debuggingPort = await port();
  const child = spawn(executable, [], {
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
        "--remote-debugging-port=" + debuggingPort,
    },
  });
  let processError = null;
  child.on("error", (error) => {
    processError = error;
  });
  child.stdout.on("data", (data) => process.stdout.write(data));
  child.stderr.on("data", (data) => process.stderr.write(data));
  let browser;
  let page;
  async function stop() {
    if (browser) await browser.close().catch(() => undefined);
    if (child.pid && child.exitCode === null)
      spawnSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        windowsHide: true,
        stdio: "ignore",
      });
    await delay(500);
  }
  try {
    const deadline = Date.now() + 90_000;
    let endpoint;
    while (Date.now() < deadline) {
      if (processError) throw processError;
      if (child.exitCode !== null)
        throw new Error(
          "Native application exited before WebView connected: " +
            child.exitCode,
        );
      try {
        const response = await fetch(
          "http://127.0.0.1:" + debuggingPort + "/json/version",
          { signal: AbortSignal.timeout(1000) },
        );
        const details = await response.json();
        endpoint = details.webSocketDebuggerUrl;
        if (endpoint) break;
      } catch {
        /* The real WebView may still be starting. */
      }
      await delay(250);
    }
    if (!endpoint)
      throw new Error(
        "Native WebView2 did not expose a CDP endpoint within 90 seconds.",
      );
    browser = await chromium.connectOverCDP(endpoint, { timeout: 15_000 });
    while (Date.now() < deadline && !page) {
      page = browser
        .contexts()
        .flatMap((context) => context.pages())
        .find((candidate) => candidate.url().includes("tauri.localhost"));
      if (!page) await delay(200);
    }
    if (!page)
      throw new Error(
        "The bundled workbench page did not open in the native WebView.",
      );
    await expect(page.getByTestId("demo-label")).toContainText("桌面开发版", {
      timeout: 20_000,
    });
    await expect(
      page.getByRole("button", { name: "每行 7 部", exact: true }),
    ).toBeEnabled();
    await expect(page.getByTestId("preferences-error")).toHaveCount(0);
    await expect(page.getByTestId("booklists-error")).toHaveCount(0);
    await expect
      .poll(() =>
        page
          .locator(".cover-button img")
          .first()
          .evaluate((image) => image.complete && image.naturalWidth > 0),
      )
      .toBe(true);
    return { page, stop };
  } catch (error) {
    if (page)
      await page
        .screenshot({ path: path.join(output, "startup-failure.png") })
        .catch(() => undefined);
    await stop();
    throw error;
  }
}

const listName = "原生重启验收 " + process.env.GITHUB_RUN_ID;
let running;
try {
  running = await launch();
  const page = running.page;
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("settings-density-5").click();
  await page.getByTestId("save-settings-page").click();
  await expect(
    page.getByText("外观已保存到本机应用数据。", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("nav-discovery").click();
  await page.getByRole("button", { name: "全部作品", exact: true }).click();
  await page.getByTestId("open-echo").click();
  await page.getByTestId("detail-booklist").click();
  await page.getByTestId("booklist-picker-create").click();
  await page.getByTestId("booklist-picker-name").fill(listName);
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toHaveCount(0);
  const prefs = JSON.parse(
    await readFile(path.join(documents, "preferences.json"), "utf8"),
  );
  const lists = JSON.parse(
    await readFile(path.join(documents, "booklists.json"), "utf8"),
  );
  assert.equal(prefs.schemaVersion, 1);
  assert.ok(prefs.revision > 0);
  assert.equal(prefs.value.appearance.density, 5);
  assert.equal(prefs.value.appearance.backgroundMode, "A");
  assert.equal(
    lists.value.lists.find((list) => list.name === listName).members[0].workId,
    "echo",
  );
  await running.stop();
  running = await launch();
  await expect(running.page.locator(".app-shell")).toHaveAttribute(
    "data-background-mode",
    "A",
  );
  await expect(
    running.page.getByRole("button", { name: "每行 5 部", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await running.page
    .getByRole("button", { name: "本地书单", exact: true })
    .click();
  await running.page
    .getByTestId("booklist-select")
    .selectOption({ label: listName + " · 1 部" });
  await expect(running.page.getByTestId("card-echo")).toBeVisible();
  await running.page.getByTestId("toggle-selection").click();
  await running.page.getByTestId("select-echo").check();
  await expect(running.page.getByTestId("batch-download")).toBeDisabled();
  assert.deepEqual(
    JSON.parse(await readFile(path.join(documents, "booklists.json"), "utf8")),
    lists,
  );
  console.log(
    "NATIVE_WEBVIEW_SMOKE_PASSED: actual Windows WebView, native IPC, disk revision and process restart; unresolved work remains blocked from download.",
  );
} catch (error) {
  if (running?.page)
    await running.page
      .screenshot({ path: path.join(output, "native-webview-failure.png") })
      .catch(() => undefined);
  throw error;
} finally {
  if (running) await running.stop();
}
