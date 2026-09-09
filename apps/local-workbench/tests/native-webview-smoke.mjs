// This runs the built Windows application and its real WebView2/IPC bridge.
// It is deliberately restricted to an ephemeral CI runner, never a user's data.
import { chromium, expect } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { readFile, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";

if (
  process.platform !== "win32" ||
  process.env.CI !== "true" ||
  process.env.GITHUB_ACTIONS !== "true"
)
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
// Share one isolated WebView profile across the two process launches. The
// application's native documents still use their actual application data path.
const webviewProfile = await mkdtemp(
  path.join(process.env.RUNNER_TEMP, "mangamonitor-webview-"),
);

async function startupDiagnostics(child, debuggingPort, lastConnectionError) {
  // Only inspect this owned CI application and its descendants, never unrelated
  // runner command lines or environment variables that could contain secrets.
  const processes = spawnSync(
    "pwsh.exe",
    [
      "-NoProfile",
      "-File",
      path.resolve("tests/native-startup-diagnostics.ps1"),
      "-AppProcessId",
      String(child.pid ?? 0),
      "-OutputDirectory",
      output,
    ],
    { windowsHide: true, encoding: "utf8", timeout: 30_000 },
  );
  const diagnostic = {
    pid: child.pid,
    exitCode: child.exitCode,
    debuggingPort,
    lastConnectionError,
    processes: processes.stdout?.trim(),
    processInspectionStatus: processes.status,
    processInspectionError: processes.error?.code,
    processInspectionStderr: processes.stderr?.trim(),
  };
  await writeFile(
    path.join(output, "startup-diagnostics.json"),
    JSON.stringify(diagnostic, null, 2),
  );
  console.error("NATIVE_STARTUP_DIAGNOSTICS", diagnostic);
}

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
  const child = spawn(executable, [], {
    // This is the actual GUI under test on a disposable CI desktop. Hiding its
    // first window would change startup behavior and obscure modal failures.
    windowsHide: false,
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
        "--remote-debugging-port=" + debuggingPort,
      WEBVIEW2_USER_DATA_FOLDER: webviewProfile,
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
  let lastConnectionError;
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
      } catch (error) {
        lastConnectionError = error.cause?.code ?? error.name;
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
    await startupDiagnostics(child, debuggingPort, lastConnectionError).catch(
      (diagnosticError) =>
        console.error("Startup inspection failed:", diagnosticError.message),
    );
    if (page)
      await page
        .screenshot({ path: path.join(output, "startup-failure.png") })
        .catch(() => undefined);
    await stop();
    throw error;
  }
}

const listName = "原生重启验收 " + process.env.GITHUB_RUN_ID;
const debuggingPort = await port();
function configureWebview(mode) {
  const result = spawnSync(
    "pwsh.exe",
    [
      "-NoProfile",
      "-File",
      path.resolve("tests/native-webview-test-config.ps1"),
      "-Mode",
      mode,
      "-DebuggingPort",
      String(debuggingPort),
      "-ProfileDirectory",
      webviewProfile,
    ],
    { windowsHide: true, encoding: "utf8", timeout: 30_000 },
  );
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.error || result.status !== 0)
    throw new Error(
      "CI WebView configuration failed: " +
        (result.error?.code ?? result.status),
    );
}
let policyPrepared = false;
let running;
try {
  configureWebview("Configure");
  policyPrepared = true;
  running = await launch();
  const page = running.page;
  await page.getByTestId("nav-settings").click();
  await expect(page.getByTestId("source-account-settings")).toHaveAttribute(
    "aria-busy",
    "false",
  );
  await expect(page.getByTestId("account-JM")).toContainText("未连接");
  await expect(page.getByTestId("account-Pica")).toContainText("未连接");
  // Synthetic input traverses the actual IPC controller. CI forbids live source
  // requests before any network call and never persists this rejected login.
  await page.getByTestId("account-connect-JM").click();
  await page.getByTestId("account-username").fill("offline-ci-fixture");
  await page.getByTestId("account-password").fill("synthetic-password-canary");
  await page.getByTestId("account-login-submit").click();
  await expect(page.getByTestId("account-password")).toHaveValue("");
  await expect(
    page.getByTestId("account-login-dialog").getByRole("alert"),
  ).toBeVisible();
  await page
    .getByTestId("account-login-dialog")
    .getByRole("button", { name: "取消", exact: true })
    .click();
  await expect(page.getByTestId("account-JM")).toContainText("未连接");
  assert.equal(
    await page.evaluate(() =>
      JSON.stringify({ ...localStorage }).includes("synthetic-password-canary"),
    ),
    false,
  );
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("settings-density-5").click();
  await page.getByTestId("save-settings-page").click();
  await expect(
    page.getByText("外观已保存到本机应用数据。", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("nav-library").click();
  await page.getByRole("button", { name: "全部作品", exact: true }).click();
  await page.getByTestId("open-summer").click();
  await page.getByTestId("detail-booklist").click();
  await page.getByTestId("booklist-picker-create").click();
  await page.getByTestId("booklist-picker-name").fill(listName);
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toHaveCount(0);
  // Exercise an unresolved reference through the actual scoped document IPC.
  // It is synthetic, does not request source metadata and belongs to this CI run.
  await page.evaluate(async (name) => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const current = await invoke("read_booklists");
    const list = current.value.lists.find((entry) => entry.name === name);
    list.members.push({ source: "JM", workId: "unresolved-native-ci" });
    list.updatedAt = Date.now();
    await invoke("write_booklists", {
      expectedRevision: current.revision,
      value: current.value,
    });
  }, listName);
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
    "summer",
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
    .selectOption({ label: listName + " · 2 部" });
  await expect(running.page.getByTestId("card-summer")).toBeVisible();
  await expect(running.page.getByTestId("unavailable-members")).toContainText(
    "unresolved-native-ci",
  );
  await expect(
    running.page
      .getByTestId("unavailable-members")
      .getByRole("button", { name: "下载并入库", exact: true }),
  ).toHaveCount(0);
  await running.page.getByTestId("toggle-selection").click();
  await running.page.getByTestId("select-summer").check();
  await expect(running.page.getByTestId("batch-download")).toBeDisabled();
  assert.deepEqual(
    JSON.parse(await readFile(path.join(documents, "booklists.json"), "utf8")),
    lists,
  );
  console.log(
    "NATIVE_WEBVIEW_SMOKE_PASSED: actual Windows WebView, native account IPC rejection/secret clearing, disk revision and process restart; unresolved work remains blocked from download.",
  );
} catch (error) {
  await writeFile(
    path.join(output, "failure.txt"),
    String(error.stack ?? error),
  );
  if (running?.page)
    await running.page
      .screenshot({ path: path.join(output, "native-webview-failure.png") })
      .catch(() => undefined);
  throw error;
} finally {
  try {
    if (running) await running.stop();
  } finally {
    if (policyPrepared) configureWebview("Restore");
  }
}
