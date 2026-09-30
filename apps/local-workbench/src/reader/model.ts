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
export function readerRequestedPages(
  current: number,
  count: number,
  visible: readonly number[],
  active: boolean,
): number[] {
  if (!count) return [];
  return [
    ...new Set([
      current,
      ...visible,
      ...(active ? readerWindow(current, count) : []),
    ]),
  ]
    .filter((index) => index >= 0 && index < count)
    .slice(0, 12);
}

// One mounted reader owns one native open. Closing its window cancels a pending
// open or awaits that reader's flush/close, without touching another window.
export class ReaderLifetime {
  private stopped = false;
  private disposed = false;
  private attached: (() => Promise<void>) | null = null;
  private cleanup: (() => Promise<void>) | null = null;
  private closing: Promise<void> | null = null;
  private disposing: Promise<void> | null = null;
  private readonly cancel: () => Promise<void>;
  constructor(cancel: () => Promise<void>) {
    this.cancel = cancel;
  }
  attach(close: () => Promise<void>, cleanup = close): boolean {
    this.attached = close;
    this.cleanup = cleanup;
    if (!this.stopped) return true;
    void (this.disposed ? cleanup() : close()).catch(() => undefined);
    return false;
  }
  close(): Promise<void> {
    this.stopped = true;
    if (this.closing) return this.closing;
    const closing = (this.attached ? this.attached() : this.cancel()).catch(
      (failure) => {
        if (this.closing === closing) this.closing = null;
        throw failure;
      },
    );
    this.closing = closing;
    return closing;
  }
  /** Unmount cannot ask the user to retry; release resources even if saving fails. */
  dispose(): Promise<void> {
    this.stopped = true;
    this.disposed = true;
    return (this.disposing ??= this.cleanup ? this.cleanup() : this.close());
  }
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
export const maxReaderPageHeight = 250_000;
export type PageSegment = {
  first: number;
  last: number;
  start: number;
  total: number;
};

// Native/browser scrolling clamps very large CSS coordinates. Keep one nearby
// physical band and translate it to logical chapter coordinates, without loading
// the chapter's images or making the progress slider depend on that band.
export function pageSegment(
  layout: PageLayout,
  page: number,
  budget = 1_000_000,
): PageSegment {
  if (!layout.tops.length) return { first: 0, last: 0, start: 0, total: 0 };
  const current = Math.max(0, Math.min(layout.tops.length - 1, page));
  let first = current,
    last = current;
  const center = layout.tops[current] + layout.heights[current] / 2;
  while (first > 0 && center - layout.tops[first - 1] <= budget / 2) first--;
  while (
    last + 1 < layout.tops.length &&
    layout.tops[last + 1] + layout.heights[last + 1] + 12 - center <= budget / 2
  )
    last++;
  const start = layout.tops[first];
  return {
    first,
    last,
    start,
    total: layout.tops[last] + layout.heights[last] + 12 - start,
  };
}
export function pageLayout(
  count: number,
  width: number,
  ratios: ReadonlyMap<number, number>,
  minimumHeight = 1,
): PageLayout {
  const tops: number[] = [],
    heights: number[] = [];
  let total = 0;
  for (let index = 0; index < count; index++) {
    tops.push(total);
    const height = Math.max(
      Math.max(1, Math.min(maxReaderPageHeight, minimumHeight)),
      Math.min(maxReaderPageHeight, width * (ratios.get(index) ?? 1.45)),
    );
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
    // Browser scroll positions round fractional CSS pixels. A requested page
    // top can therefore come back just below that top after band translation.
    // Ignore at most one CSS pixel; the rest of the 12px gap stays on its page.
    if (layout.tops[middle] <= offset + 1) lo = middle;
    else hi = middle - 1;
  }
  return lo;
}
// The viewport's top remains the saved/resize anchor. At the chapter's real
// bottom, however, a short final image can be fully read while that top still
// belongs to an earlier page. Report the final page without moving the anchor.
export function verticalReaderProgress(
  layout: PageLayout,
  segment: PageSegment,
  top: number,
  viewportHeight: number,
): { pageIndex: number; atEnd: boolean } {
  const last = layout.tops.length - 1;
  const atEnd =
    last >= 0 &&
    segment.last === last &&
    top + viewportHeight >= layout.total - 1;
  return { pageIndex: atEnd ? last : pageAtOffset(layout, top), atEnd };
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
  private accepting = true;
  private discarded = false;
  private readonly save: (position: ReaderPosition) => Promise<void>;
  constructor(save: (position: ReaderPosition) => Promise<void>) {
    this.save = save;
  }
  set(position: ReaderPosition): void {
    if (this.accepting) this.pending = { ...position };
  }
  stopAccepting(): void {
    this.accepting = false;
  }
  resumeAccepting(): void {
    if (!this.discarded) this.accepting = true;
  }
  async discard(): Promise<void> {
    this.stopAccepting();
    this.discarded = true;
    this.pending = null;
    // An already submitted native write cannot be undone or raced by closing.
    await this.running?.catch(() => undefined);
    this.pending = null;
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
          if (!this.discarded) this.pending ??= next;
          throw error;
        }
      }
    })().finally(() => {
      this.running = null;
    });
    return this.running;
  }
}

export class ReaderPositionSaveError extends Error {
  readonly reason: unknown;
  constructor(reason: unknown) {
    super("READER_POSITION_SAVE_FAILED");
    this.reason = reason;
  }
}

/** Keep a failed save retryable without closing its native reader session. */
export class ReaderCloseController {
  private closing: Promise<void> | null = null;
  private releasing: Promise<void> | null = null;
  private released = false;
  private disposed = false;
  private readonly writer: ReaderProgressWriter;
  private readonly release: () => Promise<void>;
  constructor(writer: ReaderProgressWriter, release: () => Promise<void>) {
    this.writer = writer;
    this.release = release;
  }
  private releaseOnce(): Promise<void> {
    if (this.released) return Promise.resolve();
    if (this.releasing) return this.releasing;
    const pending = Promise.resolve()
      .then(this.release)
      .then(() => {
        this.released = true;
        this.writer.stopAccepting();
      })
      .finally(() => {
        this.releasing = null;
      });
    this.releasing = pending;
    return pending;
  }
  close(discardPosition = false): Promise<void> {
    if (this.released) return Promise.resolve();
    if (this.closing) return this.closing;
    this.writer.stopAccepting();
    const pending = (async () => {
      if (discardPosition) await this.writer.discard();
      else {
        try {
          await this.writer.flush();
        } catch (failure) {
          throw new ReaderPositionSaveError(failure);
        }
      }
      await this.releaseOnce();
    })();
    this.closing = pending;
    void pending.catch(() => {
      if (this.closing === pending) this.closing = null;
      if (!this.disposed) this.writer.resumeAccepting();
    });
    return pending;
  }
  async dispose(): Promise<void> {
    this.disposed = true;
    this.writer.stopAccepting();
    try {
      await this.close();
    } catch {
      await this.writer.discard();
      await this.releaseOnce();
    }
  }
}
