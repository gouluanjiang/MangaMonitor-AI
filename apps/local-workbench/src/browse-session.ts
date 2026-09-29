export interface BrowseAnchor {
  key: string;
  offset: number;
}

export interface BrowsePosition {
  anchor: BrowseAnchor | null;
  keys: readonly string[];
  scroll: number;
}

/** Runtime-only: a fresh WebView starts with no browsing history. */
const positions = new Map<string, BrowsePosition>();
const states = new Map<string, unknown>();

export function browseScope(...parts: (string | number | null)[]): string {
  return JSON.stringify(parts);
}

export function readBrowsePosition(scope: string): BrowsePosition | undefined {
  return positions.get(scope);
}

export function saveBrowsePosition(
  scope: string,
  position: BrowsePosition,
): void {
  positions.set(scope, position);
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
        return { key, offset: position.anchor.offset };
    }
  }
  return null;
}

export function readBrowseState<T>(scope: string, initial: () => T): T {
  if (!states.has(scope)) states.set(scope, initial());
  return states.get(scope) as T;
}

export function saveBrowseState<T>(scope: string, value: T): void {
  states.set(scope, value);
}
