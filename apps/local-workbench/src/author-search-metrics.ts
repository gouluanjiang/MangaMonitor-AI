/** Durations overlap across sources; do not add them to obtain wall time. */
export interface AuthorSearchMetrics {
  startedAt: number;
  firstRecordsMs: number | null;
  firstVisibleMs: number | null;
  completedMs: number | null;
  policyMs: number;
  localCatalogMs: number;
  queueMs: number;
  sourceOperationMs: number;
  nativeQueueMs: number;
  localCommitMs: number;
  transportOtherMs: number;
  renderMs: number;
  renderedBatches: number;
  failedRequests: number;
  requestsWithoutNativeTiming: number;
}
export const newAuthorSearchMetrics = (): AuthorSearchMetrics => ({
  startedAt: Date.now(),
  firstRecordsMs: null,
  firstVisibleMs: null,
  completedMs: null,
  policyMs: 0,
  localCatalogMs: 0,
  queueMs: 0,
  sourceOperationMs: 0,
  nativeQueueMs: 0,
  localCommitMs: 0,
  transportOtherMs: 0,
  renderMs: 0,
  renderedBatches: 0,
  failedRequests: 0,
  requestsWithoutNativeTiming: 0,
});
