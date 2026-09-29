import test from "node:test";
import assert from "node:assert/strict";
import { works } from "../src/catalog.ts";
import {
  initialDemoState,
  enqueueWorks,
  tickDemo,
  toggleTaskPause,
  retryDemoTask,
  setDemoPaused,
  setDemoOnline,
  closeDemo,
  reopenDemo,
  restoreDemoState,
} from "../src/demo-store.ts";

const work = (id) => {
  const selected = works.find((item) => item.id === id);
  assert.ok(selected, `expected demonstration work ${id}`);
  return selected;
};
const findTask = (state, workId) => {
  const task = state.tasks.find((item) => item.workId === workId);
  assert.ok(task, `expected task for ${workId}`);
  return task;
};
const emptyState = () => ({ ...initialDemoState(), tasks: [] });
const progressUntil = (state, predicate, limit = 100) => {
  for (let step = 0; step < limit && !predicate(state); step += 1)
    state = tickDemo(state);
  assert.ok(predicate(state), "demonstration did not reach its expected state");
  return state;
};
const freeze = (value) => {
  if (value && typeof value === "object") {
    for (const nested of Object.values(value)) freeze(nested);
    Object.freeze(value);
  }
  return value;
};

test("initial tasks show download, imported waiting for sync, and a retryable interruption", () => {
  const state = initialDemoState();
  assert.deepEqual(
    state.tasks.map(({ workId, stage, progress }) => ({
      workId,
      stage,
      progress,
    })),
    [
      { workId: "sea", stage: "downloading", progress: 35 },
      { workId: "moon", stage: "sync_pending", progress: 100 },
      { workId: "train", stage: "error", progress: 27 },
    ],
  );
  assert.ok(state.tasks.every((task) => task.id === `demo-${task.workId}`));
  const next = tickDemo(state);
  assert.equal(findTask(next, "sea").progress, 43);
  assert.equal(
    findTask(next, "moon").stage,
    "completed",
    "sync acknowledgement is independent of the local download",
  );
  state.tasks[0].progress = 1;
  assert.equal(
    initialDemoState().tasks[0].progress,
    35,
    "defaults must not share mutable tasks",
  );
});

test("enqueue accepts known ready work once and refuses owned, review and forged items", () => {
  const state = initialDemoState();
  const result = enqueueWorks(state, [
    work("rain"),
    work("rain"),
    work("sea"),
    work("summer"),
    work("echo"),
    { ...work("summer"), status: "ready" },
    { ...work("echo"), status: "ready" },
    { ...work("rain"), id: "unknown-work" },
    { ...work("flight"), status: "review" },
  ]);
  assert.deepEqual(
    result.tasks.map((task) => task.workId),
    ["sea", "moon", "train", "rain"],
  );
  assert.deepEqual(
    state,
    initialDemoState(),
    "enqueue must not mutate its input",
  );
  assert.deepEqual(enqueueWorks(result, [work("rain")]), result);
  assert.deepEqual(
    enqueueWorks(closeDemo(result), [work("bookshop")]),
    closeDemo(result),
  );
});

test("each local stage is observable and confirmed completion cannot enqueue twice", () => {
  let state = enqueueWorks(emptyState(), [work("rain")]);
  const seen = new Set([findTask(state, "rain").stage]);
  state = progressUntil(state, (current) => {
    seen.add(findTask(current, "rain").stage);
    return findTask(current, "rain").stage === "completed";
  });
  for (const stage of [
    "queued",
    "downloading",
    "verifying",
    "packing",
    "importing",
    "sync_pending",
    "completed",
  ]) {
    assert.ok(
      seen.has(stage),
      `${stage} must be visible before the next stage`,
    );
  }
  assert.equal(findTask(state, "rain").progress, 100);
  assert.deepEqual(enqueueWorks(state, [work("rain")]), state);
});

test("offline imports wait for sync while other local work continues without re-downloading", () => {
  let state = setDemoOnline(
    enqueueWorks(emptyState(), [work("rain"), work("flight")]),
    false,
  );
  state = progressUntil(state, (current) =>
    current.tasks.every((task) => task.stage === "sync_pending"),
  );
  const imported = structuredClone(state);
  for (let step = 0; step < 5; step += 1) state = tickDemo(state);
  assert.deepEqual(state, imported);
  state = restoreDemoState(JSON.stringify(state));
  state = setDemoOnline(state, true);
  state = progressUntil(
    state,
    (current) => {
      assert.ok(
        current.tasks.every((task) =>
          ["sync_pending", "completed"].includes(task.stage),
        ),
      );
      assert.ok(current.tasks.every((task) => task.progress === 100));
      return current.tasks.every((task) => task.stage === "completed");
    },
    4,
  );
});

test("one local task advances per tick; task pause and resume preserve explicit choices", () => {
  let state = enqueueWorks(initialDemoState(), [work("rain")]);
  state = toggleTaskPause(state, "demo-sea");
  state = tickDemo(state);
  assert.equal(findTask(state, "sea").progress, 35);
  assert.equal(findTask(state, "rain").stage, "downloading");
  state = toggleTaskPause(state, "demo-sea");
  assert.equal(
    findTask(state, "rain").paused,
    false,
    "resuming sea must not auto-pause rain",
  );
  const prior = structuredClone(state);
  state = tickDemo(state);
  assert.equal(findTask(state, "sea").progress, 43);
  assert.deepEqual(
    findTask(state, "rain"),
    findTask(prior, "rain"),
    "later progress waits for its turn",
  );
  assert.deepEqual(
    tickDemo(setDemoPaused(state, true)),
    setDemoPaused(state, true),
  );
});

test("closing stops execution and additions; restoring and reopening keeps the confirmed queue", () => {
  let state = enqueueWorks(initialDemoState(), [work("rain")]);
  state = toggleTaskPause(state, "demo-sea");
  const closed = closeDemo(state);
  assert.deepEqual(tickDemo(closed), closed);
  assert.deepEqual(retryDemoTask(closed, "demo-train"), closed);
  assert.deepEqual(toggleTaskPause(closed, "demo-sea"), closed);
  assert.deepEqual(enqueueWorks(closed, [work("flight")]), closed);
  const restored = restoreDemoState(JSON.stringify(closed));
  assert.deepEqual(restored, closed);
  assert.deepEqual(reopenDemo(restored), state);
  const manuallyPaused = closeDemo(setDemoPaused(state, true));
  assert.equal(
    reopenDemo(manuallyPaused).paused,
    true,
    "reopening does not override a global pause",
  );
});

test("a failed task does not block the queue and only errors can retry from retained progress", () => {
  let state = {
    ...initialDemoState(),
    tasks: [findTask(initialDemoState(), "train")],
  };
  state = enqueueWorks(state, [work("rain")]);
  state = tickDemo(state);
  assert.equal(findTask(state, "train").stage, "error");
  assert.equal(findTask(state, "rain").stage, "downloading");
  const retried = retryDemoTask(state, "demo-train");
  assert.equal(findTask(retried, "train").stage, "downloading");
  assert.equal(findTask(retried, "train").progress, 27);
  assert.equal(findTask(retried, "train").error, null);
  assert.equal(findTask(retried, "rain").paused, false);
  assert.equal(findTask(tickDemo(retried), "train").progress, 35);
  assert.deepEqual(retryDemoTask(retried, "demo-rain"), retried);
  assert.deepEqual(retryDemoTask(retried, "missing"), retried);
});

test("persistence rejects corruption, future formats, unknown ids and impossible task states", () => {
  const invalid = [null, "", "{broken", "null", "[]", "1", " ".repeat(16_385)];
  const changes = [
    (state) => {
      state.version = 2;
    },
    (state) => {
      state.online = "true";
    },
    (state) => {
      state.closed = 0;
    },
    (state) => {
      state.paused = null;
    },
    (state) => {
      state.credentials = "not allowed";
    },
    (state) => {
      state.tasks.push({ ...state.tasks[0] });
    },
    (state) => {
      state.tasks[0].id = "real-executor-task";
    },
    (state) => {
      state.tasks[0].workId = "not-in-catalog";
    },
    (state) => {
      state.tasks[0].workId = "summer";
      state.tasks[0].id = "demo-summer";
    },
    (state) => {
      state.tasks[0].workId = "echo";
      state.tasks[0].id = "demo-echo";
    },
    (state) => {
      state.tasks[0].stage = "download-authorized";
    },
    (state) => {
      state.tasks[0].progress = -1;
    },
    (state) => {
      state.tasks[0].progress = 101;
    },
    (state) => {
      state.tasks[0].progress = 35.5;
    },
    (state) => {
      state.tasks[0].stage = "queued";
    },
    (state) => {
      state.tasks[0].stage = "completed";
    },
    (state) => {
      state.tasks[0].error = "unexpected message";
    },
    (state) => {
      state.tasks[1].progress = 99;
    },
    (state) => {
      state.tasks[2].error = null;
    },
    (state) => {
      state.tasks[2].error = "<script>not trusted</script>";
    },
    (state) => {
      state.tasks[2].paused = true;
    },
    (state) => {
      state.tasks[0].destination = "/not-a-real-library";
    },
  ];
  for (const mutate of changes) {
    const state = initialDemoState();
    mutate(state);
    invalid.push(JSON.stringify(state));
  }
  for (const raw of invalid)
    assert.deepEqual(restoreDemoState(raw), initialDemoState(), String(raw));
  assert.deepEqual(
    restoreDemoState(JSON.stringify(emptyState())),
    emptyState(),
    "an empty valid queue is preserved",
  );
});

test("commands and serialization leave frozen inputs untouched and all reached states restore", () => {
  const state = freeze(initialDemoState());
  const transitions = [
    enqueueWorks(state, [work("rain")]),
    tickDemo(state),
    toggleTaskPause(state, "demo-sea"),
    retryDemoTask(state, "demo-train"),
    setDemoPaused(state, true),
    setDemoOnline(state, false),
    closeDemo(state),
    reopenDemo(state),
  ];
  for (const result of transitions)
    assert.deepEqual(restoreDemoState(JSON.stringify(result)), result);
  let current = enqueueWorks(emptyState(), [work("rain")]);
  for (let step = 0; step < 25; step += 1) {
    current = tickDemo(freeze(current));
    assert.deepEqual(restoreDemoState(JSON.stringify(current)), current);
  }
});
