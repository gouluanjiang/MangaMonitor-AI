import type { ReaderPosition } from "./types.ts";

// Adapted from Komga PagedReader.vue eagerLoad, MIT, copyright 2019 Gauthier Roebroeck.
// Pinned source: gotson/komga@65981e600edb24944ffaae4818ff2716a5fa08dd
// See THIRD_PARTY_NOTICES.md in this directory for source and full license.
export function isReaderNeighbor(currentPage: number, page: number): boolean {
  return Math.abs(currentPage - page) <= 2;
}
export function readerWindow(current: number, count: number): number[] {
  return [current, current + 1, current - 1, current + 2, current - 2].filter(
    (page) => page >= 0 && page < count && isReaderNeighbor(current, page),
  );
}
export function clampPosition(
  position: ReaderPosition,
  count: number,
): ReaderPosition {
  return {
    chapterId: position.chapterId,
    pageIndex: Math.max(
      0,
      Math.min(count - 1, Math.trunc(position.pageIndex) || 0),
    ),
    offset: Math.max(
      0,
      Math.min(1, Number.isFinite(position.offset) ? position.offset : 0),
    ),
  };
}
export type PageLayout = { tops: number[]; heights: number[]; total: number };
export function pageLayout(
  count: number,
  width: number,
  ratios: ReadonlyMap<number, number>,
): PageLayout {
  const tops: number[] = [],
    heights: number[] = [];
  let total = 0;
  for (let index = 0; index < count; index++) {
    tops.push(total);
    const height = Math.max(1, width * (ratios.get(index) ?? 1.45));
    heights.push(height);
    total += height + 12;
  }
  return { tops, heights, total };
}
export function pageAtOffset(layout: PageLayout, offset: number): number {
  let lo = 0,
    hi = layout.tops.length - 1;
  while (lo < hi) {
    const middle = Math.ceil((lo + hi) / 2);
    if (layout.tops[middle] <= offset) lo = middle;
    else hi = middle - 1;
  }
  return lo;
}
export function visiblePages(
  layout: PageLayout,
  top: number,
  height: number,
): number[] {
  if (!layout.tops.length) return [];
  const first = Math.max(0, pageAtOffset(layout, top) - 1);
  const last = Math.min(
    layout.tops.length - 1,
    pageAtOffset(layout, top + height) + 1,
    first + 11,
  );
  return Array.from({ length: last - first + 1 }, (_, i) => first + i);
}

// Keep only the latest pending position, serializing native writes so a slower older
// write can never replace the location chosen after it.
export class ReaderProgressWriter {
  private pending: ReaderPosition | null = null;
  private running: Promise<void> | null = null;
  private readonly save: (position: ReaderPosition) => Promise<void>;
  constructor(save: (position: ReaderPosition) => Promise<void>) {
    this.save = save;
  }
  set(position: ReaderPosition): void {
    this.pending = { ...position };
  }
  flush(): Promise<void> {
    if (this.running)
      return this.running
        .catch(() => undefined)
        .then(() => (this.pending ? this.flush() : undefined));
    this.running = (async () => {
      while (this.pending) {
        const next = this.pending;
        this.pending = null;
        try {
          await this.save(next);
        } catch (error) {
          this.pending ??= next;
          throw error;
        }
      }
    })().finally(() => {
      this.running = null;
    });
    return this.running;
  }
}
