// Actual Windows/WebView2 lifecycle evidence, after the existing native smoke.
// Synthetic missing library IDs exercise window management without media I/O.
import { expect } from "@playwright/test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import path from "node:path";

function windowsForProcess(appPid, closeHandle = null) {
  assert.ok(Number.isSafeInteger(appPid) && appPid > 0);
  assert.ok(closeHandle === null || /^[1-9][0-9]*$/.test(closeHandle));
  // All inspection and optional WM_CLOSE target only this spawned CI PID.
  // PostClose rechecks HWND ownership immediately before posting; it does not
  // add a product IPC permission or bypass the real close-request handler.
  const result = spawnSync(
    "pwsh.exe",
    [
      "-NoProfile",
      "-Command",
      `
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class ReaderSmokeWindow {
  public string handle;
  public string title;
  public bool visible;
  public bool topmost;
}
public static class ReaderSmokeWindows {
  public delegate bool EnumProc(IntPtr handle, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr handle);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr handle, StringBuilder title, int length);
  [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] static extern int GetWindowLong(IntPtr handle, int index);
  [DllImport("user32.dll", EntryPoint = "PostMessageW", SetLastError = true)] static extern bool PostMessage(IntPtr handle, uint message, IntPtr wParam, IntPtr lParam);
  public static void PostClose(uint processId, string windowHandle) {
    var handle = new IntPtr(Int64.Parse(windowHandle));
    uint owner;
    GetWindowThreadProcessId(handle, out owner);
    if (owner != processId) throw new InvalidOperationException("Refused close outside owned CI application.");
    if (!PostMessage(handle, 0x0010, IntPtr.Zero, IntPtr.Zero))
      throw new InvalidOperationException("Could not post owned window close request.");
  }
  public static ReaderSmokeWindow[] Read(uint processId) {
    var found = new List<ReaderSmokeWindow>();
    EnumWindows((handle, unused) => {
      uint owner;
      GetWindowThreadProcessId(handle, out owner);
      if (owner != processId) return true;
      var title = new StringBuilder(256);
      GetWindowText(handle, title, title.Capacity);
      if (title.Length > 0) found.Add(new ReaderSmokeWindow {
        handle = handle.ToInt64().ToString(), title = title.ToString(),
        visible = IsWindowVisible(handle), topmost = (GetWindowLong(handle, -20) & 8) != 0
      });
      return true;
    }, IntPtr.Zero);
    return found.ToArray();
  }
}
'@
${closeHandle === null ? "" : `[ReaderSmokeWindows]::PostClose(${appPid}, '${closeHandle}')`}
ConvertTo-Json -InputObject @([ReaderSmokeWindows]::Read(${appPid})) -Compress
`,
    ],
    { windowsHide: true, encoding: "utf8", timeout: 15_000 },
  );
  if (result.error || result.status !== 0)
    throw new Error(
      "Owned native window inspection failed: " +
        (result.error?.code ?? result.stderr?.trim() ?? result.status),
    );
  return JSON.parse(result.stdout.trim());
}

function invoke(page, command, args = {}) {
  return page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
}

function children(browser) {
  return browser
    .contexts()
    .flatMap((context) => context.pages())
    .filter(
      (page) => !page.isClosed() && page.url().includes("#reader-window"),
    );
}

async function childFor(browser, request, count) {
  await expect
    .poll(() => children(browser).length, { timeout: 20_000 })
    .toBe(count);
  for (const page of children(browser)) {
    // Context is read only after the real frontend has installed its close
    // listeners and reached its controlled failed-open UI.
    await expect(page.getByTestId("comic-reader")).toBeVisible({
      timeout: 20_000,
    });
    await expect(
      page.getByRole("button", { name: "重试打开", exact: true }),
    ).toBeVisible();
    const context = await invoke(page, "reader_window_context");
    if (context.request.entryId === request.entryId) {
      assert.deepEqual(context.request, request);
      await expect(page.getByTestId("nav-library")).toHaveCount(0);
      await expect(page.getByTestId("comic-reader").locator("img")).toHaveCount(
        0,
      );
      return page;
    }
  }
  throw new Error("Expected synthetic native reader window was not found.");
}

async function closeFromToolbar(page) {
  try {
    // This uses the real child UI close callback, including pending-open
    // cancellation, before the native reader_window_close command.
    await page.getByRole("button", { name: "返回", exact: true }).click();
  } catch (error) {
    // Destruction may race Playwright's post-click acknowledgement. Only an
    // actually destroyed WebView is accepted; all other failures still fail.
    if (!page.isClosed()) throw error;
  }
  await expect.poll(() => page.isClosed(), { timeout: 15_000 }).toBe(true);
}

export async function verifyReaderWindows({
  page: main,
  browser,
  child,
  output,
}) {
  if (
    process.platform !== "win32" ||
    process.env.CI !== "true" ||
    process.env.GITHUB_ACTIONS !== "true"
  )
    throw new Error(
      "Native reader window smoke is restricted to disposable Windows CI.",
    );
  const receipt = {
    kind: "actual-windows-webview2-synthetic-library-window-lifecycle",
    physicalCloseButtonTested: false,
    closePath:
      "owned HWND WM_CLOSE -> native CloseRequested -> frontend close handshake; reader_main_close IPC; final child toolbar -> reader_window_close IPC",
    mediaRead: false,
    liveSourceRequest: false,
    steps: [],
    windowInspections: [],
  };
  const record = (step, extra = {}) => {
    receipt.steps.push({ step, ...extra });
    console.log("NATIVE_READER_WINDOW_STEP", step);
  };
  const observedHandles = new Set();
  const snapshot = () => {
    const windows = windowsForProcess(child.pid);
    for (const window of windows) observedHandles.add(window.handle);
    if (receipt.windowInspections.length < 40)
      receipt.windowInspections.push({ windows });
    return windows;
  };
  const postClose = (handle) => {
    assert.equal(child.exitCode, null);
    assert.ok(
      observedHandles.has(handle),
      "WM_CLOSE requires an already observed owned HWND.",
    );
    assert.ok(
      snapshot().some((window) => window.handle === handle),
      "WM_CLOSE target must still belong to the running CI application.",
    );
    windowsForProcess(child.pid, handle);
    record("owned-native-close-request-posted", {
      handle,
      message: "WM_CLOSE",
    });
  };
  const windowState = async (handle, expected) => {
    await expect
      .poll(
        () => {
          const current = snapshot().find((window) => window.handle === handle);
          return (
            current &&
            Object.fromEntries(
              Object.keys(expected).map((key) => [key, current[key]]),
            )
          );
        },
        { timeout: 15_000 },
      )
      .toEqual(expected);
  };
  try {
    const initial = snapshot();
    const visible = initial.filter((window) => window.visible);
    assert.equal(
      visible.length,
      1,
      "The isolated CI application must start with one visible main window.",
    );
    const mainHandle = visible[0].handle;
    const requestA = {
      kind: "library",
      rootId: "a".repeat(64),
      generation: 1,
      entryId: "b".repeat(64),
    };
    const requestB = { ...requestA, entryId: "c".repeat(64) };
    const openedA = await invoke(main, "reader_window_open", {
      request: requestA,
    });
    assert.equal(typeof openedA.label, "string");
    const a = await childFor(browser, requestA, 1);
    const afterA = snapshot();
    const newA = afterA.filter(
      (window) =>
        window.visible && !initial.some((old) => old.handle === window.handle),
    );
    assert.equal(newA.length, 1, "Opening A must create one actual OS window.");
    const aHandle = newA[0].handle;
    assert.equal(newA[0].topmost, false);
    const openedB = await invoke(main, "reader_window_open", {
      request: requestB,
    });
    assert.notEqual(openedB.label, openedA.label);
    const b = await childFor(browser, requestB, 2);
    const afterB = snapshot();
    const newB = afterB.filter(
      (window) =>
        window.visible && !afterA.some((old) => old.handle === window.handle),
    );
    assert.equal(
      newB.length,
      1,
      "Opening B must create a separate actual OS window.",
    );
    const bHandle = newB[0].handle;
    assert.equal(newB[0].topmost, false);
    assert.equal(afterB.filter((window) => window.visible).length, 3);
    record("two-independent-native-windows", {
      mainHandle,
      aHandle,
      bHandle,
      labels: [openedA.label, openedB.label],
    });

    assert.deepEqual(
      await invoke(main, "reader_window_open", { request: requestA }),
      openedA,
    );
    assert.equal(children(browser).length, 2);
    assert.equal(snapshot().filter((window) => window.visible).length, 3);
    await main.getByTestId("nav-queue").click();
    await expect(main.getByTestId("download-empty")).toBeVisible();
    await main.getByTestId("nav-library").click();
    await expect(main.getByTestId("library-empty")).toBeVisible();
    record("main-still-operable-and-same-book-reuses-window");

    await invoke(a, "reader_window_pin", { pinned: true });
    await windowState(aHandle, { visible: true, topmost: true });
    await windowState(bHandle, { visible: true, topmost: false });
    await invoke(a, "reader_window_pin", { pinned: false });
    await windowState(aHandle, { visible: true, topmost: false });
    record("actual-os-pin-isolated-and-reversible");
    await a.screenshot({
      path: path.join(output, "reader-window-native-a.png"),
    });
    await b.screenshot({
      path: path.join(output, "reader-window-native-b.png"),
    });

    // WM_CLOSE follows the same native CloseRequested dispatch as system X,
    // without claiming that this test physically clicked a title-bar button.
    postClose(mainHandle);
    await windowState(mainHandle, { visible: false });
    assert.equal(child.exitCode, null);
    assert.deepEqual(await invoke(b, "reader_window_context"), {
      request: requestB,
    });
    await windowState(aHandle, { visible: true });
    await windowState(bHandle, { visible: true });
    record("main-hidden-children-remain-live");
    await b.getByRole("button", { name: "显示主界面", exact: true }).click();
    await windowState(mainHandle, { visible: true });
    await main.getByTestId("nav-settings").click();
    await expect(main.getByTestId("source-account-settings")).toBeVisible();
    assert.deepEqual(await invoke(main, "jm_download_read"), {
      revision: 0,
      tasks: [],
    });
    record("child-restores-operable-main-with-empty-download-queue");

    await invoke(main, "reader_main_close");
    await windowState(mainHandle, { visible: false });
    postClose(aHandle);
    await expect.poll(() => a.isClosed(), { timeout: 15_000 }).toBe(true);
    assert.equal(child.exitCode, null);
    await expect
      .poll(() => snapshot().some((window) => window.handle === aHandle), {
        timeout: 15_000,
      })
      .toBe(false);
    assert.deepEqual(await invoke(b, "reader_window_context"), {
      request: requestB,
    });
    await windowState(bHandle, { visible: true });
    record("first-child-close-preserves-other-window-and-process");
    await closeFromToolbar(b);
    await expect.poll(() => child.exitCode, { timeout: 15_000 }).toBe(0);
    record("last-child-close-exits-native-application", {
      exitCode: child.exitCode,
    });
    receipt.passed = true;
  } catch (error) {
    receipt.passed = false;
    receipt.error = String(error.stack ?? error);
    for (const [index, page] of children(browser).slice(0, 2).entries())
      await page
        .screenshot({
          path: path.join(
            output,
            "reader-window-native-failure-" + index + ".png",
          ),
          timeout: 5_000,
        })
        .catch(() => undefined);
    throw error;
  } finally {
    await writeFile(
      path.join(output, "reader-window-native-lifecycle.json"),
      JSON.stringify(receipt, null, 2),
    );
  }
}
