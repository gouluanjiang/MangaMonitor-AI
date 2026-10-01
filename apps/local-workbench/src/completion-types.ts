import type {
  Source,
  SourceScope,
  SourceWork,
  SourceItemIssue,
  AuthorQueryPolicy,
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
  currentQueryIndex?: number | null;
  currentQueryCount?: number | null;
}
/** Latest accepted manual check; discovered does not mean newly published. */
export interface DiscoveryCheckSummary {
  id: string;
  startedAt: number;
  finishedAt: number | null;
  phase: ScanPhase | "interrupted";
  mode: DiscoveryMode;
  onlyUnfinished: boolean;
  firstCatalog: boolean;
  allFollowed: boolean;
  authorCount: number;
  totalScopes: number;
  attemptedScopes: number;
  completeScopes: number;
}
export interface DiscoverySnapshot {
  searchMetrics?: import("./author-search-metrics.ts").AuthorSearchMetrics;
  scopes: SourceScope[];
  revision: number;
  followingRevision?: number;
  policyRevision?: number;
  followedAuthors?: string[];
  run: DiscoveryRun | null;
  lastCheck?: DiscoveryCheckSummary | null;
  /** Successful manual scan only; background special checks never replace it. */
  lastSuccessfulCheck?: DiscoveryCheckSummary | null;
  otherRecordCount?: number;
  includesOther?: boolean;
  authorPolicies?: AuthorQueryPolicy[];
  authors: {
    author: string;
    source: Source;
    state: ScanPhase | "idle";
    lastAttemptAt: number | null;
    lastCompleteAt: number | null;
    lastCheckedAt?: number | null;
    lastCheckMode?: DiscoveryMode | null;
    baseline?: DiscoveryBaseline | null;
    queryFingerprint?: string | null;
    queryBaselines?: { query: string; baseline: DiscoveryBaseline }[];
    completedQueries?: string[];
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
    /** Absent legacy values are historical, never inferred from observedAt. */
    firstDiscoveredRunId?: string;
    /** Last verified detail response, separate from lightweight list metadata. */
    metadataDetailAt?: number;
  }[];
  /** Ad-hoc search only: saved works absent from this request's raw response. */
  historicalSupplementCount?: number;
  historicalSupplementAt?: number | null;
  historicalReadError?: boolean;
  observationErrorCode?: string | null;
}
export interface CompletionAdapter {
  /** In-memory ad-hoc searches publish page batches without catalog polling. */
  subscribe?(listener: () => void): () => void;
  startRecentCheck?(
    scopes: SourceScope[],
    maxPages?: number,
  ): Promise<RecentCheckRun>;
  recentCheckProgress?(): Promise<RecentCheckRun | null>;
  cancelRecentCheck?(runId: string): Promise<RecentCheckRun>;
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

export interface RecentCheckRun {
  id: string;
  phase: "checking" | "complete" | "partial" | "cancelled";
  currentSource: Source | null;
  currentPage: number;
  pagesRead: number;
  recordsRead: number;
  errorCode: string | null;
  results: {
    source: Source;
    pagesRead: number;
    recordsRead: number;
    reachedEnd: boolean;
    joinedPrevious: boolean;
    initialWindow?: boolean;
    errorCode: string | null;
  }[];
}

/** Small polling response; never transports the saved work catalog. */
export interface DiscoveryProgress extends Omit<DiscoverySnapshot, "records"> {
  recordCount: number;
}
