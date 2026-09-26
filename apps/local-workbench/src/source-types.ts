import type { WorkReference } from "./booklists.ts";
import { inheritLanguageTags } from "./source-language.ts";

export type Source = "JM" | "Pica";
export interface SourceScope {
  source: Source;
  sessionId: string;
}
export interface AuthorQueryPolicy {
  source: Source;
  author: string;
  queries: string[];
  verifiedAliases: string[];
  exactCredits?: string[];
  workCredits?: AuthorWorkCredit[];
  queryFingerprint: string;
}
export interface AuthorWorkCredit {
  workId: string;
  expectedAuthors: string[];
  expectedAuthorVariants?: string[][];
  correctedAuthors: string[];
}
export interface AuthorCreditContext {
  scope: SourceScope;
  policies: AuthorQueryPolicy[];
}
export interface ResolvedAuthorQueryPolicy
  extends AuthorQueryPolicy, SourceScope {
  revision: number;
}
export interface SourceWork {
  source: Source;
  workId: string;
  title: string;
  authors: string[];
  /** Display projection only; never part of a stored source response. */
  authorCreditReview?: { originalAuthors: string[] };
  description: string | null;
  tags: string[];
  favorite: boolean | null;
  chapterCount: number | null;
  pageCount: number | null;
  coverAvailable: boolean;
  sourceUpdatedAt?: string | null;
}
export interface SourcePage {
  items: SourceWork[];
  issues?: SourceItemIssue[];
  page: number;
  total: number | null;
  pages: number | null;
  hasMore: boolean | null;
  folders: SourceFolder[];
}
export interface SourceItemIssue {
  query?: string;
  page: number;
  index: number;
  workId: string | null;
  code: "SOURCE_ITEM_INVALID" | "SOURCE_ITEM_METADATA_MISSING";
}
export interface RankOption {
  id: string;
  label: string;
}
export interface RankOptions {
  categories: RankOption[];
  periods: RankOption[];
}
export interface SourceFolder {
  id: string;
  name: string;
  count: number | null;
}
export interface CatalogSnapshot extends SourcePage {
  complete: boolean;
  updatedAt: number;
  firstPageIds: string[];
  /** Cumulative entry counts after each source page; absent in legacy caches. */
  pageEnds?: number[];
}
export interface CatalogResult extends SourceScope {
  snapshot: CatalogSnapshot | null;
  completeSnapshot: CatalogSnapshot | null;
}
export interface CatalogRequest {
  folderId: string | null;
  reverse: boolean;
  action: "read" | "write";
  snapshot?: CatalogSnapshot;
}
export interface SourceQueryResult extends SourceScope, SourcePage {}
export interface AccountSummary {
  source: Source;
  sessionId: string | null;
  accountId: string | null;
  displayName: string | null;
  state: "disconnected" | "connected" | "expired" | "unavailable";
  remembered: boolean;
  errorCode: string | null;
}
export interface SourceQuery {
  kind: "favorites" | "search" | "detail" | "ranking" | "recent";
  query: string;
  folderId: string | null;
  page: number;
  reverse?: boolean;
}
export interface FavoriteResult extends SourceScope {
  workId: string;
  favorite: boolean;
  changed: boolean;
  verified: true;
}
export interface FollowingSnapshot extends SourceScope {
  revision: number;
  works: { workId: string; title: string }[];
  authors: string[];
}
export interface FollowMutation {
  kind: "work" | "author";
  value: string;
  desired: boolean;
  expectedRevision: number;
}
export interface SourceAdapter {
  readonly available: boolean;
  readonly mode: "native" | "unavailable";
  accounts(refresh?: boolean): Promise<AccountSummary[]>;
  login(input: {
    source: Source;
    username: string;
    password: string;
    remember: boolean;
  }): Promise<AccountSummary>;
  logout(scope: {
    source: Source;
    sessionId: string | null;
  }): Promise<AccountSummary>;
  query(scope: SourceScope, query: SourceQuery): Promise<SourceQueryResult>;
  authorPolicy(
    scope: SourceScope,
    author: string,
  ): Promise<ResolvedAuthorQueryPolicy>;
  rankingOptions(scope: SourceScope): Promise<RankOptions>;
  catalog(scope: SourceScope, request: CatalogRequest): Promise<CatalogResult>;
  favorite(
    scope: SourceScope,
    workId: string,
    desired: boolean,
  ): Promise<FavoriteResult>;
  cover(scope: SourceScope, workId: string): Promise<string | null>;
  following(scope: SourceScope): Promise<FollowingSnapshot>;
  follow(
    scope: SourceScope,
    mutation: FollowMutation,
  ): Promise<FollowingSnapshot>;
}
export const sources: Source[] = ["JM", "Pica"];
export function sourceLabel(source: Source) {
  return source === "Pica" ? "哔咔" : "JM";
}
export function sourceWorkKey(work: WorkReference | SourceWork) {
  return work.source + ":" + work.workId;
}
export function toWorkReference(work: SourceWork): WorkReference {
  return { source: work.source, workId: work.workId };
}
export function mergeSourceWorks(
  previous: SourceWork[],
  incoming: SourceWork[],
) {
  const merged = new Map(previous.map((work) => [sourceWorkKey(work), work]));
  for (const work of incoming) {
    const key = sourceWorkKey(work),
      previous = merged.get(key);
    const tags = previous
      ? inheritLanguageTags(work.tags, previous.tags)
      : work.tags;
    const sourceUpdatedAt =
      work.sourceUpdatedAt == null && previous?.sourceUpdatedAt
        ? previous.sourceUpdatedAt
        : work.sourceUpdatedAt;
    merged.set(
      key,
      tags !== work.tags || sourceUpdatedAt !== work.sourceUpdatedAt
        ? { ...work, tags, sourceUpdatedAt }
        : work,
    );
  }
  return [...merged.values()];
}
export function accountScope(
  account: AccountSummary | undefined,
): SourceScope | null {
  return account?.state === "connected" && account.sessionId
    ? { source: account.source, sessionId: account.sessionId }
    : null;
}
