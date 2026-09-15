import type { Source, SourceScope, SourceWork } from "./source-types.ts";
export type ScanPhase =
  "checking" | "complete" | "partial" | "cancelled" | "error";
export interface DiscoveryRun {
  id: string;
  phase: ScanPhase;
  currentAuthor: string | null;
  currentSource: Source | null;
  currentPage: number;
  requestsUsed: number;
  completedScopes: number;
  totalScopes: number;
  errorCode: string | null;
}
export interface DiscoverySnapshot {
  scopes: SourceScope[];
  revision: number;
  run: DiscoveryRun | null;
  authors: {
    author: string;
    source: Source;
    state: ScanPhase | "idle";
    lastAttemptAt: number | null;
    lastCompleteAt: number | null;
    observedCount: number;
    pagesRead: number;
    errorCode: string | null;
  }[];
  records: {
    work: SourceWork;
    matchedAuthors: string[];
    authorVerified: boolean;
    observedAt: number;
    scanId: string;
  }[];
}
export interface CompletionAdapter {
  read(scopes: SourceScope[]): Promise<DiscoverySnapshot>;
  start(scopes: SourceScope[], authors: string[]): Promise<DiscoverySnapshot>;
  cancel(runId: string): Promise<void>;
}
