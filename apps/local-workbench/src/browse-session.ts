export interface BrowseAnchor {
  key: string;
  offset: number;
  atTop?: boolean;
}

export interface BrowsePosition {
  anchor: BrowseAnchor | null;
  keys: readonly string[];
  scroll: number;
}

// These are logical retention budgets (UTF-16 text plus references/entry costs),
// not a claim about a JavaScript engine's measured heap. A mounted list keeps its
// own current, bounded position; old query variants are evictable LRU entries.
export const browseCacheLimits = {
  positions: 128,
  positionKeys: 65536,
  positionBytes: 8 * 1024 * 1024,
  keysPerPosition: 4096,
  bytesPerPosition: 512 * 1024,
  states: 128,
  stateBytes: 256 * 1024,
} as const;

interface CacheContext {
  owner?: symbol;
  sessions?: readonly string[];
}
interface Entry<T> extends CacheContext {
  value: T;
  keys: number;
  bytes: number;
}
let currentSessions: Set<string> | null = null;
const textBytes = (value: string) => value.length * 2;
export function browseSessionKeys(
  scopes: readonly { source: string; sessionId: string }[],
): string[] {
  return scopes.map((scope) => JSON.stringify([scope.source, scope.sessionId]));
}
export function browseSessionsCurrent(sessions: readonly string[]): boolean {
  return !currentSessions || sessions.every((key) => currentSessions!.has(key));
}

class BrowseCache<T> {
  private entries = new Map<string, Entry<T>>();
  private keys = 0;
  private bytes = 0;
  private maximumEntries: number;
  private maximumKeys: number;
  private maximumBytes: number;
  constructor(
    maximumEntries: number,
    maximumKeys: number,
    maximumBytes: number,
  ) {
    this.maximumEntries = maximumEntries;
    this.maximumKeys = maximumKeys;
    this.maximumBytes = maximumBytes;
  }
  get(scope: string): T | undefined {
    const entry = this.entries.get(scope);
    if (!entry) return undefined;
    this.entries.delete(scope);
    this.entries.set(scope, entry);
    return entry.value;
  }
  set(
    scope: string,
    value: T,
    keys: number,
    bytes: number,
    context: CacheContext,
  ) {
    this.remove(scope);
    const sessions = context.sessions ?? [];
    bytes +=
      128 +
      textBytes(scope) +
      sessions.reduce((sum, key) => sum + textBytes(key) + 16, 0);
    if (
      !browseSessionsCurrent(sessions) ||
      keys > this.maximumKeys ||
      bytes > this.maximumBytes
    )
      return;
    this.entries.set(scope, {
      value,
      keys,
      bytes,
      owner: context.owner,
      sessions,
    });
    this.keys += keys;
    this.bytes += bytes;
    while (
      this.entries.size > this.maximumEntries ||
      this.keys > this.maximumKeys ||
      this.bytes > this.maximumBytes
    )
      this.remove(this.entries.keys().next().value!);
  }
  remove(scope: string) {
    const entry = this.entries.get(scope);
    if (!entry) return;
    this.entries.delete(scope);
    this.keys -= entry.keys;
    this.bytes -= entry.bytes;
  }
  forget(predicate: (entry: Entry<T>) => boolean) {
    for (const [scope, entry] of this.entries)
      if (predicate(entry)) this.remove(scope);
  }
  usage() {
    return { entries: this.entries.size, keys: this.keys, bytes: this.bytes };
  }
}

/** Runtime-only: a fresh WebView starts with no browsing history. */
const positions = new BrowseCache<BrowsePosition>(
  browseCacheLimits.positions,
  browseCacheLimits.positionKeys,
  browseCacheLimits.positionBytes,
);
const states = new BrowseCache<unknown>(
  browseCacheLimits.states,
  0,
  browseCacheLimits.stateBytes,
);
const positionBytes = (position: BrowsePosition) =>
  64 +
  (position.anchor ? textBytes(position.anchor.key) + 32 : 0) +
  position.keys.reduce((sum, key) => sum + textBytes(key) + 16, 0);

/** Keep the exact anchor and its nearest neighbors, rather than retaining every
 * work ID for every typed query. If all retained neighbors disappear, return to
 * the top instead of guessing a distant old position. */
function boundedPosition(position: BrowsePosition): BrowsePosition {
  const keys = position.keys;
  const anchor = position.anchor;
  if (!anchor) return keys.length ? { ...position, keys: [] } : position;
  const index = Math.max(0, keys.indexOf(anchor.key));
  let start = Math.max(
    0,
    index - Math.floor((browseCacheLimits.keysPerPosition - 1) / 2),
  );
  let end = Math.min(keys.length, start + browseCacheLimits.keysPerPosition);
  start = Math.max(0, end - browseCacheLimits.keysPerPosition);
  let bytes = 96 + textBytes(anchor.key);
  for (let i = start; i < end; i++) bytes += textBytes(keys[i]) + 16;
  while (bytes > browseCacheLimits.bytesPerPosition && end - start > 1) {
    const remove = index - start >= end - 1 - index ? start++ : --end;
    bytes -= textBytes(keys[remove]) + 16;
  }
  if (bytes > browseCacheLimits.bytesPerPosition) {
    return {
      anchor:
        textBytes(anchor.key) + 96 <= browseCacheLimits.bytesPerPosition
          ? anchor
          : null,
      keys: [],
      scroll: position.scroll,
    };
  }
  return start === 0 && end === keys.length
    ? position
    : { ...position, keys: keys.slice(start, end) };
}

export function browseScope(...parts: (string | number | null)[]): string {
  return JSON.stringify(parts);
}

export function readBrowsePosition(
  scope: string,
  fallback?: { scope: string; position: BrowsePosition },
  sessions: readonly string[] = [],
): BrowsePosition | undefined {
  if (!browseSessionsCurrent(sessions)) return undefined;
  return (
    positions.get(scope) ??
    (fallback?.scope === scope ? fallback.position : undefined)
  );
}

export function saveBrowsePosition(
  scope: string,
  position: BrowsePosition,
  context: CacheContext = {},
): BrowsePosition {
  const bounded = boundedPosition(position);
  positions.set(
    scope,
    bounded,
    bounded.keys.length,
    positionBytes(bounded),
    context,
  );
  return bounded;
}

/** Release only the positions owned by a workspace that has actually closed.
 * Hidden tabs and ordinary page transitions still retain their own snapshots. */
export function forgetBrowsePositions(scopes: Iterable<string>): void {
  for (const scope of scopes) positions.remove(scope);
}

export function forgetBrowseOwner(owner: symbol): void {
  positions.forget((entry) => entry.owner === owner);
  states.forget((entry) => entry.owner === owner);
}

/** An invalidated session cannot republish its positions during late cleanup. */
export function retainBrowseSessions(
  scopes: readonly { source: string; sessionId: string }[],
): void {
  currentSessions = new Set(browseSessionKeys(scopes));
  positions.forget((entry) => !browseSessionsCurrent(entry.sessions ?? []));
  states.forget((entry) => !browseSessionsCurrent(entry.sessions ?? []));
}

export function browseCacheUsage() {
  return { positions: positions.usage(), states: states.usage() };
}

export function resolveBrowseAnchor(
  position: BrowsePosition,
  keys: readonly string[],
): BrowseAnchor | null {
  if (!position.anchor || !keys.length) return null;
  const available = new Set(keys);
  if (available.has(position.anchor.key)) return position.anchor;
  const index = position.keys.indexOf(position.anchor.key);
  if (index < 0) return null;
  // Prefer the next surviving work; if the tail disappeared, fall back backward.
  for (let distance = 1; distance < position.keys.length; distance++) {
    for (const candidate of [index + distance, index - distance]) {
      const key = position.keys[candidate];
      if (key !== undefined && available.has(key))
        return { ...position.anchor, key };
    }
  }
  return null;
}

export function readBrowseState<T>(
  scope: string,
  initial: () => T,
  context: CacheContext = {},
): T {
  const known = states.get(scope);
  if (known !== undefined) return known as T;
  const value = initial();
  saveBrowseState(scope, value, context);
  return value;
}

export function saveBrowseState<T>(
  scope: string,
  value: T,
  context: CacheContext = {},
): void {
  let bytes = Infinity;
  try {
    bytes = textBytes(JSON.stringify(value) ?? "") + 32;
  } catch {
    // A non-serializable future state can stay in its component, never this cache.
  }
  states.set(scope, value, 0, bytes, context);
}
