import type {
  Source,
  SourceScope,
  SourceWork,
  SourceItemIssue,
} from "./source-types.ts";
export type ScanPhase =
  "checking" | "complete" | "partial" | "cancelled" | "error";
export type DiscoveryMode = "incremental" | "full";
export const discoveryRecordLimit = 500000;
export interface DiscoveryBaseline {
  queryVersion: number;
  headIds: string[];
  total: number;
  establishedAt: number;
}
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
  storageWarningCode?: string | null;
  mode?: DiscoveryMode;
  currentStrategy?: DiscoveryMode | null;
}
export interface DiscoverySnapshot {
  scopes: SourceScope[];
  revision: number;
  run: DiscoveryRun | null;
  otherRecordCount?: number;
  includesOther?: boolean;
  authors: {
    author: string;
    source: Source;
    state: ScanPhase | "idle";
    lastAttemptAt: number | null;
    lastCompleteAt: number | null;
    lastCheckedAt?: number | null;
    lastCheckMode?: DiscoveryMode | null;
    baseline?: DiscoveryBaseline | null;
    observedCount: number;
    pagesRead: number;
    errorCode: string | null;
    issueCount?: number;
    issueSamples?: SourceItemIssue[];
    pagesComplete?: boolean;
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
  read(
    scopes: SourceScope[],
    includeOther?: boolean,
  ): Promise<DiscoverySnapshot>;
  progress(scopes: SourceScope[]): Promise<DiscoveryProgress>;
  start(
    scopes: SourceScope[],
    authors: string[],
    mode?: DiscoveryMode,
  ): Promise<DiscoverySnapshot>;
  startUnfinished(
    scopes: SourceScope[],
    authors: string[],
  ): Promise<DiscoverySnapshot>;
  cancel(runId: string): Promise<void>;
}

/** Small polling response; never transports the saved work catalog. */
export interface DiscoveryProgress extends Omit<DiscoverySnapshot, "records"> {
  recordCount: number;
}
