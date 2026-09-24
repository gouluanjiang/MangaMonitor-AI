import type { CoverPriority } from "./cover-scheduler.ts";

/** Observe the scroll container separately from its small preload margin. */
export function observeCover(
  element: HTMLElement,
  update: (priority: CoverPriority | null) => void,
) {
  if (!("IntersectionObserver" in window)) {
    update(null);
    return () => {};
  }
  let visible = false;
  let nearby = false;
  let disposed = false;
  let scheduled = false;
  let last: CoverPriority | null | undefined;
  const notify = () => {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      if (disposed) return;
      const next = visible ? "visible" : nearby ? "nearby" : null;
      if (last !== next) {
        last = next;
        update(next);
      }
    });
  };
  const root = element.closest("main");
  const viewport = new IntersectionObserver(
    (entries) => {
      visible = entries.some((entry) => entry.isIntersecting);
      notify();
    },
    { root },
  );
  const preload = new IntersectionObserver(
    (entries) => {
      nearby = entries.some((entry) => entry.isIntersecting);
      notify();
    },
    { root, rootMargin: "400px 0px" },
  );
  viewport.observe(element);
  preload.observe(element);
  return () => {
    disposed = true;
    viewport.disconnect();
    preload.disconnect();
  };
}
