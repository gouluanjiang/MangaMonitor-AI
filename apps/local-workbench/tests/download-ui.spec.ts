import { expect, test, type Page, type TestInfo } from "@playwright/test";
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
  initialTasks?: DownloadTask[];
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
    if (!stored && options.initialTasks)
      hooks.queue = { revision: 1, tasks: clone(options.initialTasks) };
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
    // Explicit fixtures model the native snapshot after restart/file checks.
    let restored = Boolean(options.initialTasks);
    let preparedPlan: DownloadPlan | null = null;
    let preparedBatch: DownloadBatchPlan | null = null;
    const preparedBatches = new Map<string, DownloadBatchPlan>();
    let batchSequence = 0;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
          if (command === "read_preferences") return clone(preferences);
          if (command === "read_booklists")
            return { revision: 0, value: { version: 1, lists: [] } };
          if (command === "source_accounts") return clone(hooks.accounts);
          if (command === "source_author_policy")
            return {
              source: args.source,
              sessionId: args.sessionId,
              revision: 0,
              author: args.author,
              queries: [args.author],
              verifiedAliases: [],
              exactCredits: [],
              queryFingerprint: "a".repeat(64),
            };
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
                    ? {
                        ...task,
                        localFiles: options.initialTasks
                          ? task.localFiles
                          : hooks.filePresence,
                      }
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
            const retained = (args.retainedBatchIds ?? []) as string[];
            for (const id of preparedBatches.keys())
              if (!retained.includes(id)) preparedBatches.delete(id);
            const seen = new Set<string>(
              [...preparedBatches.values()].flatMap((batch) =>
                batch.plans.map((plan) => plan.workId),
              ),
            );
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
            preparedBatch = {
              batchId: plans.length
                ? (++batchSequence).toString(16).padStart(64, "0")
                : null,
              plans,
              issues,
            };
            if (preparedBatch.batchId)
              preparedBatches.set(preparedBatch.batchId, preparedBatch);
            return clone(preparedBatch);
          }
          if (command === "jm_download_batch_cancel") {
            preparedBatches.clear();
            preparedBatch = null;
            return null;
          }
          if (
            command === "jm_download_batch_confirm" ||
            command === "jm_download_selection_confirm"
          ) {
            const ids = (
              command === "jm_download_batch_confirm"
                ? [args.batchId]
                : args.batchIds
            ) as string[];
            if (ids.some((id) => !preparedBatches.has(id)))
              throw { code: "DOWNLOAD_PLAN_STALE" };
            const tasks: DownloadTask[] = ids
              .flatMap((id) => preparedBatches.get(id)!.plans)
              .map((plan, index) => ({
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
              }));
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

function queueTask(
  index: number,
  phase: DownloadTask["phase"],
  overrides: Partial<DownloadTask> = {},
): DownloadTask {
  const source = overrides.source ?? "JM";
  return {
    id: index.toString(16).padStart(64, "0"),
    revision: 1,
    source,
    workId:
      source === "JM"
        ? String(1000 + index)
        : index.toString(16).padStart(24, "0"),
    title: "合成队列作品 " + index,
    phase,
    filesDone: phase === "downloaded" ? 3 : 1,
    filesTotal: 3,
    bytesDone: phase === "downloaded" ? 300 : 100,
    errorCode: phase === "error" ? "SOURCE_TIMEOUT" : null,
    allowedActions:
      phase === "paused"
        ? ["resume"]
        : phase === "error"
          ? ["retry"]
          : phase === "saving" || phase === "downloaded"
            ? []
            : ["pause"],
    libraryEntryId: phase === "downloaded" ? "b".repeat(64) : null,
    localFiles: phase === "downloaded" ? "present" : null,
    updatedAt: Date.parse("2026-09-26T12:00:00.000Z") + index * 60_000,
    destinationDisplay:
      "C:\\Synthetic\\下载队列视觉回归\\保留完整保存位置\\作品-" +
      index +
      ".zip",
    ...overrides,
  };
}

function categorizedQueue() {
  const active = [
    queueTask(1, "queued", { title: "合成等待作品" }),
    queueTask(2, "downloading", { title: "合成下载作品" }),
    queueTask(3, "verifying", { source: "Pica", title: "合成校验作品" }),
    queueTask(4, "saving", { title: "合成保存作品" }),
    queueTask(5, "paused", { source: "Pica", title: "合成暂停作品" }),
  ];
  const attention = [
    queueTask(6, "error", { title: "合成失败作品" }),
    queueTask(7, "downloaded", {
      source: "Pica",
      title: "合成文件已移除作品",
      localFiles: "missing",
    }),
    queueTask(8, "downloaded", {
      title: "合成文件变化作品",
      localFiles: "incomplete",
    }),
    queueTask(9, "downloaded", {
      source: "Pica",
      title: "合成目录不可用作品",
      localFiles: "unavailable",
    }),
  ];
  const downloaded = [
    queueTask(10, "downloaded", { title: "合成成功旧作品" }),
    queueTask(11, "downloaded", {
      source: "Pica",
      title: "合成成功新作品",
    }),
  ];
  return {
    active,
    attention,
    downloaded,
    tasks: [...active, ...attention, ...downloaded],
  };
}

const taskRows = (page: Page) =>
  page
    .getByTestId("native-downloads")
    .locator("[data-testid^='download-task-']");

async function expectQueueCounts(
  page: Page,
  active: number,
  attention: number,
  downloaded: number,
) {
  for (const [filter, label, count] of [
    ["active", "下载中", active],
    ["error", "下载失败／需要处理", attention],
    ["downloaded", "已下载", downloaded],
  ] as const) {
    await expect(page.getByTestId("download-filter-" + filter)).toHaveText(
      new RegExp(`^${label}\\s*[（(]?\\s*${count}\\s*[）)]?$`),
    );
  }
}

async function captureQueue(page: Page, testInfo: TestInfo, name: string) {
  const body = await page.screenshot({
    path: `visual-evidence/${name}.png`,
    fullPage: true,
  });
  await testInfo.attach(name, { body, contentType: "image/png" });
}

test("queue categories are exclusive and successful rows are compact with newest completion first", async ({
  page,
}, testInfo) => {
  const queue = categorizedQueue();
  await install(page, { initialTasks: queue.tasks });
  await page.getByTestId("nav-queue").click();
  await expectQueueCounts(page, 5, 4, 2);
  await expect(page.getByTestId("download-filter-active")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(
    page.locator("button[data-testid^='download-filter-']"),
  ).toHaveCount(3);
  await expect(page.getByTestId("download-filter-all")).toHaveCount(0);
  await expect(page.getByTestId("download-filter-history")).toHaveCount(0);
  await expect(page.getByTestId("download-batch-progress")).toHaveCount(0);
  const seen: string[] = [];
  for (const [filter, expected, imageName] of [
    ["active", queue.active, "queue-active"],
    ["error", queue.attention, "queue-attention"],
    ["downloaded", [...queue.downloaded].reverse(), "queue-downloaded-compact"],
  ] as const) {
    await page.getByTestId("download-filter-" + filter).click();
    await expect(taskRows(page)).toHaveCount(expected.length);
    const ids = await taskRows(page).evaluateAll((rows) =>
      rows.map((row) => row.getAttribute("data-testid")!),
    );
    expect([...ids].sort()).toEqual(
      expected.map((task) => "download-task-" + task.id).sort(),
    );
    seen.push(...ids);
    await captureQueue(page, testInfo, imageName);
  }
  expect(new Set(seen).size).toBe(queue.tasks.length);
  expect(seen).toHaveLength(queue.tasks.length);
  await expect(taskRows(page).locator("h3")).toHaveText(
    [...queue.downloaded].reverse().map((task) => task.title),
  );
  for (const task of queue.downloaded) {
    const row = page.getByTestId("download-task-" + task.id);
    const details = page.getByTestId("download-details-" + task.id);
    await expect(row.getByRole("heading", { name: task.title })).toBeVisible();
    await expect(
      row.getByText(
        `${task.source === "JM" ? "JM" : "哔咔"} · ${task.workId}`,
        { exact: true },
      ),
    ).toBeVisible();
    await expect(row.getByRole("progressbar")).toHaveCount(0);
    await expect(
      row.getByText(task.destinationDisplay, { exact: true }),
    ).toBeHidden();
    await expect(details).toHaveJSProperty("tagName", "DETAILS");
    await expect(details).not.toHaveAttribute("open", "");
    const completedAt = page.getByTestId("download-completed-at-" + task.id);
    await expect(completedAt).toBeVisible();
    await expect(completedAt).toHaveJSProperty("tagName", "TIME");
    await expect(completedAt).toHaveAttribute(
      "datetime",
      new Date(task.updatedAt).toISOString(),
    );
    await expect(completedAt).not.toHaveText("");
    await expect(page.getByTestId("download-open-" + task.id)).toBeVisible();
  }
  const expandedTask = queue.downloaded[1];
  await page
    .getByTestId("download-details-" + expandedTask.id)
    .locator("summary")
    .click();
  await expect(
    page.getByTestId("download-details-" + expandedTask.id),
  ).toHaveAttribute("open", "");
  await expect(
    page
      .getByTestId("download-task-" + expandedTask.id)
      .getByText(expandedTask.destinationDisplay, { exact: true }),
  ).toBeVisible();
  await captureQueue(page, testInfo, "queue-downloaded-expanded");
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

test("automatic completion leaves the active view without switching the selected category", async ({
  page,
}) => {
  await install(page);
  await prepare(page);
  await page.getByTestId("download-confirm").click();
  await expect(page.getByTestId("download-phase-" + "c".repeat(64))).toHaveText(
    "正在下载",
  );
  await expectQueueCounts(page, 1, 0, 0);
  await expect(page.getByTestId("download-batch-progress")).toContainText(
    "已完成 0 / 1 本",
  );
  await page.evaluate(() => window.downloadTest.advance("downloaded"));
  await expectQueueCounts(page, 0, 0, 1);
  await expect(page.getByTestId("download-batch-progress")).toContainText(
    "已完成 1 / 1 本",
  );
  await expect(page.getByTestId("download-filter-active")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(taskRows(page)).toHaveCount(0);
  await expect(page.getByTestId("download-empty")).toBeVisible();
  await expect(page.getByTestId("download-empty")).toContainText(
    "当前没有下载中的任务",
  );
  await expect(page.getByTestId("download-history-clear")).toHaveCount(0);
  await expect
    .poll(async () => (await calls(page, "library_read")).length)
    .toBeGreaterThan(1);
  await page.getByTestId("download-filter-downloaded").click();
  await expect(
    page.getByTestId("download-open-" + "c".repeat(64)),
  ).toBeVisible();
  expect(await calls(page, "jm_download_confirm")).toHaveLength(1);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

test("search and source filters scope every category count and empty result without changing queue totals", async ({
  page,
}) => {
  const queue = categorizedQueue();
  await install(page, { initialTasks: queue.tasks });
  await page.getByTestId("nav-queue").click();
  await expectQueueCounts(page, 5, 4, 2);
  const summary = page.getByTestId("download-summary");
  await expect(summary).toContainText("处理中 3");
  await expect(summary).toContainText("等待 1");
  await expect(summary).toContainText("暂停 1");
  await expect(summary).toContainText("需处理 4");
  await expect(summary).toContainText("已下载 2");
  const totals = await summary.innerText();
  await page.getByTestId("download-history-source").selectOption("Pica");
  await expectQueueCounts(page, 2, 2, 1);
  for (const [filter, count] of [
    ["active", 2],
    ["error", 2],
    ["downloaded", 1],
  ] as const) {
    await page.getByTestId("download-filter-" + filter).click();
    await expect(taskRows(page)).toHaveCount(count);
    await expect(page.getByTestId("download-history-source")).toHaveValue(
      "Pica",
    );
    await expect(summary).toHaveText(totals);
  }
  await page.getByTestId("download-history-query").fill("合成成功");
  await expectQueueCounts(page, 0, 0, 1);
  await expect(taskRows(page).locator("h3")).toHaveText(["合成成功新作品"]);
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-history-query")).toHaveValue(
    "合成成功",
  );
  await expect(page.getByTestId("download-empty")).toBeVisible();
  await expect(page.getByTestId("download-empty")).toContainText(
    "当前筛选没有结果",
  );
  await expect(page.getByTestId("download-history-clear")).toHaveCount(0);
  await page.getByTestId("download-history-query").fill("没有对应的合成任务");
  await expectQueueCounts(page, 0, 0, 0);
  for (const filter of ["active", "error", "downloaded"]) {
    await page.getByTestId("download-filter-" + filter).click();
    await expect(taskRows(page)).toHaveCount(0);
    await expect(page.getByTestId("download-empty")).toBeVisible();
  }
  await expect(page.getByTestId("download-history-clear")).toBeDisabled();
  await expect(summary).toHaveText(totals);
  await page.getByTestId("download-history-query").fill(queue.active[2].workId);
  await expectQueueCounts(page, 1, 0, 0);
  await page.getByTestId("download-filter-active").click();
  await expect(taskRows(page).locator("h3")).toHaveText([
    queue.active[2].title,
  ]);
  await page.getByTestId("download-history-source").selectOption("JM");
  await expectQueueCounts(page, 0, 0, 0);
  await page.getByTestId("download-history-source").selectOption("all");
  await page.getByTestId("download-history-query").fill("");
  await expectQueueCounts(page, 5, 4, 2);
  await expect(taskRows(page)).toHaveCount(5);
  expect(await calls(page, "jm_download_history_remove")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
});

test("an invalid completed-file snapshot leaves the last valid queue visible and reports the read error", async ({
  page,
}) => {
  const completed = queueTask(1, "downloaded");
  await install(page, { initialTasks: [completed] });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-filter-downloaded").click();
  await expectQueueCounts(page, 0, 0, 1);
  await page.evaluate(() => {
    window.downloadTest.queue.tasks[0].localFiles = null;
    window.downloadTest.queue.revision++;
  });
  await page.getByTestId("download-read").click();
  await expect(page.getByTestId("download-error")).toBeVisible();
  await expectQueueCounts(page, 0, 0, 1);
  await expect(page.getByTestId("download-phase-" + completed.id)).toHaveText(
    "已下载",
  );
  await expect(page.getByTestId("download-empty")).toHaveCount(0);
  expect(await calls(page, "jm_download_history_remove")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

test("disconnected task accounts disable retries while file checks and account settings remain safe", async ({
  page,
}) => {
  const failed = queueTask(1, "error", {
    source: "Pica",
    errorCode: "SOURCE_AUTH_REQUIRED",
  });
  const missing = queueTask(2, "downloaded", {
    source: "Pica",
    localFiles: "missing",
  });
  await install(page, { initialTasks: [failed, missing] });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-retry-" + failed.id)).toBeEnabled();
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
  await expectQueueCounts(page, 0, 2, 0);
  await expect(page.getByTestId("download-source")).toHaveValue("JM");
  await expect(page.getByTestId("download-retry-" + failed.id)).toBeDisabled();
  await expect(page.getByTestId("download-task-" + failed.id)).toContainText(
    "请连接哔咔账号",
  );
  await expect(page.getByTestId("download-open-" + missing.id)).toHaveCount(0);
  await expect(
    page.getByTestId("download-history-remove-" + missing.id),
  ).toBeEnabled();
  const before = (await calls(page, "jm_download_read")).length;
  await page.getByTestId("download-recheck-" + missing.id).click();
  await expect
    .poll(async () => (await calls(page, "jm_download_read")).length)
    .toBeGreaterThan(before);
  expect(
    (await calls(page, "jm_download_read")).at(-1)?.args.recheckFiles,
  ).toBe(true);
  await expect(page.getByTestId("download-retry-" + failed.id)).toBeDisabled();
  await page.getByTestId("download-accounts-" + failed.id).click();
  await expect(page.getByTestId("account-connect-Pica")).toBeVisible();
  expect(await calls(page, "jm_download_control")).toEqual([]);
  expect(await calls(page, "jm_download_prepare")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toEqual([]);
  expect(await calls(page, "jm_download_history_remove")).toEqual([]);
});

test("scoped successful-history cleanup and abnormal single removal both require confirmation and retain PC entries", async ({
  page,
}) => {
  const queue = categorizedQueue();
  const selected = queue.downloaded[1];
  const abnormal = queue.attention[1];
  await install(page, { completed: true, initialTasks: queue.tasks });
  const before = await page.evaluate(() =>
    JSON.stringify(window.downloadTest.pc),
  );
  await page.getByTestId("nav-queue").click();
  await expect(page.getByTestId("download-history-clear")).toHaveCount(0);
  await page.getByTestId("download-filter-downloaded").click();
  await page.getByTestId("download-history-source").selectOption("Pica");
  await page.getByTestId("download-history-query").fill("成功");
  await expect(taskRows(page)).toHaveCount(1);
  await page.getByTestId("download-history-clear").click();
  const confirmation = page.getByTestId("download-history-confirmation");
  await expect(confirmation).toContainText("漫画文件和漫画库索引会保留");
  await expect(confirmation.locator("li")).toHaveCount(1);
  await expect(confirmation).toContainText(selected.title);
  await expect(confirmation).not.toContainText(queue.downloaded[0].title);
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  expect(await calls(page, "jm_download_history_remove")).toEqual([]);
  await expect(taskRows(page)).toHaveCount(1);
  await page.getByTestId("download-history-clear").click();
  await page.getByTestId("download-history-confirm").click();
  await expect(confirmation).toHaveCount(0);
  await expect(taskRows(page)).toHaveCount(0);
  await expect(page.getByTestId("download-empty")).toBeVisible();
  expect((await calls(page, "jm_download_history_remove"))[0].args).toEqual({
    tasks: [{ taskId: selected.id, expectedRevision: selected.revision }],
  });
  await page.getByTestId("download-history-query").fill("");
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-history-clear")).toHaveCount(0);
  await page.getByTestId("download-history-remove-" + abnormal.id).click();
  await expect(confirmation.locator("li")).toHaveCount(1);
  await expect(confirmation).toContainText(abnormal.title);
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  expect(await calls(page, "jm_download_history_remove")).toHaveLength(1);
  await page.getByTestId("download-history-remove-" + abnormal.id).click();
  await page.getByTestId("download-history-confirm").click();
  await expect(confirmation).toHaveCount(0);
  expect((await calls(page, "jm_download_history_remove"))[1].args).toEqual({
    tasks: [{ taskId: abnormal.id, expectedRevision: abnormal.revision }],
  });
  expect(
    await page.evaluate(() => JSON.stringify(window.downloadTest.pc)),
  ).toBe(before);
  expect(
    await page.evaluate(() =>
      window.downloadTest.queue.tasks.map((task) => task.id),
    ),
  ).toEqual(
    queue.tasks
      .filter((task) => task.id !== selected.id && task.id !== abnormal.id)
      .map((task) => task.id),
  );
  expect(await calls(page, "library_scan")).toEqual([]);
  expect(await calls(page, "jm_download_control")).toEqual([]);
});

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

test("more than fifty choices retain every chunk and enter the queue in one confirmation", async ({
  page,
}) => {
  await install(page);
  await page.getByTestId("nav-queue").click();
  const ids = Array.from({ length: 61 }, (_, i) => `JM${1000 + i}`);
  await page.getByTestId("download-input").fill([...ids, ids[0]].join("\n"));
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-batch-plan")).toHaveCount(61);
  await expect(page.getByTestId("download-batch-issues")).toContainText(
    "JM1000",
  );
  const prepares = await calls(page, "jm_download_batch_prepare");
  expect(prepares.map((call) => (call.args.inputs as string[]).length)).toEqual(
    [20, 20, 20, 2],
  );
  expect(
    await page.evaluate(() => window.downloadTest.queue.tasks.length),
  ).toBe(0);
  await page.screenshot({
    path: "visual-evidence/large-download-selection.png",
  });
  await page.getByTestId("download-batch-confirm").click();
  await expect(page.getByTestId("download-batch-confirmation")).toHaveCount(0);
  expect(await calls(page, "jm_download_selection_confirm")).toHaveLength(1);
  expect(await calls(page, "jm_download_batch_confirm")).toHaveLength(0);
  expect(
    await page.evaluate(() =>
      window.downloadTest.queue.tasks.map((task) => task.workId),
    ),
  ).toEqual(ids.map((id) => id.slice(2)));
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
    "保存为一个 ZIP",
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
  await page.getByTestId("download-filter-error").click();
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
  await page.getByTestId("download-filter-active").click();
  await expect(
    page.getByTestId(
      "download-phase-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ),
  ).toHaveText("正在下载");
  await page.evaluate(() => window.downloadTest.advance("downloaded"));
  await page.getByTestId("download-filter-downloaded").click();
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
    "已入库 · 电脑漫画库",
  );
  expect(await calls(page, "library_scan")).toEqual([]);
  expect(await calls(page, "phone_library_mark")).toEqual([]);
  expect(await calls(page, "jm_download_confirm")).toHaveLength(1);
});

test("full image progress during a library rescan stays pending until final registration", async ({
  page,
}) => {
  await install(page, { fixtureSource: "Pica" });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-source").selectOption("Pica");
  await page.getByTestId("download-input").fill("0123456789abcdef01234567");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-confirmation")).toBeVisible();
  await page.getByTestId("download-confirm").click();
  const taskId = "c".repeat(64);
  // Confirmation includes an asynchronous inventory refresh. Only advance the
  // synthetic task after the UI observes the committed queue entry.
  await expect(page.getByTestId("download-phase-" + taskId)).toHaveText(
    "正在下载",
  );
  await page.evaluate(() => {
    window.downloadTest.advance("error");
    const task = window.downloadTest.queue.tasks[0];
    task.filesDone = task.filesTotal!;
    task.errorCode = "LIBRARY_BUSY";
    window.downloadTest.queue.revision++;
  });
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-finalization-pending")).toContainText(
    "保存或入库尚未完成",
  );
  await expect(page.getByTestId("download-task-" + taskId)).toContainText(
    "完成目录读取后重试当前操作",
  );
  await expect(page.getByTestId("download-phase-" + taskId)).toHaveText(
    "需要处理",
  );
  await expect(page.getByTestId("download-open-" + taskId)).toHaveCount(0);
  await page.getByTestId("download-retry-" + taskId).click();
  await page.getByTestId("download-filter-active").click();
  await expect(page.getByTestId("download-phase-" + taskId)).toHaveText(
    "正在下载",
  );
  await page.evaluate(() => window.downloadTest.advance("downloaded"));
  await page.getByTestId("download-filter-downloaded").click();
  await expect(page.getByTestId("download-phase-" + taskId)).toHaveText(
    "已下载",
  );
  await expect(page.getByTestId("download-finalization-pending")).toHaveCount(
    0,
  );
  await expect(page.getByTestId("download-open-" + taskId)).toBeVisible();
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

test("an unregistered PC metadata reference does not override explicit source download selection", async ({
  page,
}) => {
  await install(page, { existing: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-input").fill("JM123");
  await page.getByTestId("download-prepare").click();
  await expect(page.getByTestId("download-plan-source")).toContainText("JM");
  expect(await calls(page, "jm_download_prepare")).toHaveLength(1);
  await page.getByTestId("download-cancel").click();
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
  await page.getByTestId("download-filter-downloaded").click();
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
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
});

test("removed files leave the downloaded filter and require a new plan and confirmation", async ({
  page,
}) => {
  await install(page, { completed: true, phoneOwned: true });
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("download-filter-downloaded").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
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
  await page.getByTestId("download-filter-active").click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "正在下载",
  );
  await expect(page.getByTestId("download-task-" + oldTaskId)).toHaveCount(0);
  await page.getByTestId("download-filter-error").click();
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
  await page.getByTestId("download-filter-downloaded").click();
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
    await page.getByTestId("download-filter-error").click();
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
  await page.getByTestId("download-filter-downloaded").click();
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
  await page.getByTestId("download-filter-error").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "文件已移除",
  );
  await page.evaluate(() => {
    window.downloadTest.filePresence = "present";
    window.dispatchEvent(new Event("focus"));
  });
  await page.getByTestId("download-filter-downloaded").click();
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
  await page.getByTestId("download-filter-downloaded").click();
  await expect(page.getByTestId("download-phase-" + oldTaskId)).toHaveText(
    "已下载",
  );
  await expect
    .poll(async () => (await calls(page, "library_read")).length)
    .toBeGreaterThan(1);
  await page.getByTestId("download-open-" + oldTaskId).click();
  await expect(page.getByTestId("library-detail-stock")).toHaveText(
    "已入库 · 电脑漫画库",
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
  await page.getByTestId("download-filter-error").click();
  await page.getByTestId("download-retry-" + newTaskId).click();
  await page.getByTestId("download-filter-active").click();
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
  await page.getByTestId("download-filter-error").click();
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
  await page.getByTestId("download-filter-active").click();
  await expect(page.getByTestId("download-phase-" + newTaskId)).toHaveText(
    "正在下载",
  );
  await expect(page.getByTestId("download-task-" + oldTaskId)).toHaveCount(0);
  await page.getByTestId("download-filter-error").click();
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
  test(`Pica preparation ignores an unregistered ${fixtureSource} PC metadata reference`, async ({
    page,
  }) => {
    await install(page, { existing: true, fixtureSource });
    await page.getByTestId("nav-queue").click();
    await page.getByTestId("download-source").selectOption("Pica");
    await page.getByTestId("download-input").fill(picaWorkId);
    await page.getByTestId("download-prepare").click();
    await expect(page.getByTestId("download-plan-source")).toContainText(
      "哔咔",
    );
    await page.getByTestId("download-cancel").click();
    expect(await calls(page, "jm_download_prepare")).toHaveLength(1);
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
  await page.getByTestId("download-filter-downloaded").click();
  await page.getByTestId("download-history-query").fill("123");
  await page.getByTestId("download-history-clear").click();
  await expect(page.getByTestId("download-history-confirmation")).toContainText(
    "漫画文件和漫画库索引会保留",
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
