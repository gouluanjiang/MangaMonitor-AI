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
    () => parseDownloadInputs(Array(501).fill("JM1").join("\n")),
    DownloadError,
  );
  assert.throws(() => parseDownloadInputs(" "), DownloadError);
  assert.equal(
    parseDownloadInputs(
      Array.from({ length: 500 }, (_, i) => String(i + 1)).join("\n"),
    ).length,
    500,
  );
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

function selectionAdapter() {
  const calls = [],
    plans = [];
  return {
    calls,
    plans,
    read: async () => ({ revision: 1, tasks: [] }),
    prepareBatch: async (ctx, inputs, retained) => {
      calls.push({ source: ctx.scope.source, inputs, retained });
      const rows = inputs.map((input) => ({
        ...plan(Number(input)),
        source: ctx.scope.source,
        planId: hash(
          plans.length +
            Number(input) +
            (ctx.scope.source === "Pica" ? 10000 : 0),
        ),
        workId: input,
      }));
      plans.push(...rows);
      return { batchId: hash(1000 + calls.length), plans: rows, issues: [] };
    },
    confirmSelection: async (ids) => {
      calls.push({ confirm: ids });
      return {
        revision: 2,
        tasks: plans.map((p) => ({
          ...task(1),
          id: p.planId,
          workId: p.workId,
          source: p.source,
        })),
      };
    },
    cancelBatch: async () => {
      calls.push({ cancel: true });
    },
  };
}
const bothContexts = {
  JM: context,
  Pica: { ...context, scope: { source: "Pica", sessionId: "pica-session" } },
};

test("large mixed-source selection prepares bounded chunks, then confirms all once", async () => {
  const adapter = selectionAdapter(),
    controller = new DownloadController(adapter);
  const inputs = [
    ...Array.from({ length: 61 }, (_, i) => ({
      source: "JM",
      input: String(i + 1),
    })),
    { source: "Pica", input: "71" },
    { source: "Pica", input: "72" },
  ];
  await controller.prepareSelection(bothContexts, inputs);
  assert.deepEqual(
    adapter.calls.map((c) => c.inputs.length),
    [20, 20, 20, 1, 2],
  );
  assert.deepEqual(
    adapter.calls.map((c) => c.retained.length),
    [0, 1, 2, 3, 4],
  );
  assert.equal(controller.getState().batchPlan.plans.length, 63);
  assert.equal(controller.getState().snapshot.tasks.length, 0);
  assert.equal(await controller.confirmBatch(bothContexts), true);
  assert.equal(controller.getState().snapshot.tasks.length, 63);
  assert.equal(adapter.calls.filter((c) => c.confirm).length, 1);
  controller.dispose();
});

test("cancel during a chunk ignores its late result and never starts another chunk", async () => {
  const adapter = selectionAdapter(),
    original = adapter.prepareBatch;
  let release;
  adapter.prepareBatch = async (...args) => {
    const result = await original(...args);
    if (adapter.calls.filter((c) => c.inputs).length === 2)
      await new Promise((resolve) => {
        release = resolve;
      });
    return result;
  };
  const controller = new DownloadController(adapter);
  const pending = controller.prepareBatch(
    context,
    Array.from({ length: 61 }, (_, i) => String(i + 1)),
  );
  while (!release) await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(controller.getState().preparation, { done: 20, total: 61 });
  controller.cancelPlan();
  release();
  await pending;
  assert.equal(controller.getState().batchPlan, null);
  assert.equal(adapter.calls.filter((c) => c.inputs).length, 2);
  assert.equal(await controller.confirmBatch(context), false);
  assert.equal(adapter.calls.filter((c) => c.confirm).length, 0);
  controller.dispose();
});

test("a lost item or changed second-source session cannot confirm a prepared prefix", async () => {
  const adapter = selectionAdapter(),
    original = adapter.prepareBatch;
  adapter.prepareBatch = async (...args) => {
    const next = await original(...args);
    if (adapter.calls.filter((c) => c.inputs).length === 2) next.plans.pop();
    return next;
  };
  const controller = new DownloadController(adapter);
  await controller.prepareBatch(
    context,
    Array.from({ length: 41 }, (_, i) => String(i + 1)),
  );
  assert.equal(controller.getState().batchPlan, null);
  assert.notEqual(controller.getState().error, "");
  assert.equal(await controller.confirmBatch(context), false);
  controller.dispose();
  const valid = selectionAdapter(),
    changed = new DownloadController(valid);
  await changed.prepareSelection(bothContexts, [
    { source: "JM", input: "1" },
    { source: "Pica", input: "2" },
  ]);
  assert.equal(
    await changed.confirmBatch({
      ...bothContexts,
      Pica: {
        ...bothContexts.Pica,
        scope: { source: "Pica", sessionId: "changed" },
      },
    }),
    false,
  );
  assert.equal(valid.calls.filter((c) => c.confirm).length, 0);
  changed.dispose();
});
