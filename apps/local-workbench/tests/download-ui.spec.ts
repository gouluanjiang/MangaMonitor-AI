import { expect, test, type Page } from "@playwright/test";
import type { DownloadSnapshot, DownloadTask } from "../src/download-types.ts";
import type { LibrarySnapshot } from "../src/library-types.ts";
type Options = {
  root?: boolean;
  existing?: boolean;
  phoneOwned?: boolean;
  failRead?: boolean;
};
type Hooks = {
  calls: { command: string; args: Record<string, unknown> }[];
  queue: DownloadSnapshot;
  pc: LibrarySnapshot;
  blockedRead: boolean;
  advance(phase: "error" | "downloaded"): void;
};
declare global {
  interface Window {
    downloadTest: Hooks;
  }
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const list: string[] = [];
  errors.set(page, list);
  page.on("pageerror", (error) => list.push(error.message));
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  const calls = await page.evaluate(() => window.downloadTest?.calls ?? []);
  expect(
    calls.filter((call) =>
      /phone_library_mark|phone_library_unmark|source_set_favorite|delete|promote|remove_file|move_file/.test(
        call.command,
      ),
    ),
  ).toEqual([]);
});
// Synthetic IPC only. No website, credentials, physical library, transfer or real download is used.
async function install(page: Page, options: Options = {}) {
  await page.addInitScript((options: Options) => {
    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
    const rootId = "a".repeat(64),
      entryId = "b".repeat(64),
      sourceId = "123",
      picaId = "0123456789abcdef01234567";
    const entry = {
      id: entryId,
      relativePath: "合成单本作品",
      fileName: "合成单本作品",
      format: "directory" as const,
      title: "合成单本作品",
      authors: ["合成作者"],
      description: null,
      tags: [],
      bytes: 300,
      modifiedAt: 1,
      pageCount: 3,
      coverAvailable: false,
      state: "indexed" as const,
      errorCode: null,
      sourceRef: { source: "JM" as const, workId: sourceId },
      identityEvidence: "metadata" as const,
    };
    const pc: LibrarySnapshot = {
      revision: 1,
      rootId: options.root === false ? null : rootId,
      rootPath: options.root === false ? null : "C:\\Synthetic",
      generation: options.root === false ? 0 : 1,
      phase: options.root === false ? "idle" : "complete",
      freshness: options.root === false ? "none" : "cached",
      items: options.existing ? [entry] : [],
      visited: options.existing ? 1 : 0,
      skipped: 0,
      updatedAt: 1,
      errorCode: null,
    };
    const stored = localStorage.getItem("synthetic.jm.download.queue");
    const hooks: Hooks = (window.downloadTest = {
      calls: [],
      queue: stored
        ? (JSON.parse(stored) as DownloadSnapshot)
        : { revision: 0, tasks: [] },
      pc,
      blockedRead: Boolean(options.failRead),
      advance: () => {},
    });
    const save = () =>
      localStorage.setItem(
        "synthetic.jm.download.queue",
        JSON.stringify(hooks.queue),
      );
    hooks.advance = (phase) => {
      hooks.queue = {
        revision: hooks.queue.revision + 1,
        tasks: hooks.queue.tasks.map((task) => ({
          ...task,
          revision: task.revision + 1,
          phase,
          filesDone: phase === "downloaded" ? 3 : 1,
          filesTotal: 3,
          bytesDone: phase === "downloaded" ? 300 : 100,
          errorCode: phase === "error" ? "SOURCE_TIMEOUT" : null,
          allowedActions: phase === "error" ? ["retry"] : [],
          libraryEntryId: phase === "downloaded" ? entryId : null,
          updatedAt: 2,
        })),
      };
      if (phase === "downloaded")
        hooks.pc = {
          ...hooks.pc,
          revision: hooks.pc.revision + 1,
          items: [entry],
          visited: 1,
        };
      save();
    };
    const preferences = {
      revision: 0,
      value: {
        version: 1,
        appearance: {
          backgroundMode: "B",
          density: 7,
          backgroundImage: null,
          backgroundName: null,
        },
        resources: {
          profile: "balanced",
          simultaneousWorks: 2,
          imageRequests: 4,
        },
      },
    };
    const accounts = ["JM", "Pica"].map((source) => ({
      source,
      sessionId: "session-" + source,
      accountId: "account-" + source,
      displayName: "合成账号",
      state: "connected",
      remembered: false,
      errorCode: null,
    }));
    const sourceWork = (source: string) => ({
      source,
      workId: source === "JM" ? sourceId : picaId,
      title: source === "JM" ? "合成单本作品" : "合成 Pica 作品",
      authors: ["合成作者"],
      description: null,
      tags: [],
      favorite: true,
      chapterCount: 1,
      pageCount: 3,
      coverAvailable: false,
    });
    let restored = false;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
          if (command === "read_preferences") return clone(preferences);
          if (command === "read_booklists")
            return { revision: 0, value: { version: 1, lists: [] } };
          if (command === "source_accounts") return clone(accounts);
          if (command === "library_read") return clone(hooks.pc);
          if (command === "phone_library_read")
            return {
              revision: 0,
              importedNames: options.phoneOwned ? ["合成单本作品.zip"] : [],
              importedAt: options.phoneOwned ? 1 : null,
              importFileName: options.phoneOwned ? "合成手机名单.txt" : null,
              manualEntries: [],
            };
          if (command === "source_following")
            return {
              source: args.source,
              sessionId: args.sessionId,
              revision: 0,
              works: [],
              authors: [],
            };
          if (command === "source_catalog")
            return {
              source: args.source,
              sessionId: args.sessionId,
              snapshot: args.action === "write" ? args.snapshot : null,
              completeSnapshot: args.action === "write" ? args.snapshot : null,
            };
          if (command === "source_query")
            return {
              source: args.source,
              sessionId: args.sessionId,
              items: [sourceWork(String(args.source))],
              page: 1,
              total: 1,
              pages: 1,
              hasMore: false,
              folders: [],
            };
          if (command === "jm_download_read") {
            if (hooks.blockedRead)
              throw {
                code: "DOWNLOAD_INVALID_DOCUMENT",
                message: "synthetic-secret-not-for-ui",
              };
            if (!restored) {
              restored = true;
              hooks.queue = {
                ...hooks.queue,
                tasks: hooks.queue.tasks.map((task) =>
                  ["queued", "downloading", "verifying", "saving"].includes(
                    task.phase,
                  )
                    ? { ...task, phase: "paused", allowedActions: ["resume"] }
                    : task,
                ),
              };
              save();
            }
            return clone(hooks.queue);
          }
          if (command === "jm_download_prepare")
            return {
              planId:
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
              revision: hooks.queue.revision,
              source: "JM",
              workId: sourceId,
              title: "合成单本作品",
              authors: ["合成作者"],
              destinationDisplay: "C:\\Synthetic\\合成单本作品",
              rootId,
              generation: 1,
            };
          if (command === "jm_download_confirm") {
            if (hooks.queue.tasks.length)
              throw { code: "DOWNLOAD_ALREADY_EXISTS" };
            const task: DownloadTask = {
              id: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
              revision: 1,
              source: "JM",
              workId: sourceId,
              title: "合成单本作品",
              phase: "downloading",
              filesDone: 0,
              filesTotal: null,
              bytesDone: 0,
              errorCode: null,
              allowedActions: ["pause"],
              libraryEntryId: null,
              updatedAt: 1,
              destinationDisplay: "C:\\Synthetic\\合成单本作品",
            };
            hooks.queue = { revision: 1, tasks: [task] };
            save();
            return clone(hooks.queue);
          }
          if (command === "jm_download_control") {
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: hooks.queue.tasks.map((task) => ({
                ...task,
                revision: task.revision + 1,
                phase: args.action === "pause" ? "paused" : "downloading",
                errorCode: null,
                allowedActions:
                  args.action === "pause" ? ["resume"] : ["pause"],
              })),
            };
            save();
            return clone(hooks.queue);
          }
          throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
        },
      },
    });
  }, options);
  await page.goto("/");
  await expect(page.getByTestId("library-workbench")).toBeVisible();
}
const calls = (page: Page, name: string) =>
  page.evaluate(
    (name) => window.downloadTest.calls.filter((call) => call.command === name),
    name,
  );
async function prepare(page: Page) {
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
}

test("native queue is empty on first read and never shows or persists demo tasks", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-empty")).toBeVisible();
  await expect(page.getByTestId("queue-page")).toHaveCount(0);
  await expect(page.getByTestId("demo-offline")).toHaveCount(0);
  await expect(page.locator(".statusbar")).not.toContainText("模拟");
  expect(
    await page.evaluate(() =>
      Object.keys(localStorage).filter((key) =>
        key.startsWith("mangamonitor.workbench.demo"),
      ),
    ),
  ).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

test("preparation shows the exact title and destination while cancel creates no task", async ({
  page,
}) => {
  await install(page);
  await prepare(page);
  await expect(page.getByTestId("download-plan-title")).toHaveText(
    "合成单本作品",
  );
  await expect(page.getByTestId("download-plan-destination")).toContainText(
    "C:\\Synthetic\\合成单本作品",
  );
  await expect(page.getByTestId("download-confirmation")).toContainText(
    "电脑副本继续保留",
  );
  expect((await calls(page, "jm_download_prepare"))[0].args.input).toBe("123");
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(page.getByTestId("download-empty")).toBeVisible();
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("confirmed work pauses, survives restart and resumes only after an explicit click", async ({
  page,
}) => {
  await install(page);
  await prepare(page);
  await page.getByTestId("download-confirm").click();
  expect(
    (await calls(page, "jm_download_confirm"))[0].args.expectedRevision,
  ).toBe(0);
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("正在下载");
  await page
    .getByTestId(
      "download-pause-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    )
    .click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("已暂停");
  await page
    .getByTestId(
      "download-resume-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    )
    .click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("正在下载");
  await page.reload();
  await page.getByTestId("nav-queue").click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("已暂停");
  expect(await calls(page, "jm_download_control")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  await page
    .getByTestId(
      "download-resume-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    )
    .click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("正在下载");
  expect(
    (await calls(page, "jm_download_control")).map((call) => call.args.action),
  ).toEqual(["resume"]);
});

test("error retry retains the task and final native registration refreshes PC metadata without changing phone", async ({
  page,
}) => {
  await install(page, { phoneOwned: true });
  await prepare(page);
  await page.getByTestId("download-confirm").click();
  await expect(
    page.getByTestId(
      "download-task-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toBeVisible();
  await page.evaluate(() => window.downloadTest.advance("error"));
  await expect(
    page.getByTestId(
      "download-retry-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toBeVisible();
  await page
    .getByTestId(
      "download-retry-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    )
    .click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("正在下载");
  await page.evaluate(() => window.downloadTest.advance("downloaded"));
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("已下载");
  await page
    .getByTestId(
      "download-open-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    )
    .click();
  await expect(page.getByTestId("library-detail-stock")).toHaveText(
    "已入库 · 手机名单",
  );
  expect(await calls(page, "library_scan")).toEqual([]);
  expect(await calls(page, "phone_library_mark")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toHaveLength(1);
});

test("a missing PC root routes to directory selection without preparation or media work", async ({
  page,
}) => {
  await install(page, { root: false });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("library-empty")).toBeVisible();
  await expect(page.getByTestId("library-choose")).toBeVisible();
  expect(await calls(page, "jm_download_prepare")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("an exact existing PC reference opens its copy instead of creating another download", async ({
  page,
}) => {
  await install(page, { existing: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  expect(await calls(page, "jm_download_prepare")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("JM source detail prepares one plan while Pica download remains unavailable", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-favorites").click();
  await page.getByTestId("source-open-JM:123").click();
  await page.getByTestId("source-download").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await page.getByTestId("download-cancel").click();
  await page.getByTestId("source-detail-back").click();
  await page.getByTestId("source-tab-Pica").click();
  await page.getByTestId("source-open-Pica:0123456789abcdef01234567").click();
  await expect(page.getByTestId("source-download")).toBeDisabled();
  await expect(page.getByTestId("source-download")).toContainText("后续批次");
  expect(await calls(page, "jm_download_prepare")).toHaveLength(1);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("unreadable persisted queue is not treated as empty and raw native messages stay hidden", async ({
  page,
}) => {
  await install(page, { failRead: true });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-error")).toBeVisible();
  await expect(page.getByTestId("download-empty")).toHaveCount(0);
  await expect(page.getByTestId("download-prepare")).toBeDisabled();
  await expect(page.locator("body")).not.toContainText(
    "synthetic-secret-not-for-ui",
  );
  await page.evaluate(() => {
    window.downloadTest.blockedRead = false;
  });
  await page.getByTestId("download-read").click();
  await expect(page.getByTestId("download-empty")).toBeVisible();
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});
