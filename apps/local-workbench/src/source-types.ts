import type { WorkReference } from "./booklists.ts";

export type Source = "JM" | "Pica";
export interface SourceScope {
  source: Source;
  sessionId: string;
}
export interface SourceWork {
  source: Source;
  workId: string;
  title: string;
  authors: string[];
  description: string | null;
  tags: string[];
  favorite: boolean | null;
  chapterCount: number | null;
  pageCount: number | null;
  coverAvailable: boolean;
}
export interface SourcePage {
  items: SourceWork[];
  page: number;
  total: number | null;
  pages: number | null;
  hasMore: boolean | null;
  folders: { id: string; name: string; count: number | null }[];
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
  kind: "favorites" | "search" | "detail";
  query: string;
  folderId: string | null;
  page: number;
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
  for (const work of incoming) merged.set(sourceWorkKey(work), work);
  return [...merged.values()];
}
export function accountScope(
  account: AccountSummary | undefined,
): SourceScope | null {
  return account?.state === "connected" && account.sessionId
    ? { source: account.source, sessionId: account.sessionId }
    : null;
}
