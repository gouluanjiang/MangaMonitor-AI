import test from "node:test";
import assert from "node:assert/strict";
import {
  DownloadController,
  filterDownloadTasks,
  downloadQueueFilters,
  downloadQueueSummary,
  downloadCompletedAt,
  downloadAttentionReason,
  downloadBatchProgress,
} from "../src/download-runtime.ts";

const hash = (n) => n.toString(16).padStart(64, "0");
const context = {
  scope: { source: "JM", sessionId: "synthetic" },
  rootId: hash(90),
  generation: 1,
};
const task = (n, phase, overrides = {}) => ({
  id: hash(n),
  revision: 1,
  source: "JM",
  workId: String(n),
  title: `Synthetic ${n}`,
  phase,
  filesDone: 1,
  filesTotal: 1,
  bytesDone: 100,
  errorCode: null,
  allowedActions: [],
  libraryEntryId: phase === "downloaded" ? hash(n + 100) : null,
  localFiles: phase === "downloaded" ? "present" : null,
  updatedAt: Date.parse("2026-09-26T00:00:00.000Z") + n,
  destinationDisplay: `C:\\Synthetic\\${n}.zip`,
  ...overrides,
});
const plan = (n) => ({
  planId: hash(n),
  revision: 1,
  source: "JM",
  workId: String(n),
  title: `Synthetic ${n}`,
  authors: [],
  destinationDisplay: `C:\\Synthetic\\${n}.zip`,
  rootId: context.rootId,
  generation: 1,
});

test("every valid task belongs to exactly one category including full-image pending registration and changed files", () => {
  const tasks = [
    ...["queued", "downloading", "verifying", "saving", "paused", "error"].map(
      (phase, n) => task(n, phase),
    ),
    ...["present", "missing", "incomplete", "unavailable"].map(
      (localFiles, n) => task(n + 10, "downloaded", { localFiles }),
    ),
  ];
  const groups = downloadQueueFilters.map((filter) =>
    filterDownloadTasks(tasks, filter),
  );
  assert.deepEqual(
    groups.map((group) => group.length),
    [5, 4, 1],
  );
  assert.equal(groups.flat().length, tasks.length);
  assert.equal(
    new Set(groups.flat().map((item) => item.id)).size,
    tasks.length,
  );
  assert.deepEqual(downloadQueueSummary(tasks), {
    processing: 3,
    waiting: 1,
    paused: 1,
    attention: 4,
    downloaded: 1,
  });
});

test("success ordering uses completion time with stable ties and presence checks do not rewrite the input", () => {
  const tasks = [
    task(1, "downloaded"),
    task(3, "downloaded"),
    task(2, "downloaded", {
      updatedAt: Date.parse("2026-09-26T00:00:00.003Z"),
    }),
  ];
  const before = structuredClone(tasks);
  assert.deepEqual(
    filterDownloadTasks(tasks, "downloaded").map((item) => item.id),
    [hash(2), hash(3), hash(1)],
  );
  assert.deepEqual(tasks, before);
  assert.equal(downloadCompletedAt(tasks[0]), "2026-09-26T00:00:00.001Z");
  const changed = tasks.map((item) =>
    item.id === hash(2) ? { ...item, localFiles: "missing" } : item,
  );
  assert.deepEqual(
    filterDownloadTasks(changed, "downloaded").map((item) => item.id),
    [hash(3), hash(1)],
  );
  assert.equal(downloadCompletedAt(changed[2]), downloadCompletedAt(tasks[2]));
  assert.equal(downloadCompletedAt(task(1, "saving")), null);
  assert.equal(
    downloadCompletedAt(task(1, "downloaded", { updatedAt: 0 })),
    null,
  );
  assert.equal(
    downloadCompletedAt(
      task(1, "downloaded", { updatedAt: Number.MAX_SAFE_INTEGER }),
    ),
    null,
  );
});

test("errors distinguish account, registration, validation and file state without making actions", () => {
  assert.equal(
    downloadAttentionReason(
      task(1, "error", { errorCode: "INDEX_WRITE_FAILED" }),
    ),
    "保存成功，入库登记未完成",
  );
  assert.equal(
    downloadAttentionReason(
      task(1, "error", { errorCode: "DOWNLOAD_SESSION_REQUIRED" }),
    ),
    "需要连接来源账号",
  );
  assert.equal(
    downloadAttentionReason(
      task(1, "error", { errorCode: "DOWNLOAD_VERIFY_FAILED" }),
    ),
    "完整校验未通过",
  );
  assert.equal(
    downloadAttentionReason(task(1, "error", { errorCode: "LIBRARY_BUSY" })),
    "漫画库目录需要处理",
  );
  assert.equal(
    downloadAttentionReason(
      task(1, "downloaded", { localFiles: "unavailable" }),
    ),
    "目录不可用",
  );
});

test("recent confirmed batch excludes old history, survives filtered views and history cleanup, and resets with a new confirmation", async () => {
  let snapshot = { revision: 1, tasks: [task(10, "downloaded")] };
  const controller = new DownloadController({
    read: async () => structuredClone(snapshot),
    prepareBatch: async () => ({
      batchId: hash(80),
      plans: [plan(1), plan(2)],
      issues: [],
    }),
    confirmBatch: async () =>
      (snapshot = {
        revision: 2,
        tasks: [
          ...snapshot.tasks,
          task(1, "paused"),
          task(2, "error", { errorCode: "SOURCE_TIMEOUT" }),
        ],
      }),
    removeHistory: async (items) =>
      (snapshot = {
        revision: snapshot.revision + 1,
        tasks: snapshot.tasks.filter(
          (item) => !items.some((remove) => item.id === remove.taskId),
        ),
      }),
    prepare: async () => plan(3),
    confirm: async () =>
      (snapshot = {
        revision: snapshot.revision + 1,
        tasks: [...snapshot.tasks, task(3, "paused")],
      }),
  });
  await controller.read();
  assert.equal(controller.getState().recentBatch, null);
  await controller.prepareBatch(context, ["1", "2"]);
  assert.equal(await controller.confirmBatch(context), true);
  assert.deepEqual(controller.getState().recentBatch.taskIds, [
    hash(1),
    hash(2),
  ]);
  assert.deepEqual(
    downloadBatchProgress(snapshot.tasks, controller.getState().recentBatch),
    { total: 2, completed: 0, attention: 1 },
  );
  snapshot = {
    revision: 3,
    tasks: [task(10, "downloaded"), task(1, "downloaded"), task(2, "error")],
  };
  await controller.read();
  filterDownloadTasks(snapshot.tasks, "downloaded", "does not exist", "Pica");
  assert.deepEqual(
    downloadBatchProgress(snapshot.tasks, controller.getState().recentBatch),
    { total: 2, completed: 1, attention: 1 },
  );
  await controller.removeHistory([snapshot.tasks[1]]);
  assert.deepEqual(
    downloadBatchProgress(snapshot.tasks, controller.getState().recentBatch),
    { total: 2, completed: 1, attention: 1 },
  );
  await controller.prepare(context, "3");
  assert.equal(await controller.confirm(context), true);
  assert.deepEqual(controller.getState().recentBatch.taskIds, [hash(3)]);
  const retained = structuredClone(controller.getState().recentBatch);
  await controller.prepare(context, "3");
  controller.cancelPlan();
  assert.equal(await controller.confirm(context), false);
  assert.deepEqual(controller.getState().recentBatch, retained);
  controller.adapter.confirm = async () => {
    throw new Error("synthetic failure");
  };
  await controller.prepare(context, "3");
  assert.equal(await controller.confirm(context), false);
  assert.deepEqual(controller.getState().recentBatch, retained);
  controller.dispose();
  const reopened = new DownloadController({
    read: async () => structuredClone(snapshot),
  });
  await reopened.read();
  assert.equal(reopened.getState().recentBatch, null);
  assert.equal(
    reopened.getState().snapshot.tasks.length,
    snapshot.tasks.length,
  );
  reopened.dispose();
});

test("invalid or cancelled confirmations do not invent a batch or count unrelated completions", async () => {
  const controller = new DownloadController({
    prepare: async () => plan(1),
    confirm: async () => ({ revision: 1, tasks: [task(9, "downloaded")] }),
  });
  await controller.prepare(context, "1");
  controller.cancelPlan();
  assert.equal(await controller.confirm(context), false);
  assert.equal(controller.getState().recentBatch, null);
  await controller.prepare(context, "1");
  assert.equal(await controller.confirm(context), false);
  assert.equal(controller.getState().recentBatch, null);
  controller.dispose();
});
