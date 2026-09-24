import { CoverScheduler } from "./cover-scheduler.ts";
import type { CoverPriority } from "./cover-scheduler.ts";

export const SOURCE_COVER_CONCURRENCY = 4;
export const SOURCE_COVER_MAX_WAITING = 64;
const queue = new CoverScheduler<string | null>(
  SOURCE_COVER_CONCURRENCY,
  SOURCE_COVER_MAX_WAITING,
);
export function queueCover(
  run: () => Promise<string | null>,
  priority: CoverPriority = "visible",
) {
  return queue.enqueue(run, priority);
}
