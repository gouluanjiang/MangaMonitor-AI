export type DownloadSource = "JM" | "Pica";
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
export interface DownloadAdapter {
  read(recheckFiles?: boolean): Promise<DownloadSnapshot>;
  prepare(context: DownloadContext, input: string): Promise<DownloadPlan>;
  confirm(planId: string, expectedRevision: number): Promise<DownloadSnapshot>;
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
