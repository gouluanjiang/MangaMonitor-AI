import test from "node:test";
import assert from "node:assert/strict";
import {
  DownloadController,
  DownloadError,
  downloadActionState,
  validateDownloadSnapshot,
} from "../src/download-runtime.ts";
import { downloadInputWorkId } from "../src/download-input.ts";
import {
  DownloadAttentionTracker,
  DownloadAttentionAggregator,
} from "../src/download-attention.ts";

const hash = (number) => number.toString(16).padStart(64, "0");
const context = {
  scope: { source: "JM", sessionId: "synthetic" },
  rootId: hash(100),
  generation: 1,
};
const contexts = { JM: context, Pica: null };
const input = (n) => ({ source: "JM", input: `JM${n}` });
const row = (n, overrides = {}) => ({
  id: hash(n),
  revision: 1,
  source: "JM",
  workId: String(n),
  title: `合成作品 ${n}`,
  rootId: context.rootId,
  tags: [],
  phase: "queued",
  filesDone: 2,
  filesTotal: 4,
  bytesDone: 42,
  errorCode: null,
  allowedActions: ["pause"],
  libraryEntryId: null,
  localFiles: null,
  updatedAt: 1,
  destinationDisplay: `C:\\Synthetic\\${n}.zip`,
  ...overrides,
});
function deferred() {
  let resolve;
  return {
    promise: new Promise((r) => {
      resolve = r;
    }),
    resolve: (value) => resolve(value),
  };
}
function fixture(initial = [], customize = {}) {
  let revision = 1,
    tasks = initial;
  const calls = [];
  const prepared = new Map();
  const snapshot = () => ({ revision: ++revision, tasks: [...tasks] });
  const adapter = {
    read: async () => snapshot(),
    prepare: async (ctx, value) => {
      const id = downloadInputWorkId(ctx.scope.source, value);
      calls.push(`prepare:${id}`);
      if (!id) throw new DownloadError("SOURCE_WORK_ID_INVALID");
      await customize.prepare?.(id);
      let planId = hash(Number(id));
      while (tasks.some((task) => task.id === planId))
        planId = hash(Number.parseInt(planId, 16) + 1000);
      const plan = {
        planId,
        revision: 1,
        source: ctx.scope.source,
        workId: id,
        title: `合成作品 ${id}`,
        authors: [],
        destinationDisplay: `C:\\Synthetic\\${id}.zip`,
        rootId: ctx.rootId,
        generation: ctx.generation,
      };
      prepared.set(planId, plan);
      return plan;
    },
    confirm: async (id, expectedRevision) => {
      assert.equal(expectedRevision, 1);
      const plan = prepared.get(id);
      const n = Number(plan.workId);
      calls.push(`confirm:${n}`);
      tasks.push(row(n, { id, rootId: plan.rootId }));
      return snapshot();
    },
    control: async (scope, id, expectedRevision, action) => {
      const current = tasks.find((task) => task.id === id);
      assert.equal(current.revision, expectedRevision);
      assert.equal(scope.source, current.source);
      calls.push(`${action}:${current.workId}`);
      tasks = tasks.map((task) =>
        task.id === id
          ? {
              ...task,
              revision: task.revision + 1,
              phase: "queued",
              allowedActions: ["pause"],
              errorCode: null,
            }
          : task,
      );
      return snapshot();
    },
  };
  return { adapter, controller: new DownloadController(adapter), calls };
}

test("rapid different clicks are serialized instead of lost, and each click has immediate pending feedback", async () => {
  const gate = deferred();
  const f = fixture([], {
    prepare: (id) => (id === "1" ? gate.promise : undefined),
  });
  const first = f.controller.enqueueSelection(contexts, [input(1)]);
  const second = f.controller.enqueueSelection(contexts, [input(2)]);
  assert.equal(
    downloadActionState(f.controller.getState(), "JM", "2").label,
    "正在加入…",
  );
  gate.resolve();
  const [a, b] = await Promise.all([first, second]);
  assert.equal(a.failed.length + b.failed.length, 0);
  assert.deepEqual(f.calls, [
    "prepare:1",
    "confirm:1",
    "prepare:2",
    "confirm:2",
  ]);
  assert.deepEqual(f.controller.getState().submittingKeys, []);
  assert.equal(
    downloadActionState(f.controller.getState(), "JM", "1").label,
    "已排队",
  );
  assert.equal(f.controller.getState().plan, null);
  assert.equal(f.controller.getState().batchPlan, null);
  f.controller.dispose();
});

test("failed and paused works use revision-bound retry/resume and retain progress", async () => {
  const f = fixture([
    row(1, {
      phase: "error",
      errorCode: "DOWNLOAD_MEDIA_NETWORK_ERROR",
      allowedActions: ["retry"],
    }),
    row(2, { phase: "paused", allowedActions: ["resume"] }),
  ]);
  await f.controller.read(false);
  assert.equal(
    downloadActionState(f.controller.getState(), "JM", "1").label,
    "重试下载",
  );
  const result = await f.controller.enqueueSelection(contexts, [
    input(1),
    input(2),
  ]);
  assert.deepEqual(f.calls, ["retry:1", "resume:2"]);
  assert.deepEqual(
    result.accepted.map((item) => item.outcome),
    ["retried", "resumed"],
  );
  assert.ok(
    f.controller
      .getState()
      .snapshot.tasks.every(
        (task) => task.filesDone === 2 && task.bytesDone === 42,
      ),
  );
  f.controller.dispose();
});

test("already active work and duplicate inputs remain idempotent without blocking later valid work", async () => {
  const f = fixture([row(1)]);
  const result = await f.controller.enqueueSelection(contexts, [
    input(1),
    input(2),
    { source: "JM", input: "0002" },
    input(3),
  ]);
  assert.deepEqual(f.calls, [
    "prepare:2",
    "confirm:2",
    "prepare:3",
    "confirm:3",
  ]);
  assert.equal(result.accepted.length, 3);
  assert.equal(result.accepted[0].outcome, "existing");
  const repeat = await f.controller.enqueueSelection(contexts, [input(2)]);
  assert.equal(repeat.accepted[0].outcome, "existing");
  assert.equal(f.calls.length, 4);
  f.controller.dispose();
});

test("a batch preserves failed selections and still admits subsequent work; queue polling remains healthy", async () => {
  const f = fixture([], {
    prepare: (id) => {
      if (id === "2") throw new DownloadError("DOWNLOAD_METADATA_INVALID");
    },
  });
  const result = await f.controller.enqueueSelection(contexts, [
    input(1),
    input(2),
    input(3),
  ]);
  assert.deepEqual(
    result.accepted.map((item) => item.workId),
    ["1", "3"],
  );
  assert.deepEqual(
    result.failed.map((item) => [item.workId, item.errorCode]),
    [["2", "DOWNLOAD_METADATA_INVALID"]],
  );
  assert.deepEqual(f.calls, [
    "prepare:1",
    "confirm:1",
    "prepare:2",
    "prepare:3",
    "confirm:3",
  ]);
  assert.equal(f.controller.getState().error, "");
  assert.equal(f.controller.getState().submissionIssues.length, 1);
  f.controller.dispose();
});

test("changing account or root while preparation yields never confirms under the old context", async () => {
  for (const replacement of [
    { ...context, generation: 2 },
    { ...context, rootId: hash(101) },
    { ...context, scope: { ...context.scope, sessionId: "other" } },
  ]) {
    const prepared = deferred(),
      release = deferred();
    let current = contexts;
    const f = fixture([], {
      prepare: async () => {
        prepared.resolve();
        await release.promise;
      },
    });
    const resultPromise = f.controller.enqueueSelection(
      contexts,
      [input(1)],
      () => current,
    );
    await prepared.promise;
    current = { JM: replacement, Pica: null };
    release.resolve();
    const result = await resultPromise;
    assert.deepEqual(f.calls, ["prepare:1"]);
    assert.equal(result.failed[0].errorCode, "DOWNLOAD_PLAN_STALE");
    f.controller.dispose();
  }
});

test("task identity includes its library root and missing finished output can be downloaded again", async () => {
  const f = fixture([
    row(1, { rootId: hash(999), phase: "error", allowedActions: ["retry"] }),
    row(2, {
      phase: "downloaded",
      localFiles: "missing",
      allowedActions: [],
      libraryEntryId: hash(200),
    }),
  ]);
  const result = await f.controller.enqueueSelection(contexts, [
    input(1),
    input(2),
  ]);
  assert.deepEqual(f.calls, [
    "prepare:1",
    "confirm:1",
    "prepare:2",
    "confirm:2",
  ]);
  assert.equal(result.failed.length, 0);
  f.controller.dispose();
});

test("unmount during preparation and waiting requests cannot later start a native download", async () => {
  const gate = deferred(),
    prepared = deferred();
  const f = fixture([], {
    prepare: async () => {
      prepared.resolve();
      await gate.promise;
    },
  });
  const a = f.controller.enqueueSelection(contexts, [input(1)]);
  const b = f.controller.enqueueSelection(contexts, [input(2)]);
  await prepared.promise;
  f.controller.dispose();
  gate.resolve();
  assert.equal((await a).failed[0].errorCode, "DOWNLOAD_REQUEST_CANCELLED");
  assert.equal((await b).failed[0].errorCode, "DOWNLOAD_REQUEST_CANCELLED");
  assert.deepEqual(f.calls, ["prepare:1"]);
});

test("only supported ID/link shapes can target an existing retry; arbitrary pasted URLs do not gain authority", () => {
  for (const value of [
    "JM123",
    "000123",
    "https://18comic.vip/album/123",
    "https://www.cdnhth.cc/album?id=123",
  ])
    assert.equal(downloadInputWorkId("JM", value), "123");
  for (const value of [
    "https://untrusted.invalid/album/123",
    "http://18comic.vip/album/123",
    "https://u@18comic.vip/album/123",
    "https://18comic.vip/album/123?token=secret",
    "https://18comic.vip/album/123#x",
  ])
    assert.equal(downloadInputWorkId("JM", value), null);
  assert.equal(
    downloadInputWorkId("Pica", "ABCDEF0123456789ABCDEF01"),
    "abcdef0123456789abcdef01",
  );
  assert.equal(
    downloadInputWorkId(
      "Pica",
      "https://picaapi.picacomic.com/comics/abcdef0123456789abcdef01",
    ),
    "abcdef0123456789abcdef01",
  );
});

test("download DTO preserves explicit filtering tags and roots but tolerates historical field absence", () => {
  const valid = { revision: 1, tasks: [row(1, { tags: ["BL"] })] };
  assert.deepEqual(validateDownloadSnapshot(valid), valid);
  const legacy = row(1);
  delete legacy.tags;
  delete legacy.rootId;
  assert.deepEqual(
    validateDownloadSnapshot({ revision: 1, tasks: [legacy] }).tasks[0],
    legacy,
  );
  assert.throws(() =>
    validateDownloadSnapshot({ ...valid, tasks: [row(1, { tags: [null] })] }),
  );
  assert.throws(() =>
    validateDownloadSnapshot({ ...valid, tasks: [row(1, { rootId: "bad" })] }),
  );
});

test("historical failures and successful completion stay silent, new failures group once and can notify after retry", () => {
  const tracker = new DownloadAttentionTracker();
  const snapshot = (...tasks) => ({ revision: 1, tasks });
  const failed = (n) =>
    row(n, {
      phase: "error",
      allowedActions: ["retry"],
      errorCode: "DOWNLOAD_MEDIA_NETWORK_ERROR",
    });
  assert.deepEqual(tracker.observe(snapshot(failed(1), row(2))), []);
  assert.deepEqual(tracker.observe(snapshot(failed(1), failed(2))), [hash(2)]);
  assert.deepEqual(tracker.observe(snapshot(failed(1), failed(2))), []);
  assert.deepEqual(tracker.observe(snapshot(row(1), failed(2))), []);
  assert.deepEqual(tracker.observe(snapshot(failed(1), failed(2))), [hash(1)]);
  assert.deepEqual(
    tracker.observe(
      snapshot(
        failed(1),
        failed(2),
        row(3, { phase: "downloaded", localFiles: "present" }),
      ),
    ),
    [],
  );
  assert.deepEqual(
    tracker.observe(
      snapshot(failed(1), failed(2), row(4, { ...failed(4), tags: ["BL"] })),
    ),
    [],
  );
  const notices = [],
    aggregator = new DownloadAttentionAggregator((notice) =>
      notices.push(notice),
    );
  aggregator.push([hash(1)]);
  aggregator.push([hash(2), hash(1)]);
  aggregator.flush();
  aggregator.flush();
  assert.equal(notices.length, 1);
  assert.equal(notices[0].count, 2);
  assert.equal(notices[0].message, "2 个下载任务需要处理");
  aggregator.dispose();
});
