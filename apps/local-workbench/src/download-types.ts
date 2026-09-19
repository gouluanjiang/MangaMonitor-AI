export type DownloadSource = "JM" | "Pica";
export const downloadSelectionLimit = 500;
export const downloadPreparationChunk = 20;
export interface DownloadScope {
  source: DownloadSource;
  sessionId: string;
}
export interface DownloadContext {
  scope: DownloadScope;
  rootId: string;
  generation: number;
}
export type DownloadContexts = Record<DownloadSource, DownloadContext | null>;
export interface DownloadPlan {
  planId: string;
  revision: number;
  source: DownloadSource;
  workId: string;
  title: string;
  authors: string[];
  destinationDisplay: string;
  rootId: string;
  generation: number;
}
export interface DownloadBatchPlan {
  batchId: string | null;
  plans: DownloadPlan[];
  issues: { input: string; errorCode: string }[];
}
export interface DownloadSelectionPlan extends DownloadBatchPlan {
  batchIds: string[];
}
export interface DownloadSelectionInput {
  source: DownloadSource;
  input: string;
}
export interface DownloadTaskRevision {
  taskId: string;
  expectedRevision: number;
}
export type DownloadAction = "pause" | "resume" | "retry";
export type DownloadLocalFiles =
  "present" | "missing" | "incomplete" | "unavailable";
export type DownloadPhase =
  | "queued"
  | "downloading"
  | "verifying"
  | "saving"
  | "paused"
  | "error"
  | "downloaded";
export interface DownloadTask {
  id: string;
  revision: number;
  source: DownloadSource;
  workId: string;
  title: string;
  phase: DownloadPhase;
  filesDone: number;
  filesTotal: number | null;
  bytesDone: number;
  errorCode: string | null;
  allowedActions: DownloadAction[];
  libraryEntryId: string | null;
  localFiles: DownloadLocalFiles | null;
  updatedAt: number;
  destinationDisplay: string;
}
export interface DownloadSnapshot {
  revision: number;
  tasks: DownloadTask[];
}
export interface DownloadInventorySnapshot {
  revision: number;
  libraryRevision: number;
  rootId: string | null;
  items: {
    source: DownloadSource;
    workId: string;
    libraryEntryId: string;
    localFiles: DownloadLocalFiles;
  }[];
}
export const emptyDownloadInventory = (): DownloadInventorySnapshot => ({
  revision: 0,
  libraryRevision: 0,
  rootId: null,
  items: [],
});
export interface DownloadAdapter {
  inventory(): Promise<DownloadInventorySnapshot>;
  read(recheckFiles?: boolean): Promise<DownloadSnapshot>;
  prepare(context: DownloadContext, input: string): Promise<DownloadPlan>;
  confirm(planId: string, expectedRevision: number): Promise<DownloadSnapshot>;
  prepareBatch(
    context: DownloadContext,
    inputs: string[],
    retainedBatchIds?: string[],
  ): Promise<DownloadBatchPlan>;
  confirmBatch(batchId: string): Promise<DownloadSnapshot>;
  confirmSelection(batchIds: string[]): Promise<DownloadSnapshot>;
  cancelBatch(): Promise<void>;
  pauseAll(): Promise<DownloadSnapshot>;
  resumeMany(
    scope: DownloadScope,
    tasks: DownloadTaskRevision[],
  ): Promise<DownloadSnapshot>;
  removeHistory(tasks: DownloadTaskRevision[]): Promise<DownloadSnapshot>;
  control(
    scope: DownloadScope,
    taskId: string,
    expectedRevision: number,
    action: DownloadAction,
  ): Promise<DownloadSnapshot>;
}
export const emptyDownloads = (): DownloadSnapshot => ({
  revision: 0,
  tasks: [],
});
