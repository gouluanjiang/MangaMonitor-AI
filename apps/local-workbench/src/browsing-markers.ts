/** Identity-only browse baselines; publication time, scan additions and unread are separate. */
export const MAX_BROWSING_IDS = 500_000;
export type BrowsingSurface = "recent" | "authors";
export interface BrowsingBaseline {
  knownIds: string[];
  headIds: string[];
  reachedEnd: boolean;
}
export interface RecentBrowsingObservation {
  liveIds: string[] | null;
  savedIds: string[];
  historyReady: boolean;
  hasIssues: boolean;
  reachedEnd: boolean;
  failed: boolean;
}

export function validateBrowsingBaseline(
  value: unknown,
): BrowsingBaseline | null {
  if (value === null) return null;
  if (!value || typeof value !== "object")
    throw new Error("BROWSING_BASELINE_INVALID");
  const row = value as Record<string, unknown>;
  const ids = (input: unknown, maximum: number): input is string[] =>
    Array.isArray(input) &&
    input.length <= maximum &&
    input.every(
      (id) => typeof id === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(id),
    ) &&
    new Set(input).size === input.length;
  if (
    !ids(row.knownIds, MAX_BROWSING_IDS) ||
    !ids(row.headIds, 1000) ||
    typeof row.reachedEnd !== "boolean"
  )
    throw new Error("BROWSING_BASELINE_INVALID");
  const known = new Set(row.knownIds);
  if (row.headIds.some((id) => !known.has(id)))
    throw new Error("BROWSING_BASELINE_INVALID");
  return {
    knownIds: [...row.knownIds],
    headIds: [...row.headIds],
    reachedEnd: row.reachedEnd,
  };
}

export function validateBrowsingDocument(
  raw: unknown,
): BrowsingBaseline | null {
  if (!raw || typeof raw !== "object")
    throw new Error("BROWSING_BASELINE_INVALID");
  const document = raw as {
    revision?: unknown;
    value?: { version?: unknown; baseline?: unknown };
  };
  if (
    !Number.isSafeInteger(document.revision) ||
    (document.revision as number) < 0 ||
    document.value?.version !== 1
  )
    throw new Error("BROWSING_BASELINE_INVALID");
  return validateBrowsingBaseline(document.value.baseline);
}

const same = (left: readonly string[], right: readonly string[]) =>
  left.length === right.length &&
  left.every((id, index) => id === right[index]);

/** Opening baseline never changes during this launch, even after successful saves. */
export class BrowsingMarkerTracker {
  readonly firstLaunch: boolean;
  readonly newIds = new Set<string>();
  readonly opening: BrowsingBaseline | null;
  private known: Set<string>;
  next: BrowsingBaseline | null;
  status: "waiting" | "initial" | "ready" | "partial" = "waiting";
  limited = false;
  revision = 0;

  constructor(baseline: BrowsingBaseline | null) {
    this.opening = validateBrowsingBaseline(baseline);
    this.firstLaunch = baseline === null;
    this.known = new Set(baseline?.knownIds ?? []);
    this.next = this.opening;
  }

  private remember(ids: string[], headIds: string[], reachedEnd: boolean) {
    const known = new Set([...(this.next?.knownIds ?? []), ...ids]);
    if (known.size > MAX_BROWSING_IDS) {
      this.limited = true;
      this.status = "partial";
      return;
    }
    const next = {
      knownIds: [...known],
      headIds: headIds.slice(0, 1000),
      reachedEnd,
    };
    if (
      !this.next ||
      !same(this.next.knownIds, next.knownIds) ||
      !same(this.next.headIds, next.headIds) ||
      this.next.reachedEnd !== next.reachedEnd
    )
      this.next = next;
  }

  private withinLimit(ids: string[]) {
    return (
      ids.length <= MAX_BROWSING_IDS * 2 &&
      new Set([...(this.next?.knownIds ?? []), ...this.newIds, ...ids]).size <=
        MAX_BROWSING_IDS
    );
  }

  observeAuthors(ids: string[], incomplete: boolean) {
    const previous = this.next,
      count = this.newIds.size,
      status = this.status;
    if (!this.withinLimit(ids)) {
      this.limited = true;
      this.status = "partial";
    } else {
      if (!this.firstLaunch)
        for (const id of ids) if (!this.known.has(id)) this.newIds.add(id);
      this.status = incomplete
        ? "partial"
        : this.firstLaunch
          ? "initial"
          : "ready";
      this.remember(ids, [], false);
    }
    if (
      this.next !== previous ||
      count !== this.newIds.size ||
      status !== this.status
    )
      this.revision++;
  }

  observeRecent(input: RecentBrowsingObservation) {
    const previous = this.next,
      count = this.newIds.size,
      status = this.status;
    const live = input.liveIds;
    if (!input.historyReady || live === null) {
      this.status = input.failed ? "partial" : "waiting";
    } else if (!this.withinLimit([...live, ...input.savedIds])) {
      this.limited = true;
      this.status = "partial";
    } else {
      const heads = new Set(this.opening?.headIds ?? []);
      const joinedAt = live.findIndex((id) => heads.has(id));
      const emptyBaseline = this.opening?.reachedEnd && heads.size === 0;
      const unprovenBaseline =
        !this.firstLaunch && heads.size === 0 && !emptyBaseline;
      const joined = joinedAt >= 0 || !!emptyBaseline;
      const trustworthy = !input.hasIssues && !input.failed;
      if (!this.firstLaunch && joined && trustworthy) {
        // Tail pages and arbitrary saved records can never create badges.
        for (const id of live.slice(0, emptyBaseline ? live.length : joinedAt))
          if (!this.known.has(id)) this.newIds.add(id);
      }
      this.status =
        !trustworthy || (!this.firstLaunch && !joined)
          ? "partial"
          : this.firstLaunch
            ? "initial"
            : "ready";
      if (this.firstLaunch || (trustworthy && (joined || unprovenBaseline))) {
        this.remember(
          [...input.savedIds, ...live],
          trustworthy ? live.slice(0, 1000) : (this.next?.headIds ?? []),
          trustworthy ? input.reachedEnd : (this.next?.reachedEnd ?? false),
        );
      }
      // A failed/gapped comparison never advances the next launch's head.
    }
    if (
      this.next !== previous ||
      count !== this.newIds.size ||
      status !== this.status
    )
      this.revision++;
  }
}
