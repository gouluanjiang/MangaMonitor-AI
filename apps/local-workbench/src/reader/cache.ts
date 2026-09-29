import type { ReaderImage } from "./types.ts";
import { ReaderError } from "./runtime.ts";

export type CachedPage =
  | { state: "loading" }
  | { state: "error"; error: unknown }
  | { state: "ready"; image: ReaderImage };
export class ReaderPageCache {
  readonly entries = new Map<number, CachedPage>();
  private desired: number[] = [];
  private inFlight = new Set<number>();
  private disposed = false;
  private suppressed = new Set<number>();
  private readonly load: (pageIndex: number) => Promise<ReaderImage>;
  private readonly changed: () => void;
  readonly maxBytes: number;
  constructor(
    load: (pageIndex: number) => Promise<ReaderImage>,
    changed: () => void,
    maxBytes = 96 * 1024 * 1024,
  ) {
    this.load = load;
    this.changed = changed;
    this.maxBytes = maxBytes;
  }
  get bytes(): number {
    let result = 0;
    for (const entry of this.entries.values())
      if (entry.state === "ready") result += imageBytes(entry.image);
    return result;
  }
  request(pages: number[]): void {
    if (this.disposed) return;
    const desired = [...new Set(pages)].slice(0, 12);
    if (desired.join(",") !== this.desired.join(",")) this.suppressed.clear();
    this.desired = desired;
    for (const key of this.entries.keys())
      if (!this.desired.includes(key)) this.entries.delete(key);
    this.pump();
  }
  retry(page: number): void {
    if (this.entries.get(page)?.state !== "error") return;
    this.entries.delete(page);
    this.pump();
  }
  dispose(): void {
    this.disposed = true;
    this.desired = [];
    this.entries.clear();
  }
  private pump(): void {
    if (this.disposed) return;
    for (const index of this.desired) {
      if (this.inFlight.size >= 2) return;
      if (
        this.inFlight.has(index) ||
        this.entries.has(index) ||
        this.suppressed.has(index)
      )
        continue;
      this.inFlight.add(index);
      this.entries.set(index, { state: "loading" });
      void this.load(index)
        .then((image) => {
          if (this.disposed || !this.desired.includes(index)) return;
          if (image.pageIndex !== index)
            throw new ReaderError("READER_STALE_RESPONSE");
          const size = imageBytes(image);
          if (size > this.maxBytes) throw new ReaderError("READER_IMAGE_LIMIT");
          for (const other of [...this.desired].reverse()) {
            if (this.bytes + size <= this.maxBytes) break;
            if (this.desired.indexOf(other) > this.desired.indexOf(index)) {
              this.entries.delete(other);
              this.suppressed.add(other);
            }
          }
          if (this.bytes + size <= this.maxBytes)
            this.entries.set(index, { state: "ready", image });
          else {
            this.entries.delete(index);
            this.suppressed.add(index);
          }
        })
        .catch((error: unknown) => {
          if (!this.disposed && this.desired.includes(index))
            this.entries.set(index, { state: "error", error });
        })
        .finally(() => {
          this.inFlight.delete(index);
          if (!this.disposed) {
            this.changed();
            this.pump();
          }
        });
    }
  }
}
function imageBytes(image: ReaderImage): number {
  return image.dataUrl.length * 2 + image.width * image.height * 4;
}
