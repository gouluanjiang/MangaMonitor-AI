import { expect, test, type Page } from "@playwright/test";
import type {
  DownloadLocalFiles,
  DownloadSnapshot,
  DownloadTask,
  DownloadPlan,
  DownloadSource,
  DownloadBatchPlan,
} from "../src/download-types.ts";
import type { LibrarySnapshot } from "../src/library-types.ts";
import type { AccountSummary } from "../src/source-types.ts";
type Options = {
  root?: boolean;
  existing?: boolean;
  phoneOwned?: boolean;
  failRead?: boolean;
  completed?: boolean;
  fixtureSource?: DownloadSource;
  wrongPlanSource?: boolean;
  mixedQueue?: boolean;
  batchWorks?: boolean;
};
type Hooks = {
  calls: { command: string; args: Record<string, unknown> }[];
  queue: DownloadSnapshot;
  pc: LibrarySnapshot;
  blockedRead: boolean;
  filePresence: DownloadLocalFiles;
  accounts: AccountSummary[];
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
    const fixtureSource = options.fixtureSource ?? "JM";
    const fixtureTitle =
      fixtureSource === "JM" ? "合成单本作品" : "合成 Pica 作品";
    const fixtureId = fixtureSource === "JM" ? sourceId : picaId;
    const entry = {
      id: entryId,
      relativePath: fixtureTitle,
      fileName: fixtureTitle,
      format: "directory" as const,
      title: fixtureTitle,
      authors: ["合成作者"],
      description: null,
      tags: [],
      bytes: 300,
      modifiedAt: 1,
      pageCount: 3,
      coverAvailable: false,
      state: "indexed" as const,
      errorCode: null,
      sourceRef: { source: fixtureSource, workId: fixtureId },
      identityEvidence: "metadata" as const,
    };
    const pc: LibrarySnapshot = {
      revision: 1,
      rootId: options.root === false ? null : rootId,
      rootPath: options.root === false ? null : "C:\\Synthetic",
      generation: options.root === false ? 0 : 1,
      phase: options.root === false ? "idle" : "complete",
      freshness: options.root === false ? "none" : "cached",
      items: options.existing || options.completed ? [entry] : [],
      visited: options.existing || options.completed ? 1 : 0,
      skipped: 0,
      updatedAt: 1,
      errorCode: null,
    };
    const stored = localStorage.getItem("synthetic.jm.download.queue");
    const hooks: Hooks = (window.downloadTest = {
      calls: [],
      queue: stored
        ? (JSON.parse(stored) as DownloadSnapshot)
        : options.completed
          ? {
              revision: 1,
              tasks: [
                {
                  id: "c".repeat(64),
                  revision: 1,
                  source: fixtureSource,
                  workId: fixtureId,
                  title: fixtureTitle,
                  phase: "downloaded",
                  filesDone: 3,
                  filesTotal: 3,
                  bytesDone: 300,
                  errorCode: null,
                  allowedActions: [],
                  libraryEntryId: entryId,
                  localFiles: "present",
                  updatedAt: 1,
                  destinationDisplay: "C:\\Synthetic\\" + fixtureTitle,
                },
              ],
            }
          : { revision: 0, tasks: [] },
      pc,
      blockedRead: Boolean(options.failRead),
      filePresence: "present",
      accounts: (["JM", "Pica"] as const).map((source) => ({
        source,
        sessionId: "session-" + source,
        accountId: "account-" + source,
        displayName: "合成账号",
        state: "connected",
        remembered: false,
        errorCode: null,
      })),
      advance: () => {},
    });
    if (!stored && options.mixedQueue)
      hooks.queue = {
        revision: 1,
        tasks: (["JM", "Pica"] as const).map((source, index) => ({
          id: (index === 0 ? "c" : "d").repeat(64),
          revision: 1,
          source,
          workId: source === "JM" ? sourceId : picaId,
          title: source === "JM" ? "合成单本作品" : "合成 Pica 作品",
          phase: index === 0 ? "paused" : "error",
          filesDone: 1,
          filesTotal: 3,
          bytesDone: 100,
          errorCode: index === 0 ? null : "SOURCE_TIMEOUT",
          allowedActions: index === 0 ? ["resume"] : ["retry"],
          libraryEntryId: null,
          localFiles: null,
          updatedAt: 1,
          destinationDisplay: "C:\\Synthetic\\" + source,
        })),
      };
    const save = () =>
      localStorage.setItem(
        "synthetic.jm.download.queue",
        JSON.stringify(hooks.queue),
      );
    hooks.advance = (phase) => {
      hooks.queue = {
        revision: hooks.queue.revision + 1,
        tasks: hooks.queue.tasks.map((task) =>
          task.phase === "downloaded"
            ? task
            : {
                ...task,
                revision: task.revision + 1,
                phase,
                filesDone: phase === "downloaded" ? 3 : 1,
                filesTotal: 3,
                bytesDone: phase === "downloaded" ? 300 : 100,
                errorCode: phase === "error" ? "SOURCE_TIMEOUT" : null,
                allowedActions: phase === "error" ? ["retry"] : [],
                libraryEntryId: phase === "downloaded" ? entryId : null,
                localFiles: phase === "downloaded" ? "present" : null,
                updatedAt: 2,
              },
        ),
      };
      if (phase === "downloaded")
        hooks.pc = {
          ...hooks.pc,
          revision: hooks.pc.revision + 1,
          items: hooks.queue.tasks
            .filter((task) => task.phase === "downloaded")
            .map((task) => ({
              ...entry,
              title: task.title,
              fileName: task.title,
              relativePath: task.title,
              sourceRef: { source: task.source, workId: task.workId },
            })),
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
    let preparedPlan: DownloadPlan | null = null;
    let preparedBatch: DownloadBatchPlan | null = null;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
          if (command === "read_preferences") return clone(preferences);
          if (command === "read_booklists")
            return { revision: 0, value: { version: 1, lists: [] } };
          if (command === "source_accounts") return clone(hooks.accounts);
          if (command === "library_read") return clone(hooks.pc);
          if (command === "source_matches_read")
            return { revision: 0, pairs: [] };
          if (command === "phone_library_read")
            return {
              revision: 0,
              importedNames: options.phoneOwned ? [fixtureTitle + ".zip"] : [],
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
              items:
                options.batchWorks && args.kind !== "detail"
                  ? [
                      sourceWork(String(args.source)),
                      {
                        ...sourceWork(String(args.source)),
                        workId: "124",
                        title: "合成批量作品 124",
                      },
                    ]
                  : [sourceWork(String(args.source))],
              page: 1,
              total: options.batchWorks && args.kind !== "detail" ? 2 : 1,
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
            if (args.recheckFiles !== false) {
              hooks.queue = {
                ...hooks.queue,
                tasks: hooks.queue.tasks.map((task) =>
                  task.phase === "downloaded"
                    ? { ...task, localFiles: hooks.filePresence }
                    : task,
                ),
              };
            }
            return clone(hooks.queue);
          }
          if (command === "jm_download_prepare") {
            const requestedSource = (args.scope as { source: DownloadSource })
              .source;
            const source = options.wrongPlanSource
              ? requestedSource === "JM"
                ? "Pica"
                : "JM"
              : requestedSource;
            const title = source === "JM" ? "合成单本作品" : "合成 Pica 作品";
            preparedPlan = {
              planId: (hooks.queue.tasks.length ? "d" : "c").repeat(64),
              revision: hooks.queue.revision,
              source,
              workId: source === "JM" ? sourceId : picaId,
              title,
              authors: ["合成作者"],
              destinationDisplay: "C:\\Synthetic\\" + title,
              rootId,
              generation: 1,
            };
            return clone(preparedPlan);
          }
          if (command === "jm_download_batch_prepare") {
            const seen = new Set<string>();
            const plans: DownloadPlan[] = [],
              issues: DownloadBatchPlan["issues"] = [];
            for (const input of args.inputs as string[]) {
              const id = input.replace(/^JM/i, "");
              if (seen.has(id)) {
                issues.push({ input, errorCode: "DOWNLOAD_BATCH_DUPLICATE" });
                continue;
              }
              seen.add(id);
              plans.push({
                planId: Number(id).toString(16).padStart(64, "0"),
                revision: hooks.queue.revision,
                source: "JM",
                workId: id,
                title: "合成批量作品 " + id,
                authors: ["合成作者"],
                destinationDisplay: "C:\\Synthetic\\" + id,
                rootId,
                generation: 1,
              });
            }
            preparedBatch = { batchId: "e".repeat(64), plans, issues };
            return clone(preparedBatch);
          }
          if (command === "jm_download_batch_confirm") {
            if (!preparedBatch || preparedBatch.batchId !== args.batchId)
              throw { code: "DOWNLOAD_PLAN_STALE" };
            const tasks: DownloadTask[] = preparedBatch.plans.map(
              (plan, index) => ({
                id: plan.planId,
                revision: 1,
                source: plan.source,
                workId: plan.workId,
                title: plan.title,
                phase: index === 0 ? "downloading" : "queued",
                filesDone: 0,
                filesTotal: null,
                bytesDone: 0,
                errorCode: null,
                allowedActions: ["pause"],
                libraryEntryId: null,
                localFiles: null,
                updatedAt: 1,
                destinationDisplay: plan.destinationDisplay,
              }),
            );
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: [...hooks.queue.tasks, ...tasks],
            };
            save();
            return clone(hooks.queue);
          }
          if (
            command === "jm_download_pause_all" ||
            command === "jm_download_resume_many"
          ) {
            const ids = new Set(
              ((args.tasks ?? []) as { taskId: string }[]).map(
                (task) => task.taskId,
              ),
            );
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: hooks.queue.tasks.map((task) => {
                if (
                  command === "jm_download_pause_all" &&
                  ["queued", "downloading", "verifying"].includes(task.phase)
                )
                  return {
                    ...task,
                    revision: task.revision + 1,
                    phase: "paused",
                    allowedActions: ["resume"],
                  };
                if (command === "jm_download_resume_many" && ids.has(task.id))
                  return {
                    ...task,
                    revision: task.revision + 1,
                    phase: "queued",
                    allowedActions: ["pause"],
                  };
                return task;
              }),
            };
            save();
            return clone(hooks.queue);
          }
          if (command === "jm_download_history_remove") {
            const requests = args.tasks as {
              taskId: string;
              expectedRevision: number;
            }[];
            if (
              !requests.every((request) =>
                hooks.queue.tasks.some(
                  (task) =>
                    task.id === request.taskId &&
                    task.revision === request.expectedRevision &&
                    task.phase === "downloaded",
                ),
              )
            )
              throw { code: "DOWNLOAD_TASK_STALE" };
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: hooks.queue.tasks.filter(
                (task) =>
                  !requests.some((request) => request.taskId === task.id),
              ),
            };
            save();
            return clone(hooks.queue);
          }
          if (command === "jm_download_confirm") {
            if (
              !preparedPlan ||
              preparedPlan.planId !== args.planId ||
              preparedPlan.revision !== args.expectedRevision
            )
              throw { code: "DOWNLOAD_PLAN_STALE" };
            if (
              hooks.queue.tasks.some(
                (task) =>
                  task.phase !== "downloaded" || task.localFiles !== "missing",
              )
            )
              throw { code: "DOWNLOAD_ALREADY_EXISTS" };
            const task: DownloadTask = {
              id: String(args.planId),
              revision: 1,
              source: preparedPlan.source,
              workId: preparedPlan.workId,
              title: preparedPlan.title,
              phase: "downloading",
              filesDone: 0,
              filesTotal: null,
              bytesDone: 0,
              errorCode: null,
              allowedActions: ["pause"],
              libraryEntryId: null,
              localFiles: null,
              updatedAt: 1,
              destinationDisplay: preparedPlan.destinationDisplay,
            };
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: [...hooks.queue.tasks, task],
            };
            save();
            return clone(hooks.queue);
          }
          if (command === "jm_download_control") {
            const target = hooks.queue.tasks.find(
              (task) => task.id === args.taskId,
            );
            const scope = args.scope as {
              source: DownloadSource;
              sessionId: string;
            };
            if (!target || target.source !== scope.source)
              throw { code: "DOWNLOAD_SOURCE_MISMATCH" };
            if (
              args.action !== "pause" &&
              !hooks.accounts.some(
                (account) =>
                  account.source === target.source &&
                  account.state === "connected" &&
                  account.sessionId === scope.sessionId,
              )
            )
              throw { code: "DOWNLOAD_SESSION_REQUIRED" };
            hooks.queue = {
              revision: hooks.queue.revision + 1,
              tasks: hooks.queue.tasks.map((task) =>
                task.id !== args.taskId
                  ? task
                  : {
                      ...task,
                      revision: task.revision + 1,
                      phase: args.action === "pause" ? "paused" : "downloading",
                      errorCode: null,
                      allowedActions:
                        args.action === "pause" ? ["resume"] : ["pause"],
                    },
              ),
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
  // The controller waits for any in-flight read before sending confirmation.
  // Wait for the actual invocation while retaining exact count and revision.
  await expect
    .poll(async () =>
      (await calls(page, "jm_download_confirm")).map(
        (call) => call.args.expectedRevision,
      ),
    )
    .toEqual([0]);
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

test("JM and Pica source details prepare only their own source without creating a task on cancel", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-favorites").click();
  await page.getByTestId("source-open-JM:123").click();
  await page.getByTestId("source-download").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-plan-source")).toContainText(
    "JM · 123",
  );
  await page.getByTestId("download-cancel").click();
  await page.getByTestId("source-detail-back").click();
  await page.getByTestId("source-tab-Pica").click();
  await page.getByTestId("source-open-Pica:0123456789abcdef01234567").click();
  await expect(page.getByTestId("source-download")).toBeEnabled();
  await page.getByTestId("source-download").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-plan-source")).toContainText(
    "哔咔 · 0123456789abcdef01234567",
  );
  await expect(page.getByTestId("download-plan-title")).toHaveText(
    "合成 Pica 作品",
  );
  await page.getByTestId("download-cancel").click();
  expect(
    (await calls(page, "jm_download_prepare")).map((call) => call.args.scope),
  ).toEqual([
    { source: "JM", sessionId: "session-JM" },
    { source: "Pica", sessionId: "session-Pica" },
  ]);
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

const oldTaskId = "c".repeat(64);
const newTaskId = "d".repeat(64);

test("typing a removed work directly rechecks stale presence before deciding whether a PC copy exists", async ({
  page,
}) => {
  await install(page, { completed: true });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  const readsBefore = (await calls(page, "jm_download_read")).length;
  await page.evaluate(() => {
    window.downloadTest.filePresence = "missing";
  });
  await page.getByTestId("download-input").fill("JM123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  expect(
    (await calls(page, "jm_download_read"))
      .slice(readsBefore)
      .map((call) => call.args.recheckFiles),
  ).toEqual([true]);
  expect(
    (await calls(page, "jm_download_prepare")).map((call) => call.args.input),
  ).toEqual(["123"]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
});

test("removed files leave the downloaded filter and require a new plan and confirmation", async ({
  page,
}) => {
  await install(page, { completed: true, phoneOwned: true });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  await page.getByTestId("download-filter-downloaded").click();
  await page.evaluate(() => {
    window.downloadTest.filePresence = "missing";
  });
  await page.getByTestId("download-read").click();
  await expect(page.getByTestId("download-task-" + oldTaskId)).toHaveCount(0);
  await page.getByTestId("download-filter-error").click();
  const oldTask = page.getByTestId("download-task-" + oldTaskId);
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  await expect(oldTask).toContainText("历史完成：3 / 3");
  await expect(oldTask).not.toContainText("电脑文件已保存并登记");
  await expect(page.getByTestId("download-open-" + oldTaskId)).toHaveCount(0);
  expect(await page.evaluate(() => window.downloadTest.queue.revision)).toBe(1);
  expect(await calls(page, "jm_download_control")).toEqual([]);
  await page.getByTestId("download-reprepare-" + oldTaskId).click();
  await expect(page.getByTestId("download-input")).toHaveValue("123");
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  await page.getByTestId("download-cancel").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  expect(
    await page.evaluate(() => window.downloadTest.queue.tasks),
  ).toHaveLength(1);
  await page.getByTestId("download-reprepare-" + oldTaskId).click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await page.getByTestId("download-confirm").click();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await page.getByTestId("download-filter-all").click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "正在下载",
  );
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  expect(
    (await calls(page, "jm_download_confirm")).map((call) => call.args.planId),
  ).toEqual([newTaskId]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
  expect(await calls(page, "library_scan")).toEqual([]);
  expect(await calls(page, "phone_library_mark")).toEqual([]);
});

test("changed or inaccessible files keep history without offering old-task retry or file opening", async ({
  page,
}) => {
  await install(page, { completed: true });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  for (const [localFiles, label] of [
    ["incomplete", "文件已变化"],
    ["unavailable", "目录不可用"],
  ] as const) {
    await page.evaluate((localFiles) => {
      window.downloadTest.filePresence = localFiles;
    }, localFiles);
    await page.getByTestId("download-read").click();
    await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
      label,
    );
    await expect(page.getByTestId("download-open-" + oldTaskId)).toHaveCount(0);
    await expect(page.getByTestId("download-retry-" + oldTaskId)).toHaveCount(
      0,
    );
    await expect(
      page.getByTestId("download-reprepare-" + oldTaskId),
    ).toHaveCount(0);
    await expect(page.getByTestId("download-task-" + oldTaskId)).toContainText(
      "历史完成：3 / 3",
    );
  }
  await expect(page.getByTestId("download-task-" + oldTaskId)).toContainText(
    "重新选择可访问的保存目录",
  );
  await page.getByTestId("download-select-directory-" + oldTaskId).click();
  await expect(page.getByTestId("library-workbench")).toBeVisible();
  expect(await calls(page, "jm_download_prepare")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

test("entry and focus recheck files only while the queue is active without replaying completion", async ({
  page,
}) => {
  await install(page, { completed: true });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  await page.getByTestId("nav-settings").click();
  await expect(page.getByTestId("native-downloads")).toBeHidden();
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  const before = (await calls(page, "jm_download_read")).length;
  const libraryReads = (await calls(page, "library_read")).length;
  await page.evaluate(() => {
    window.downloadTest.filePresence = "missing";
    window.dispatchEvent(new Event("focus"));
  });
  expect(await calls(page, "jm_download_read")).toHaveLength(before);
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  await page.evaluate(() => {
    window.downloadTest.filePresence = "present";
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  expect(
    (await calls(page, "jm_download_read")).every(
      (call) => call.args.recheckFiles === true,
    ),
  ).toBe(true);
  expect(await calls(page, "library_read")).toHaveLength(libraryReads);
  expect(await calls(page, "jm_download_control")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

const picaWorkId = "0123456789abcdef01234567";

test("manual Pica selection preserves the official link and only confirmed completion updates PC files", async ({
  page,
}) => {
  await install(page, { fixtureSource: "Pica", phoneOwned: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123");
  await page.getByTestId("download-source").selectOption("Pica");
  await expect(page.getByTestId("download-input")).toHaveValue("");
  const input = `https://picaapi.picacomic.com/comics/${picaWorkId}`;
  await page.getByTestId("download-input").fill(input);
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-plan-source")).toContainText(
    `哔咔 · ${picaWorkId}`,
  );
  await expect(page.getByTestId("download-confirmation")).toContainText(
    "哔咔保留原图格式",
  );
  expect((await calls(page, "jm_download_prepare"))[0].args).toEqual({
    scope: { source: "Pica", sessionId: "session-Pica" },
    input,
    rootId: "a".repeat(64),
    generation: 1,
  });
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await page.evaluate(() => window.downloadTest.pc.items)).toEqual([]);
  await page.getByTestId("download-confirm").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "正在下载",
  );
  await expect(page.getByTestId("download-task-" + oldTaskId)).toContainText(
    `哔咔 · ${picaWorkId}`,
  );
  expect(await calls(page, "jm_download_confirm")).toHaveLength(1);
  await page.reload();
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已暂停",
  );
  await expect(page.getByTestId("download-source")).toHaveValue("JM");
  expect(await calls(page, "jm_download_control")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  await page.getByTestId("download-resume-" + oldTaskId).click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "正在下载",
  );
  expect((await calls(page, "jm_download_control"))[0].args.scope).toEqual({
    source: "Pica",
    sessionId: "session-Pica",
  });
  await page.evaluate(() => window.downloadTest.advance("downloaded"));
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  await expect
    .poll(async () => (await calls(page, "library_read")).length)
    .toBeGreaterThan(1);
  await page.getByTestId("download-open-" + oldTaskId).click();
  await expect(page.getByTestId("library-detail-stock")).toHaveText(
    "已入库 · 手机名单",
  );
  expect(
    await page.evaluate(() => window.downloadTest.pc.items[0].sourceRef),
  ).toEqual({ source: "Pica", workId: picaWorkId });
  expect(await calls(page, "library_scan")).toEqual([]);
  expect(await calls(page, "phone_library_mark")).toEqual([]);
});

test("a Pica preparation returning a JM plan cannot show a confirmation or create a task", async ({
  page,
}) => {
  await install(page, { wrongPlanSource: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-source").selectOption("Pica");
  await page.getByTestId("download-input").fill(picaWorkId);
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-error")).toBeVisible();
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  expect(await calls(page, "jm_download_prepare")).toHaveLength(1);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await page.evaluate(() => window.downloadTest.queue.tasks)).toEqual(
    [],
  );
});

test("mixed queue controls use each task account regardless of the source selector and logout still permits pause", async ({
  page,
}) => {
  await install(page, { mixedQueue: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-source").selectOption("Pica");
  await page.getByTestId("download-resume-" + oldTaskId).click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "正在下载",
  );
  await page.getByTestId("download-pause-" + oldTaskId).click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已暂停",
  );
  await page.getByTestId("download-source").selectOption("JM");
  await page.getByTestId("download-retry-" + newTaskId).click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "正在下载",
  );
  expect(
    (await calls(page, "jm_download_control")).map((call) => [
      call.args.scope,
      call.args.action,
    ]),
  ).toEqual([
    [{ source: "JM", sessionId: "session-JM" }, "resume"],
    [{ source: "JM", sessionId: "session-JM" }, "pause"],
    [{ source: "Pica", sessionId: "session-Pica" }, "retry"],
  ]);
  await page.evaluate(() => {
    window.downloadTest.accounts = window.downloadTest.accounts.map(
      (account) =>
        account.source === "Pica"
          ? {
              ...account,
              state: "disconnected",
              sessionId: null,
              accountId: null,
              displayName: null,
            }
          : account,
    );
  });
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("accounts-reload").click();
  await expect(page.getByTestId("account-connect-Pica")).toHaveText(
    "连接哔咔账号",
  );
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-pause-" + newTaskId)).toBeEnabled();
  await page.getByTestId("download-pause-" + newTaskId).click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "已暂停",
  );
  await expect(page.getByTestId("download-resume-" + newTaskId)).toBeDisabled();
  await expect(page.getByTestId("download-resume-" + oldTaskId)).toBeEnabled();
  expect((await calls(page, "jm_download_control")).at(-1)?.args.scope).toEqual(
    { source: "Pica", sessionId: "" },
  );
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("Pica missing-file reprepare selects the original task source and creates a separate confirmed attempt", async ({
  page,
}) => {
  await install(page, {
    completed: true,
    fixtureSource: "Pica",
    phoneOwned: true,
  });
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-source")).toHaveValue("JM");
  await page.evaluate(() => {
    window.downloadTest.filePresence = "missing";
    window.dispatchEvent(new Event("focus"));
  });
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  await page.getByTestId("download-reprepare-" + oldTaskId).click();
  await expect(page.getByTestId("download-source")).toHaveValue("Pica");
  await expect(page.getByTestId("download-input")).toHaveValue(picaWorkId);
  await expect(page.getByTestId("download-plan-source")).toContainText(
    `哔咔 · ${picaWorkId}`,
  );
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  await page.getByTestId("download-confirm").click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "正在下载",
  );
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  expect((await calls(page, "jm_download_prepare"))[0].args.scope).toEqual({
    source: "Pica",
    sessionId: "session-Pica",
  });
  expect(
    (await calls(page, "jm_download_confirm")).map((call) => call.args.planId),
  ).toEqual([newTaskId]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

for (const fixtureSource of ["JM", "Pica"] as const)
  test(`Pica preparation distinguishes an existing ${fixtureSource} PC reference`, async ({
    page,
  }) => {
    await install(page, { existing: true, fixtureSource });
    await page.getByTestId("nav-queue").click();
    await page.getByTestId("download-source").selectOption("Pica");
    await page.getByTestId("download-input").fill(picaWorkId);
    await page.getByTestId("download-prepare").click();
    if (fixtureSource === "JM") {
      await expect(page.getByTestId("download-plan-source")).toContainText(
        "哔咔",
      );
      await page.getByTestId("download-cancel").click();
    } else {
      await expect(page.getByTestId("library-detail")).toBeVisible();
      await expect(page.getByTestId("library-reference")).toContainText(
        `Pica · ${picaWorkId}`,
      );
    }
    expect(await calls(page, "jm_download_prepare")).toHaveLength(
      fixtureSource === "JM" ? 1 : 0,
    );
    expect(await calls(page, "jm_download_confirm")).toEqual([]);
  });

test("batch review excludes duplicates and cancellation never queues tasks", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123\nJM124\nJM123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-batch-confirmation")).toBeVisible();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  await expect(page.getByTestId("download-batch-issues")).toContainText(
    "JM123",
  );
  await expect(page.getByTestId("download-batch-confirmation")).toContainText(
    "C:\\Synthetic\\124",
  );
  await page.getByTestId("download-batch-cancel").click();
  expect(await calls(page, "jm_download_batch_confirm")).toEqual([]);
  await expect(page.getByTestId("download-empty")).toBeVisible();
});

test("multiple queued works pause and resume explicitly under one source after restart", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123\nJM124");
  await page.getByTestId("download-prepare").click();
  await page.getByTestId("download-batch-confirm").click();
  await expect(
    page.getByTestId("download-phase-" + "7b".padStart(64, "0")),
  ).toHaveText("正在下载");
  await expect(
    page.getByTestId("download-phase-" + "7c".padStart(64, "0")),
  ).toHaveText("等待下载");
  await page.getByTestId("download-pause-all").click();
  await expect(page.getByTestId("download-resume-many-JM")).toHaveText(
    "继续JM 2 本",
  );
  await page.reload();
  await page.getByTestId("nav-queue").click();
  expect(await calls(page, "jm_download_resume_many")).toEqual([]);
  await expect(
    page.getByTestId("download-phase-" + "7b".padStart(64, "0")),
  ).toHaveText("已暂停");
  await page.getByTestId("download-resume-many-JM").click();
  await expect(
    page.getByTestId("download-phase-" + "7b".padStart(64, "0")),
  ).toHaveText("等待下载");
  await expect
    .poll(async () => (await calls(page, "jm_download_resume_many")).length)
    .toBe(1);
  expect((await calls(page, "jm_download_resume_many"))[0].args).toEqual({
    scope: { source: "JM", sessionId: "session-JM" },
    tasks: [
      { taskId: "7b".padStart(64, "0"), expectedRevision: 2 },
      { taskId: "7c".padStart(64, "0"), expectedRevision: 2 },
    ],
  });
});

test("completed history filtering and removal preserve PC entries", async ({
  page,
}) => {
  await install(page, { completed: true });
  const before = await page.evaluate(() =>
    JSON.stringify(window.downloadTest.pc),
  );
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-filter-history").click();
  await page.getByTestId("download-history-query").fill("123");
  await page.getByTestId("download-history-clear").click();
  await expect(page.getByTestId("download-history-confirmation")).toContainText(
    "电脑漫画文件、电脑索引和手机名单会保留",
  );
  await page.getByTestId("download-history-confirm").click();
  await expect(page.getByTestId("download-empty")).toBeVisible();
  expect(
    await page.evaluate(() => JSON.stringify(window.downloadTest.pc)),
  ).toBe(before);
  expect(await calls(page, "jm_download_history_remove")).toHaveLength(1);
});

test("favorite multi-selection opens reviewed batch before any download", async ({
  page,
}) => {
  await install(page, { batchWorks: true });
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("source-card-JM:124")).toBeVisible();
  await page.getByTestId("source-toggle-selection").click();
  await page.getByTestId("source-select-all").click();
  await page.getByTestId("source-batch-download").click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(2);
  expect(
    (await calls(page, "jm_download_batch_prepare"))[0].args.inputs,
  ).toEqual(["123", "124"]);
  expect(await calls(page, "jm_download_batch_confirm")).toEqual([]);
  await page.getByTestId("download-batch-confirm").click();
  await expect(page.getByTestId("native-downloads")).toBeVisible();
  expect(await calls(page, "jm_download_batch_confirm")).toHaveLength(1);
});
