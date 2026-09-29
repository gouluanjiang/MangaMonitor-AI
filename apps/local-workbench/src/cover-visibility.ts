import type { CoverPriority } from "./cover-scheduler.ts";

type Subscription = {
  visible: boolean;
  nearby: boolean;
  last?: CoverPriority | null;
  update(priority: CoverPriority | null): void;
};

const groups = new Map<HTMLElement | null, CoverVisibility>();

/** One observer pair/listener per scroll container, shared by all mounted covers. */
class CoverVisibility {
  private entries = new Map<HTMLElement, Subscription>();
  private viewport: IntersectionObserver;
  private preload!: IntersectionObserver;
  private target: HTMLElement | Window;
  private direction: "down" | "up" = "down";
  private anchor: number;
  private margin = "";
  private preloadEpoch = 0;
  private pending = new Set<HTMLElement>();
  private scheduled = false;
  private disposed = false;
  private root: HTMLElement | null;

  constructor(root: HTMLElement | null) {
    this.root = root;
    this.target = root ?? window;
    this.anchor = this.position();
    this.viewport = new IntersectionObserver(
      (entries) => this.receive(entries, "visible"),
      { root },
    );
    this.rebuildPreload();
    this.target.addEventListener("scroll", this.onScroll, { passive: true });
    window.addEventListener("resize", this.onResize);
  }

  private position() {
    return this.root?.scrollTop ?? window.scrollY;
  }

  private onScroll = () => {
    const position = this.position();
    const delta = position - this.anchor;
    const forward = this.direction === "down" ? delta >= 0 : delta <= 0;
    if (forward) this.anchor = position;
    else if (Math.abs(delta) >= 48) {
      this.direction = this.direction === "down" ? "up" : "down";
      this.anchor = position;
      this.rebuildPreload();
    }
  };

  private onResize = () => this.rebuildPreload();

  private rebuildPreload() {
    const ahead = Math.round(
      Math.min(
        1200,
        Math.max(400, this.root?.clientHeight || window.innerHeight),
      ),
    );
    const margin =
      this.direction === "down"
        ? `200px 0px ${ahead}px 0px`
        : `${ahead}px 0px 200px 0px`;
    if (margin === this.margin) return;
    this.margin = margin;
    const epoch = ++this.preloadEpoch;
    this.preload?.disconnect();
    this.preload = new IntersectionObserver(
      (entries) => {
        if (epoch === this.preloadEpoch) this.receive(entries, "nearby");
      },
      { root: this.root, rootMargin: margin },
    );
    for (const [element, entry] of this.entries) {
      entry.nearby = false;
      this.notify(element);
      this.preload.observe(element);
    }
  }

  private receive(
    entries: IntersectionObserverEntry[],
    field: "visible" | "nearby",
  ) {
    if (this.disposed) return;
    for (const event of entries) {
      const element = event.target as HTMLElement;
      const entry = this.entries.get(element);
      if (!entry) continue;
      entry[field] = event.isIntersecting;
      this.notify(element);
    }
  }

  private notify(element: HTMLElement) {
    this.pending.add(element);
    if (this.scheduled) return;
    this.scheduled = true;
    queueMicrotask(() => {
      this.scheduled = false;
      if (this.disposed) return;
      const pending = [...this.pending];
      this.pending.clear();
      for (const element of pending) {
        const entry = this.entries.get(element);
        if (!entry) continue;
        const next = entry.visible ? "visible" : entry.nearby ? "nearby" : null;
        if (entry.last !== next) {
          entry.last = next;
          entry.update(next);
        }
      }
    });
  }

  observe(element: HTMLElement, update: Subscription["update"]) {
    const subscription: Subscription = {
      visible: false,
      nearby: false,
      update,
    };
    this.entries.set(element, subscription);
    this.viewport.observe(element);
    this.preload.observe(element);
    return () => {
      if (this.entries.get(element) !== subscription) return;
      this.entries.delete(element);
      this.pending.delete(element);
      this.viewport.unobserve(element);
      this.preload.unobserve(element);
      if (this.entries.size === 0) {
        this.disposed = true;
        this.viewport.disconnect();
        this.preload.disconnect();
        this.target.removeEventListener("scroll", this.onScroll);
        window.removeEventListener("resize", this.onResize);
        groups.delete(this.root);
      }
    };
  }
}

/** Visible cards lead; spare capacity prepares one bounded screen in the scroll direction. */
export function observeCover(
  element: HTMLElement,
  update: Subscription["update"],
) {
  if (!("IntersectionObserver" in window)) {
    update(null);
    return () => {};
  }
  const root = element.closest("main");
  let group = groups.get(root);
  if (!group) {
    group = new CoverVisibility(root);
    groups.set(root, group);
  }
  return group.observe(element, update);
}
