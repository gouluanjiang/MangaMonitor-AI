import type { DownloadSnapshot, DownloadTask } from "./download-types.ts";
import { isContentHidden } from "./content-filter.ts";

export interface DownloadAttentionNotice {
  count: number;
  taskIds: string[];
  message: string;
}
const needsAttention = (task: DownloadTask) =>
  !isContentHidden(task) &&
  (task.phase === "error" ||
    (task.phase === "downloaded" && task.localFiles !== "present"));

/** Prime with the initial snapshot: historical failures are not new events.
 * A task can notify again only after it left its attention state (e.g. retry). */
export class DownloadAttentionTracker {
  private previous: Map<string, boolean> | null = null;
  observe(snapshot: DownloadSnapshot): string[] {
    const current = new Map(
      snapshot.tasks.map((task) => [task.id, needsAttention(task)]),
    );
    const changed = this.previous
      ? [...current]
          .filter(
            ([id, attention]) => attention && this.previous!.get(id) !== true,
          )
          .map(([id]) => id)
      : [];
    this.previous = current;
    return changed;
  }
}

/** Fixed windows, not a debounce: a long batch still reports failures promptly. */
export class DownloadAttentionAggregator {
  private ids = new Set<string>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  constructor(
    privateCallback: (notice: DownloadAttentionNotice) => void,
    delay = 1200,
  ) {
    this.callback = privateCallback;
    this.delay = delay;
  }
  private callback: (notice: DownloadAttentionNotice) => void;
  private delay: number;
  push(ids: string[]) {
    for (const id of ids) this.ids.add(id);
    if (!this.ids.size || this.timer !== undefined) return;
    this.timer = setTimeout(() => this.flush(), this.delay);
  }
  flush() {
    clearTimeout(this.timer);
    this.timer = undefined;
    if (!this.ids.size) return;
    const taskIds = [...this.ids];
    this.ids.clear();
    this.callback({
      count: taskIds.length,
      taskIds,
      message: `${taskIds.length} 个下载任务需要处理`,
    });
  }
  dispose() {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.ids.clear();
  }
}
