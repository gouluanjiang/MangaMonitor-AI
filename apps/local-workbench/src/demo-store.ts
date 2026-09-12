import { works } from "./catalog.ts";
import type { DemoState, DemoTask, TaskStage, Work } from "./types.ts";

// This module only describes a browser demonstration. A simulated completion
// creates no download, archive, library entry, approval, or cloud publication.
const readyWorkIds = new Set(
  works.filter((work) => work.status === "ready").map((work) => work.id),
);
const demoError = "模拟连接中断，进度已保留。";
const taskStages = new Set<TaskStage>([
  "queued",
  "downloading",
  "verifying",
  "packing",
  "importing",
  "sync_pending",
  "completed",
  "error",
]);
const localStages = new Set<TaskStage>([
  "queued",
  "downloading",
  "verifying",
  "packing",
  "importing",
]);
const fullProgressStages = new Set<TaskStage>([
  "verifying",
  "packing",
  "importing",
  "sync_pending",
  "completed",
]);

export function initialDemoState(): DemoState {
  return {
    version: 1,
    tasks: [
      {
        id: "demo-sea",
        workId: "sea",
        stage: "downloading",
        progress: 35,
        paused: false,
        error: null,
      },
      {
        id: "demo-moon",
        workId: "moon",
        stage: "sync_pending",
        progress: 100,
        paused: false,
        error: null,
      },
      {
        id: "demo-train",
        workId: "train",
        stage: "error",
        progress: 27,
        paused: false,
        error: demoError,
      },
    ],
    online: true,
    paused: false,
    closed: false,
  };
}

export function enqueueWorks(
  state: DemoState,
  selectedWorks: Work[],
): DemoState {
  if (state.closed) return state;
  const existing = new Set(state.tasks.map((task) => task.workId));
  const additions: DemoTask[] = [];
  for (const work of selectedWorks) {
    // Both the selected item and the fixed demonstration catalog must permit
    // enqueueing. A forged "ready" value cannot turn a review/owned item ready.
    if (
      work.status !== "ready" ||
      !readyWorkIds.has(work.id) ||
      existing.has(work.id)
    )
      continue;
    existing.add(work.id);
    additions.push({
      id: `demo-${work.id}`,
      workId: work.id,
      stage: "queued",
      progress: 0,
      paused: false,
      error: null,
    });
  }
  return additions.length === 0
    ? state
    : { ...state, tasks: [...state.tasks, ...additions] };
}

function advanceLocalTask(task: DemoTask): DemoTask {
  switch (task.stage) {
    case "queued":
      return { ...task, stage: "downloading" };
    case "downloading": {
      const progress = Math.min(100, task.progress + 8);
      return {
        ...task,
        progress,
        stage: progress === 100 ? "verifying" : "downloading",
      };
    }
    case "verifying":
      return { ...task, stage: "packing" };
    case "packing":
      return { ...task, stage: "importing" };
    case "importing":
      return { ...task, stage: "sync_pending" };
    default:
      return task;
  }
}

export function tickDemo(state: DemoState): DemoState {
  if (state.closed || state.paused) return state;
  // Several tasks can retain a downloading stage after explicit pause/resume.
  // Queue order picks exactly one local task; the others wait with intact progress.
  const localIndex = state.tasks.findIndex(
    (task) => !task.paused && localStages.has(task.stage),
  );
  const syncIndex = state.online
    ? state.tasks.findIndex(
        (task) => !task.paused && task.stage === "sync_pending",
      )
    : -1;
  if (localIndex === -1 && syncIndex === -1) return state;
  return {
    ...state,
    tasks: state.tasks.map<DemoTask>((task, index) => {
      if (index === localIndex) return advanceLocalTask(task);
      // Select from the original state so a new import visibly waits for its
      // next simulated acknowledgement, even when this demo is "online".
      if (index === syncIndex) return { ...task, stage: "completed" };
      return task;
    }),
  };
}

export function toggleTaskPause(state: DemoState, id: string): DemoState {
  if (state.closed) return state;
  const task = state.tasks.find((item) => item.id === id);
  if (!task || task.stage === "completed" || task.stage === "error")
    return state;
  return {
    ...state,
    tasks: state.tasks.map<DemoTask>((item) =>
      item.id === id ? { ...item, paused: !item.paused } : item,
    ),
  };
}

export function retryDemoTask(state: DemoState, id: string): DemoState {
  if (state.closed) return state;
  const task = state.tasks.find((item) => item.id === id);
  if (!task || task.stage !== "error") return state;
  return {
    ...state,
    tasks: state.tasks.map<DemoTask>((item) =>
      item.id === id ? { ...item, stage: "downloading", error: null } : item,
    ),
  };
}

export function setDemoPaused(state: DemoState, paused: boolean): DemoState {
  return state.paused === paused ? state : { ...state, paused };
}

export function setDemoOnline(state: DemoState, online: boolean): DemoState {
  return state.online === online ? state : { ...state, online };
}

export function closeDemo(state: DemoState): DemoState {
  return state.closed ? state : { ...state, closed: true };
}

export function reopenDemo(state: DemoState): DemoState {
  return state.closed ? { ...state, closed: false } : state;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  return (
    Object.keys(value).length === keys.length &&
    keys.every((key) => Object.prototype.hasOwnProperty.call(value, key))
  );
}

function restoreTask(value: unknown): DemoTask | null {
  if (
    !isObject(value) ||
    !exactKeys(value, ["id", "workId", "stage", "progress", "paused", "error"])
  )
    return null;
  if (
    typeof value.workId !== "string" ||
    !readyWorkIds.has(value.workId) ||
    value.id !== `demo-${value.workId}` ||
    typeof value.stage !== "string" ||
    !taskStages.has(value.stage as TaskStage) ||
    typeof value.progress !== "number" ||
    !Number.isInteger(value.progress) ||
    value.progress < 0 ||
    value.progress > 100 ||
    typeof value.paused !== "boolean"
  )
    return null;
  const stage = value.stage as TaskStage;
  if (stage === "queued" && value.progress !== 0) return null;
  if (fullProgressStages.has(stage) && value.progress !== 100) return null;
  if (stage === "error" ? value.error !== demoError : value.error !== null)
    return null;
  if ((stage === "completed" || stage === "error") && value.paused) return null;
  return {
    id: `demo-${value.workId}`,
    workId: value.workId,
    stage,
    progress: value.progress,
    paused: value.paused,
    error: stage === "error" ? demoError : null,
  };
}

export function restoreDemoState(raw: string | null): DemoState {
  if (typeof raw !== "string" || raw.length > 16_384) return initialDemoState();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return initialDemoState();
  }
  if (
    !isObject(value) ||
    !exactKeys(value, ["version", "tasks", "online", "paused", "closed"]) ||
    value.version !== 1 ||
    !Array.isArray(value.tasks) ||
    value.tasks.length > readyWorkIds.size ||
    typeof value.online !== "boolean" ||
    typeof value.paused !== "boolean" ||
    typeof value.closed !== "boolean"
  )
    return initialDemoState();
  const tasks: DemoTask[] = [];
  const seen = new Set<string>();
  for (const item of value.tasks) {
    const task = restoreTask(item);
    if (!task || seen.has(task.workId)) return initialDemoState();
    seen.add(task.workId);
    tasks.push(task);
  }
  return {
    version: 1,
    tasks,
    online: value.online,
    paused: value.paused,
    closed: value.closed,
  };
}
