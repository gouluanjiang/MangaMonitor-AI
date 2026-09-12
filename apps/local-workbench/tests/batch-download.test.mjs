import test from "node:test";
import assert from "node:assert/strict";
import {
  DownloadController,
  DownloadError,
  createDownloadAdapter,
  validateDownloadBatchPlan,
  parseDownloadInputs,
  filterDownloadTasks,
} from "../src/download-runtime.ts";

const hash = (n) => n.toString(16).padStart(64, "0");
const context = {
  scope: { source: "JM", sessionId: "synthetic" },
  rootId: hash(9),
  generation: 1,
};
const plan = (n) => ({
  planId: hash(n),
  revision: 1,
  source: "JM",
  workId: String(n),
  title: `合成作品 ${n}`,
  authors: [],
  destinationDisplay: `C:\\Synthetic\\${n}`,
  rootId: hash(9),
  generation: 1,
});
const batch = () => ({
  batchId: hash(100),
  plans: [plan(1), plan(2)],
  issues: [{ input: "JM1", errorCode: "DOWNLOAD_BATCH_DUPLICATE" }],
});
const task = (n, phase = "queued") => ({
  id: hash(n),
  revision: 1,
  source: "JM",
  workId: String(n),
  title: `合成作品 ${n}`,
  phase,
  filesDone: 0,
  filesTotal: null,
  bytesDone: 0,
  errorCode: null,
  allowedActions: ["pause"],
  libraryEntryId: null,
  localFiles: null,
  updatedAt: 1,
  destinationDisplay: `C:\\Synthetic\\${n}`,
});

test("batch input bounds and response identities reject ambiguous plans", () => {
  assert.deepEqual(parseDownloadInputs(" JM1\r\n\n JM2\n"), ["JM1", "JM2"]);
  assert.throws(
    () => parseDownloadInputs(Array(51).fill("JM1").join("\n")),
    DownloadError,
  );
  assert.throws(() => parseDownloadInputs(" "), DownloadError);
  assert.equal(validateDownloadBatchPlan(batch()).plans.length, 2);
  for (const value of [
    { ...batch(), plans: [plan(1), plan(1)] },
    { ...batch(), batchId: null },
    { batchId: hash(100), plans: [], issues: [] },
    { ...batch(), issues: [{ input: "JM1", errorCode: "URL secret" }] },
  ])
    assert.throws(() => validateDownloadBatchPlan(value), DownloadError);
});

test("batch adapter binds source/root and sends only scoped identifiers", async () => {
  const calls = [];
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return command.endsWith("prepare")
        ? batch()
        : { revision: 2, tasks: [task(1), task(2)] };
    },
  });
  await adapter.prepareBatch(
    { ...context, path: "NEVER", scope: { ...context.scope, token: "NEVER" } },
    ["JM1", "JM2", "JM1"],
  );
  await adapter.confirmBatch(hash(100));
  await adapter.resumeMany(context.scope, [
    { taskId: hash(1), expectedRevision: 1, path: "NEVER" },
  ]);
  await adapter.pauseAll();
  await adapter.removeHistory([{ taskId: hash(2), expectedRevision: 1 }]);
  assert.deepEqual(
    calls.map((v) => v.command),
    [
      "jm_download_batch_prepare",
      "jm_download_batch_confirm",
      "jm_download_resume_many",
      "jm_download_pause_all",
      "jm_download_history_remove",
    ],
  );
  assert.doesNotMatch(JSON.stringify(calls), /NEVER|token|path/);
  const wrong = createDownloadAdapter({
    native: true,
    invoke: async () => ({
      ...batch(),
      plans: [{ ...plan(1), rootId: hash(10) }],
    }),
  });
  await assert.rejects(
    wrong.prepareBatch(context, ["JM1"]),
    /DOWNLOAD_PLAN_STALE/,
  );
});

test("batch cancellation and source/root change never confirm; partial confirmation is rejected", async () => {
  let confirms = 0;
  const adapter = {
    read: async () => ({ revision: 1, tasks: [] }),
    prepareBatch: async () => batch(),
    confirmBatch: async () => {
      confirms++;
      return { revision: 2, tasks: [task(1)] };
    },
  };
  const controller = new DownloadController(adapter);
  await controller.prepareBatch(context, ["JM1", "JM2", "JM1"]);
  controller.cancelPlan();
  assert.equal(await controller.confirmBatch(context), false);
  await controller.prepareBatch(context, ["JM1", "JM2", "JM1"]);
  assert.equal(
    await controller.confirmBatch({ ...context, generation: 2 }),
    false,
  );
  assert.equal(confirms, 0);
  await controller.prepareBatch(context, ["JM1", "JM2", "JM1"]);
  assert.equal(await controller.confirmBatch(context), false);
  assert.equal(controller.getState().snapshot.tasks.length, 0);
  assert.equal(confirms, 1);
  controller.dispose();
});

test("confirmed batch accepts unrelated queue progress and pause/resume/history stay explicit", async () => {
  let revision = 1;
  const calls = [];
  const adapter = {
    read: async () => ({ revision: ++revision, tasks: [task(3)] }),
    prepareBatch: async () => batch(),
    confirmBatch: async () => {
      calls.push("confirm");
      return { revision: ++revision, tasks: [task(3), task(1), task(2)] };
    },
    pauseAll: async () => {
      calls.push("pause");
      return {
        revision: ++revision,
        tasks: [{ ...task(1, "paused"), allowedActions: ["resume"] }],
      };
    },
    resumeMany: async () => {
      calls.push("resume");
      return { revision: ++revision, tasks: [task(1)] };
    },
    removeHistory: async () => {
      calls.push("history");
      return { revision: ++revision, tasks: [task(1)] };
    },
  };
  const controller = new DownloadController(adapter);
  await controller.prepareBatch(context, ["JM1", "JM2", "JM1"]);
  await controller.read(false);
  assert.equal(await controller.confirmBatch(context), true);
  await controller.pauseAll();
  await controller.resumeMany(
    context.scope,
    controller.getState().snapshot.tasks,
  );
  assert.equal(await controller.removeHistory([task(1)]), false);
  assert.equal(
    await controller.removeHistory([
      { ...task(2, "downloaded"), allowedActions: [] },
    ]),
    true,
  );
  assert.deepEqual(calls, ["confirm", "pause", "resume", "history"]);
  controller.dispose();
});

test("history filters retain removed-file completion without treating it as current download", () => {
  const rows = [
    task(1),
    { ...task(2, "downloaded"), localFiles: "present" },
    { ...task(3, "downloaded"), localFiles: "missing" },
  ];
  assert.deepEqual(
    filterDownloadTasks(rows, "history").map((v) => v.workId),
    ["2", "3"],
  );
  assert.deepEqual(
    filterDownloadTasks(rows, "downloaded").map((v) => v.workId),
    ["2"],
  );
  assert.equal(filterDownloadTasks(rows, "history", "作品 3", "JM").length, 1);
  assert.equal(filterDownloadTasks(rows, "all", "", "Pica").length, 0);
});
