import type { Source, SourceScope, SourceWork } from "./source-types.ts";
import type { LibraryReference } from "./library-types.ts";

export type CompletionLanguage = "chinese" | "japanese" | "other" | "unknown";
export type CompletionStatus =
  | "missing"
  | "downloaded"
  | "owned_chinese"
  | "waiting_translation"
  | "translation_available"
  | "translation_downloaded"
  | "review_required"
  | "unknown";
export type CompletionMember =
  | { kind: "source"; reference: LibraryReference }
  | { kind: "phone"; name: string }
  | { kind: "computer"; itemId: string };
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
export interface CompletionCandidate {
  groupId: string;
  reference: LibraryReference;
  kind: "missing" | "translation";
  evidenceHash: string;
}
export interface CompletionGroup {
  groupId: string;
  title: string;
  authors: string[];
  status: CompletionStatus;
  reasons: string[];
  sources: {
    reference: LibraryReference;
    title: string;
    language: CompletionLanguage;
    authorVerified: boolean;
  }[];
  phone: {
    member: CompletionMember;
    name: string;
    language: CompletionLanguage;
  }[];
  computer: {
    member: CompletionMember;
    name: string;
    language: CompletionLanguage;
  }[];
  eligible: CompletionCandidate | null;
}
export interface CompletenessSnapshot {
  revision: number;
  phoneRevision: number;
  libraryRevision: number;
  matchesRevision: number;
  discoveryRevision: number;
  evidenceHash: string;
  groups: CompletionGroup[];
}
export interface CompletionSettings {
  revision: number;
  families: { id: string; members: CompletionMember[] }[];
  languages: { member: CompletionMember; language: CompletionLanguage }[];
}
export interface AutomaticCompletion {
  runId: string | null;
  phase: "idle" | "waiting" | "enqueueing" | "complete" | "cancelled" | "error";
  queued: number;
  skipped: number;
  errorCode: string | null;
}
export interface CompletionView {
  discovery: DiscoverySnapshot;
  completeness: CompletenessSnapshot;
  automatic: AutomaticCompletion;
}
export interface CompletionAdapter {
  read(scopes: SourceScope[], recheckFiles?: boolean): Promise<CompletionView>;
  start(
    scopes: SourceScope[],
    authors: string[],
    automatic: boolean,
    rootId: string | null,
    generation: number,
  ): Promise<CompletionView>;
  cancel(runId: string): Promise<void>;
  settings(): Promise<CompletionSettings>;
  family(
    revision: number,
    members: CompletionMember[],
  ): Promise<CompletionSettings>;
  unlink(revision: number, familyId: string): Promise<CompletionSettings>;
  language(
    revision: number,
    member: CompletionMember,
    language: CompletionLanguage | null,
  ): Promise<CompletionSettings>;
}
