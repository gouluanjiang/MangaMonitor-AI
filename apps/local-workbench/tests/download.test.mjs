import test from "node:test";
import assert from "node:assert/strict";
import {
  createDownloadAdapter,
  DownloadController,
  DownloadError,
  downloadErrorMessage,
  downloadTaskLabel,
  filterDownloadTasks,
  isDownloadPresent,
  getDownloadScope,
  canControlDownload,
  validateDownloadPlan,
  validateDownloadSnapshot,
} from "../src/download-runtime.ts";

test("media failures distinguish transport, redirects and invalid images without exposing raw errors", () => {
  for (const [code, expected] of [
    ["DOWNLOAD_MEDIA_TIMEOUT", /响应超时/],
    ["DOWNLOAD_MEDIA_NETWORK_ERROR", /无法连接图片服务器/],
    ["DOWNLOAD_MEDIA_ADDRESS_UNSUPPORTED", /暂不支持的图片地址/],
    ["DOWNLOAD_MEDIA_REDIRECT_FAILED", /跳转地址异常/],
    ["DOWNLOAD_MEDIA_ACCESS_DENIED", /图片服务器拒绝访问/],
    ["DOWNLOAD_MEDIA_UNAVAILABLE", /图片暂时不可用/],
    ["DOWNLOAD_MEDIA_RATE_LIMITED", /暂时限制请求/],
    ["DOWNLOAD_MEDIA_SERVER_ERROR", /图片服务器暂时出错/],
    ["DOWNLOAD_MEDIA_EMPTY", /返回了空内容/],
    ["DOWNLOAD_MEDIA_TOO_LARGE", /超过当前支持的大小/],
    ["DOWNLOAD_MEDIA_IMAGE_INVALID", /无法解码或格式不符/],
  ]) {
    const message = downloadErrorMessage({ code });
    assert.match(message, expected);
    assert.match(message, /保留/);
    assert.doesNotMatch(message, /重新登录/);
  }
  const unsafe = "PICA_MEDIA_HTTP_403: https://private.invalid/?token=secret";
  assert.doesNotMatch(
    downloadErrorMessage({ code: unsafe }),
    /private|secret|https:/,
  );
});
const rootId = "a".repeat(64),
  entryId = "b".repeat(64);
const context = {
  scope: { source: "JM", sessionId: "synthetic-JM" },
  rootId,
  generation: 1,
};
const plan = (overrides = {}) => ({
  planId: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  revision: 1,
  source: "JM",
  workId: "123",
  title: "合成单本作品",
  authors: ["合成作者"],
  destinationDisplay: "C:\\Synthetic\\合成单本作品",
  rootId,
  generation: 1,
  ...overrides,
});
const task = (overrides = {}) => ({
  id: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  revision: 1,
  source: "JM",
  workId: "123",
  title: "合成单本作品",
  phase: "paused",
  filesDone: 1,
  filesTotal: 3,
  bytesDone: 100,
  errorCode: null,
  allowedActions: ["resume"],
  libraryEntryId: null,
  localFiles: null,
  updatedAt: 1,
  destinationDisplay: "C:\\Synthetic\\合成单本作品",
  ...overrides,
});
const snapshot = (tasks = [], revision = 1) => ({ revision, tasks });
const pending = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};

test("only complete registered desktop output is accepted as downloaded", () => {
  assert.equal(
    validateDownloadSnapshot(
      snapshot([
        task({
          phase: "downloaded",
          filesDone: 3,
          allowedActions: [],
          libraryEntryId: entryId,
          localFiles: "present",
        }),
      ]),
    ).tasks[0].phase,
    "downloaded",
  );
  for (const change of [
    { libraryEntryId: null },
    { filesDone: 2 },
    { filesTotal: null },
    { errorCode: "DOWNLOAD_INCOMPLETE" },
    { allowedActions: ["retry"] },
  ])
    assert.throws(
      () =>
        validateDownloadSnapshot(
          snapshot([
            task({
              phase: "downloaded",
              filesDone: 3,
              allowedActions: [],
              libraryEntryId: entryId,
              localFiles: "present",
              ...change,
            }),
          ]),
        ),
      DownloadError,
    );
  assert.equal(
    validateDownloadSnapshot(snapshot([task({ filesTotal: null })])).tasks[0]
      .filesTotal,
    null,
  );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task(), task()])),
    DownloadError,
  );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task({ source: "Pica" })])),
    DownloadError,
  );
  assert.throws(
    () =>
      validateDownloadSnapshot(
        snapshot([task({ phase: "paused", allowedActions: ["retry"] })]),
      ),
    DownloadError,
  );
});

test("plan identities retain twenty digit JM strings and reject malformed bindings", () => {
  const id = "12345678901234567890";
  assert.equal(validateDownloadPlan(plan({ revision: 0 })).revision, 0);
  assert.equal(validateDownloadPlan(plan({ workId: id })).workId, id);
  for (const change of [
    { workId: "0" },
    { workId: "01" },
    { workId: id + "1" },
    { source: "Pica" },
    { rootId: "C:\\Synthetic" },
    { generation: -1 },
  ])
    assert.throws(() => validateDownloadPlan(plan(change)), DownloadError);
});

test("native queue and task identity bounds match storage", () => {
  const tasks = Array.from({ length: 50 }, (_, index) =>
    task({ id: (index + 1).toString(16).padStart(64, "0") }),
  );
  assert.equal(validateDownloadSnapshot(snapshot(tasks)).tasks.length, 50);
  assert.throws(
    () => validateDownloadSnapshot(snapshot([...tasks, task()])),
    DownloadError,
  );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task({ revision: 0 })])),
    DownloadError,
  );
  assert.throws(
    () => validateDownloadPlan(plan({ planId: "plan-1" })),
    DownloadError,
  );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task({ id: "task-1" })])),
    DownloadError,
  );
});

test("adapter forwards only the explicit typed JM preparation and control fields", async () => {
  const calls = [];
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return command === "jm_download_prepare" ? plan() : snapshot([task()]);
    },
  });
  await adapter.prepare(
    {
      ...context,
      path: "C:\\Never sent",
      scope: { ...context.scope, token: "Never sent" },
    },
    " JM123 ",
  );
  await adapter.confirm(
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    1,
  );
  await adapter.control(
    context.scope,
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    1,
    "resume",
  );
  assert.deepEqual(calls[0], {
    command: "jm_download_prepare",
    args: { scope: context.scope, input: "JM123", rootId, generation: 1 },
  });
  assert.deepEqual(calls[1].args, {
    planId: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    expectedRevision: 1,
  });
  assert.deepEqual(calls[2].args, {
    scope: context.scope,
    taskId: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    expectedRevision: 1,
    action: "resume",
  });
  const count = calls.length;
  await assert.rejects(
    adapter.prepare(
      { ...context, scope: { source: "Unknown", sessionId: "Pica" } },
      "123",
    ),
    DownloadError,
  );
  await assert.rejects(
    adapter.control(context.scope, "../task", 1, "retry"),
    DownloadError,
  );
  assert.equal(calls.length, count);
});

test("browser mode cannot invoke native download commands and raw error text is discarded", async () => {
  let calls = 0;
  await assert.rejects(
    createDownloadAdapter({
      native: false,
      invoke: async () => {
        calls++;
      },
    }).read(),
    DownloadError,
  );
  assert.equal(calls, 0);
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async () => {
      throw { code: "SOURCE_TIMEOUT", message: "synthetic-secret-token" };
    },
  });
  try {
    await adapter.read();
    assert.fail();
  } catch (error) {
    assert.equal(error.code, "SOURCE_TIMEOUT");
    assert.equal(JSON.stringify(error).includes("synthetic-secret"), false);
    assert.equal(
      downloadErrorMessage(error).includes("synthetic-secret"),
      false,
    );
  }
});

test("restoring paused tasks performs one read and never resumes them", async () => {
  const calls = [];
  const controller = new DownloadController({
    read: async () => {
      calls.push("read");
      return snapshot([task()]);
    },
    control: async () => {
      calls.push("control");
    },
  });
  await controller.read();
  assert.equal(controller.getState().ready, true);
  assert.equal(controller.getState().snapshot.tasks[0].phase, "paused");
  assert.deepEqual(calls, ["read"]);
  controller.dispose();
});

test("a pause waits for native release and polling discovers the explicit resume action", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let reads = 0;
  const controller = new DownloadController({
    read: async () =>
      snapshot(
        [task({ allowedActions: ++reads === 1 ? [] : ["resume"] })],
        reads,
      ),
  });
  await controller.read();
  assert.deepEqual(controller.getState().snapshot.tasks[0].allowedActions, []);
  t.mock.timers.tick(1000);
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(reads, 2);
  assert.deepEqual(controller.getState().snapshot.tasks[0].allowedActions, [
    "resume",
  ]);
  t.mock.timers.tick(5000);
  assert.equal(reads, 2);
  controller.dispose();
});

test("canceling an in-flight preparation prevents its late plan and never creates a task", async () => {
  const gate = pending();
  let confirms = 0;
  const controller = new DownloadController({
    prepare: async () => gate.promise,
    confirm: async () => {
      confirms++;
      return snapshot();
    },
  });
  const preparation = controller.prepare(context, "123");
  await Promise.resolve();
  controller.cancelPlan();
  gate.resolve(plan());
  await preparation;
  assert.equal(controller.getState().plan, null);
  assert.equal(await controller.confirm(context), false);
  assert.equal(confirms, 0);
  controller.dispose();
});

test("changing account or selected directory invalidates an unconfirmed plan", async () => {
  let confirms = 0;
  const controller = new DownloadController({
    prepare: async () => plan(),
    confirm: async () => {
      confirms++;
      return snapshot([task()]);
    },
  });
  await controller.prepare(context, "123");
  assert.equal(await controller.confirm({ ...context, generation: 2 }), false);
  await controller.prepare(context, "123");
  assert.equal(
    await controller.confirm({
      ...context,
      scope: { source: "JM", sessionId: "changed" },
    }),
    false,
  );
  assert.equal(confirms, 0);
  assert.equal(controller.getState().plan, null);
  controller.dispose();
});

test("double confirmation creates one task and preserves the exact plan revision", async () => {
  const gate = pending();
  const calls = [];
  const controller = new DownloadController({
    prepare: async () => plan({ revision: 9 }),
    confirm: async (...args) => {
      calls.push(args);
      return gate.promise;
    },
  });
  await controller.prepare(context, "123");
  const first = controller.confirm(context);
  const second = controller.confirm(context);
  await Promise.resolve();
  gate.resolve(snapshot([task()]));
  assert.equal(await first, true);
  assert.equal(await second, false);
  assert.deepEqual(calls, [
    ["cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", 9],
  ]);
  assert.equal(controller.getState().plan, null);
  controller.dispose();
});

test("read polling is single-flight and a queued mutation waits for the read result", async () => {
  const gate = pending(),
    calls = [];
  const controller = new DownloadController({
    read: async () => {
      calls.push("read");
      return gate.promise;
    },
    prepare: async () => {
      calls.push("prepare");
      return plan();
    },
  });
  const first = controller.read(),
    second = controller.read(),
    prepare = controller.prepare(context, "123");
  assert.equal(first, second);
  assert.deepEqual(calls, ["read"]);
  gate.resolve(snapshot([], 3));
  await Promise.all([first, second, prepare]);
  assert.deepEqual(calls, ["read", "prepare"]);
  assert.equal(controller.getState().snapshot.revision, 3);
  controller.dispose();
});

test("corrupt queue reads retain the prior task list and do not enable an empty replacement", async () => {
  let fail = false;
  const controller = new DownloadController({
    read: async () => {
      if (fail) throw new DownloadError("DOWNLOAD_RESPONSE_INVALID");
      return snapshot([task()], 5);
    },
  });
  await controller.read();
  fail = true;
  await controller.read();
  assert.equal(controller.getState().snapshot.tasks.length, 1);
  assert.equal(controller.getState().snapshot.revision, 5);
  assert.ok(controller.getState().error);
  controller.dispose();
});

test("control results from a disposed runtime cannot overwrite a newly restored queue", async () => {
  const gate = pending();
  const controller = new DownloadController({
    read: async () => snapshot([task({ title: "新读取" })], 3),
    control: async () => gate.promise,
  });
  const action = controller.control(context.scope, task(), "resume");
  await Promise.resolve();
  controller.dispose();
  await controller.read();
  gate.resolve(
    snapshot(
      [
        task({
          title: "旧请求",
          phase: "error",
          allowedActions: ["retry"],
          errorCode: "SOURCE_TIMEOUT",
        }),
      ],
      4,
    ),
  );
  await action;
  assert.equal(controller.getState().snapshot.tasks[0].title, "新读取");
  controller.dispose();
});

test("confirmation failure drops the plan without hidden retries or synthetic completion", async () => {
  let calls = 0;
  const controller = new DownloadController({
    prepare: async () => plan(),
    confirm: async () => {
      calls++;
      throw new DownloadError("DOWNLOAD_PLAN_STALE");
    },
  });
  await controller.prepare(context, "123");
  assert.equal(await controller.confirm(context), false);
  assert.equal(controller.getState().snapshot.tasks.length, 0);
  assert.equal(controller.getState().plan, null);
  assert.equal(calls, 1);
  assert.ok(controller.getState().error);
  controller.dispose();
});

const completedTask = (overrides = {}) =>
  task({
    phase: "downloaded",
    filesDone: 3,
    filesTotal: 3,
    allowedActions: [],
    libraryEntryId: entryId,
    localFiles: "present",
    ...overrides,
  });

test("historical completion requires a separate explicit local-file observation", () => {
  for (const localFiles of [
    "present",
    "missing",
    "incomplete",
    "unavailable",
  ]) {
    const value = validateDownloadSnapshot(
      snapshot([completedTask({ localFiles })]),
    ).tasks[0];
    assert.equal(value.phase, "downloaded");
    assert.equal(value.filesDone, 3);
    assert.equal(value.libraryEntryId, entryId);
    assert.equal(isDownloadPresent(value), localFiles === "present");
    assert.equal(
      filterDownloadTasks([value], "downloaded").length,
      localFiles === "present" ? 1 : 0,
    );
    assert.equal(
      filterDownloadTasks([value], "error").length,
      localFiles === "present" ? 0 : 1,
    );
    assert.equal(filterDownloadTasks([value], "active").length, 0);
  }
  for (const localFiles of [null, undefined, "exists", true])
    assert.throws(
      () => validateDownloadSnapshot(snapshot([completedTask({ localFiles })])),
      DownloadError,
    );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task({ localFiles: "present" })])),
    DownloadError,
  );
  assert.equal(
    downloadTaskLabel(completedTask({ localFiles: "missing" })),
    "文件已移除",
  );
  assert.equal(
    downloadTaskLabel(completedTask({ localFiles: "incomplete" })),
    "文件已变化",
  );
  assert.equal(
    downloadTaskLabel(completedTask({ localFiles: "unavailable" })),
    "目录不可用",
  );
});

test("download reads explicitly distinguish file rechecks from progress-only reads", async () => {
  const calls = [];
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return snapshot();
    },
  });
  await adapter.read();
  await adapter.read(false);
  await adapter.read(true);
  assert.deepEqual(
    calls,
    [true, false, true].map((recheckFiles) => ({
      command: "jm_download_read",
      args: { recheckFiles },
    })),
  );
  await assert.rejects(adapter.read("false"), DownloadError);
  assert.equal(calls.length, 3);
});

test("a same-revision recheck changes file filters without rewriting completion history", async () => {
  let localFiles = "present";
  const controller = new DownloadController({
    read: async () => snapshot([completedTask({ localFiles })], 9),
  });
  await controller.read();
  assert.equal(
    filterDownloadTasks(controller.getState().snapshot.tasks, "downloaded")
      .length,
    1,
  );
  localFiles = "missing";
  await controller.read();
  assert.equal(controller.getState().snapshot.revision, 9);
  assert.equal(
    filterDownloadTasks(controller.getState().snapshot.tasks, "downloaded")
      .length,
    0,
  );
  assert.equal(
    filterDownloadTasks(controller.getState().snapshot.tasks, "error").length,
    1,
  );
  assert.equal(controller.getState().snapshot.tasks[0].filesDone, 3);
  assert.equal(controller.getState().snapshot.tasks[0].phase, "downloaded");
  controller.dispose();
});

test("timed progress polling never rechecks completed files", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const calls = [];
  const controller = new DownloadController({
    read: async (recheckFiles) => {
      calls.push(recheckFiles);
      return snapshot([
        task({ phase: "downloading", allowedActions: ["pause"] }),
      ]);
    },
  });
  await controller.read();
  t.mock.timers.tick(1000);
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(calls, [true, false]);
  t.mock.timers.tick(1000);
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(calls, [true, false, false]);
  controller.dispose();
});

test("an explicit recheck arriving during a progress read is coalesced and not lost", async () => {
  const gate = pending(),
    calls = [];
  const controller = new DownloadController({
    read: async (recheckFiles) => {
      calls.push(recheckFiles);
      return recheckFiles
        ? snapshot([completedTask({ localFiles: "missing" })], 4)
        : gate.promise;
    },
  });
  const progress = controller.read(false);
  const firstRecheck = controller.read(true),
    duplicateRecheck = controller.read(true);
  assert.equal(progress, firstRecheck);
  assert.equal(firstRecheck, duplicateRecheck);
  assert.deepEqual(calls, [false]);
  gate.resolve(snapshot([completedTask()], 4));
  await firstRecheck;
  assert.deepEqual(calls, [false, true]);
  assert.equal(controller.getState().snapshot.tasks[0].localFiles, "missing");
  controller.dispose();
});

const picaId = "0123456789abcdef01234567";
const picaContext = {
  ...context,
  scope: { source: "Pica", sessionId: "synthetic-Pica" },
};
const picaPlan = (overrides = {}) =>
  plan({ source: "Pica", workId: picaId, ...overrides });
const picaTask = (overrides = {}) =>
  task({ source: "Pica", workId: picaId, ...overrides });

test("Pica identities are lowercase 24-hex strings and never accept JM numeric identities", () => {
  assert.equal(validateDownloadPlan(picaPlan()).workId, picaId);
  assert.equal(
    validateDownloadSnapshot(snapshot([picaTask()])).tasks[0].source,
    "Pica",
  );
  for (const workId of [
    "123",
    picaId.toUpperCase(),
    picaId + "f",
    "../" + picaId,
  ]) {
    assert.throws(
      () => validateDownloadPlan(picaPlan({ workId })),
      DownloadError,
    );
    assert.throws(
      () => validateDownloadSnapshot(snapshot([picaTask({ workId })])),
      DownloadError,
    );
  }
  assert.throws(
    () => validateDownloadPlan(plan({ workId: picaId })),
    DownloadError,
  );
  assert.throws(
    () => validateDownloadSnapshot(snapshot([task({ source: "Other" })])),
    DownloadError,
  );
});

test("Pica preparation keeps the existing IPC shape and rejects a valid plan from another source", async () => {
  const calls = [];
  let response = picaPlan();
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async (command, args) => {
      calls.push({ command, args });
      return response;
    },
  });
  const input = `https://picaapi.picacomic.com/comics/${picaId}`;
  await adapter.prepare(
    {
      ...picaContext,
      path: "never sent",
      scope: { ...picaContext.scope, token: "never sent" },
    },
    input,
  );
  assert.deepEqual(calls, [
    { command: "jm_download_prepare", args: { ...picaContext, input } },
  ]);
  response = plan();
  await assert.rejects(adapter.prepare(picaContext, picaId), DownloadError);
});

test("download scopes never fall back to the other source account", () => {
  assert.match(downloadErrorMessage("PICA_TOKEN_MISSING"), /对应来源的账号/);
  assert.match(
    downloadErrorMessage("DOWNLOAD_SOURCE_INCOMPLETE"),
    /来源目录暂未读取完整/,
  );
  assert.match(
    downloadErrorMessage("DOWNLOAD_SOURCE_MISMATCH"),
    /任务来源与当前账号不一致/,
  );
  const accounts = [
    { source: "JM", state: "connected", sessionId: "JM-session" },
    { source: "Pica", state: "expired", sessionId: "old-Pica-session" },
  ];
  assert.deepEqual(getDownloadScope(accounts, "JM"), {
    source: "JM",
    sessionId: "JM-session",
  });
  assert.equal(getDownloadScope(accounts, "Pica"), null);
  assert.equal(getDownloadScope(accounts.slice(0, 1), "Pica"), null);
  accounts[1].state = "connected";
  assert.deepEqual(getDownloadScope(accounts, "Pica"), {
    source: "Pica",
    sessionId: "old-Pica-session",
  });
});

test("mixed-source controls bind each task to its own source and permit only pause without a session", async () => {
  const calls = [];
  const controller = new DownloadController({
    control: async (...args) => {
      calls.push(args);
      return snapshot([
        picaTask({ phase: "paused", allowedActions: ["resume"] }),
      ]);
    },
  });
  await controller.control(context.scope, picaTask(), "resume");
  await controller.control(null, picaTask(), "resume");
  assert.equal(calls.length, 0);
  assert.equal(canControlDownload(picaTask(), "resume", context.scope), false);
  await controller.control(picaContext.scope, picaTask(), "resume");
  const running = picaTask({ phase: "downloading", allowedActions: ["pause"] });
  await controller.control(context.scope, running, "pause");
  assert.equal(calls.length, 1);
  await controller.control(null, running, "pause");
  assert.deepEqual(
    calls.map((call) => [call[0], call[3]]),
    [
      [picaContext.scope, "resume"],
      [{ source: "Pica", sessionId: "" }, "pause"],
    ],
  );
  controller.dispose();
  const adapterCalls = [];
  const adapter = createDownloadAdapter({
    native: true,
    invoke: async (command, args) => {
      adapterCalls.push({ command, args });
      return snapshot([picaTask()]);
    },
  });
  await adapter.control(
    { source: "Pica", sessionId: "" },
    running.id,
    1,
    "pause",
  );
  await assert.rejects(
    adapter.control({ source: "Pica", sessionId: "" }, running.id, 1, "retry"),
    DownloadError,
  );
  assert.equal(adapterCalls.length, 1);
  assert.deepEqual(adapterCalls[0].args.scope, {
    source: "Pica",
    sessionId: "",
  });
});

test("changing source invalidates a Pica confirmation and a wrong-source result never completes it", async () => {
  let confirms = 0;
  const controller = new DownloadController({
    prepare: async () => picaPlan(),
    confirm: async () => {
      confirms++;
      return snapshot([task()]);
    },
  });
  await controller.prepare(picaContext, picaId);
  assert.equal(await controller.confirm(context), false);
  assert.equal(confirms, 0);
  await controller.prepare(picaContext, picaId);
  assert.equal(await controller.confirm(picaContext), false);
  assert.equal(confirms, 1);
  assert.equal(controller.getState().snapshot.tasks.length, 0);
  assert.equal(controller.getState().plan, null);
  assert.ok(controller.getState().error);
  controller.dispose();
});
