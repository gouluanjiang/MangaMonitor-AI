import type { RecentUpdatesReader } from "./recent-updates.ts";

/** One user-directed continuation; layout changes and request completion are not inputs. */
export function bindRecentUpdatesScroll(
  main: HTMLElement,
  sentinel: HTMLElement,
  reader: RecentUpdatesReader,
) {
  let frame = 0;
  let lastScroll = main.scrollTop;
  let downUntil = 0;
  let pointer: {
    id: number;
    x: number;
    y: number;
    credit: boolean;
  } | null = null;
  let touch: { id: number; x: number; y: number } | null = null;
  const ready = () =>
    reader.state.phase === "ready" &&
    reader.state.snapshot?.hasMore === true &&
    !document.querySelector('dialog[open], [aria-modal="true"]');
  const clearIntent = () => {
    downUntil = 0;
    if (pointer) pointer.credit = false;
    cancelAnimationFrame(frame);
    frame = 0;
  };
  const check = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!ready()) {
        clearIntent();
        return;
      }
      if (
        main.clientHeight === 0 ||
        sentinel.getBoundingClientRect().top >
          main.getBoundingClientRect().bottom + 360
      )
        return;
      // Consume this input before starting the request. Neither its completion
      // nor a subsequent virtual-grid layout adjustment receives another credit.
      clearIntent();
      void reader.loadNext();
    });
  };
  const down = () => {
    if (!ready()) {
      clearIntent();
      return;
    }
    // Wheel/keyboard scrolling can settle over several animation frames. A
    // held scrollbar pointer has a separate credit without a time limit.
    downUntil = performance.now() + 1000;
    if (pointer) pointer.credit = true;
    check();
  };
  const wheel = (event: WheelEvent) => {
    if (
      event.ctrlKey ||
      event.shiftKey ||
      event.deltaY <= 0 ||
      Math.abs(event.deltaY) <= Math.abs(event.deltaX) ||
      (event.target instanceof Element &&
        event.target.closest(
          'input, select, textarea, [contenteditable]:not([contenteditable="false"])',
        ))
    ) {
      clearIntent();
      return;
    }
    // At the bottom wheel still fires, even when scrollTop cannot change.
    down();
  };
  const controls = (target: EventTarget | null, key: string) =>
    target instanceof Element &&
    Boolean(
      target.closest(
        'input, select, textarea, [role="textbox"], [contenteditable]:not([contenteditable="false"])',
      ) ||
      ([" ", "Enter"].includes(key) &&
        target.closest('button, a, [role="button"]')),
    );
  const key = (event: KeyboardEvent) => {
    if (
      event.defaultPrevented ||
      controls(event.target, event.key) ||
      !(event.target instanceof Node) ||
      (!main.contains(event.target) &&
        event.target !== document.body &&
        event.target !== document.documentElement)
    )
      return;
    if (
      ["ArrowUp", "PageUp", "Home"].includes(event.key) ||
      (event.key === " " && event.shiftKey)
    ) {
      clearIntent();
      return;
    }
    if (["ArrowDown", "PageDown", "End", " "].includes(event.key)) down();
  };
  const pointerDown = (event: PointerEvent) => {
    const rect = main.getBoundingClientRect();
    // Clicking a card or action is not scroll intent. Only a scrollbar drag
    // can keep its mouse credit while held; touch direction is tracked below.
    if (
      event.pointerType === "touch" ||
      event.button !== 0 ||
      event.target !== main ||
      event.clientX <
        rect.right - Math.max(16, main.offsetWidth - main.clientWidth)
    )
      return;
    pointer = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      credit: true,
    };
  };
  const pointerMove = (event: PointerEvent) => {
    if (!pointer || pointer.id !== event.pointerId) return;
    if (!(event.buttons & 1)) {
      pointer = null;
      clearIntent();
      return;
    }
    const dx = event.clientX - pointer.x;
    const dy = event.clientY - pointer.y;
    pointer.x = event.clientX;
    pointer.y = event.clientY;
    if (dy > 0 && Math.abs(dy) > Math.abs(dx)) down();
    else if (dy < 0) clearIntent();
  };
  const pointerUp = (event: PointerEvent) => {
    if (pointer?.id === event.pointerId) pointer = null;
  };
  const reset = () => {
    pointer = null;
    touch = null;
    clearIntent();
  };
  const pointerCancel = (event: PointerEvent) => {
    if (pointer?.id === event.pointerId) {
      pointer = null;
      clearIntent();
    }
  };
  const touchStart = (event: TouchEvent) => {
    const first = event.touches[0];
    touch =
      event.touches.length === 1
        ? { id: first.identifier, x: first.clientX, y: first.clientY }
        : null;
    clearIntent();
  };
  const touchMove = (event: TouchEvent) => {
    const next = Array.from(event.touches).find(
      (item) => item.identifier === touch?.id,
    );
    if (!touch || !next || event.touches.length !== 1) return;
    const dx = next.clientX - touch.x;
    const dy = touch.y - next.clientY;
    touch.x = next.clientX;
    touch.y = next.clientY;
    if (dy > 0 && Math.abs(dy) > Math.abs(dx)) down();
    else if (dy < 0) clearIntent();
  };
  const touchEnd = () => {
    touch = null;
  };
  const scroll = () => {
    const previous = lastScroll;
    lastScroll = main.scrollTop;
    if (lastScroll < previous) clearIntent();
    else if (
      lastScroll > previous &&
      (pointer?.credit || performance.now() < downUntil)
    ) {
      if (ready()) check();
      else clearIntent();
    }
  };
  main.addEventListener("wheel", wheel, { passive: true });
  main.addEventListener("pointerdown", pointerDown, { passive: true });
  main.addEventListener("touchstart", touchStart, { passive: true });
  main.addEventListener("touchmove", touchMove, { passive: true });
  main.addEventListener("touchend", touchEnd, { passive: true });
  main.addEventListener("touchcancel", reset, { passive: true });
  main.addEventListener("scroll", scroll, { passive: true });
  window.addEventListener("keydown", key);
  window.addEventListener("pointermove", pointerMove, { passive: true });
  window.addEventListener("pointerup", pointerUp);
  window.addEventListener("pointercancel", pointerCancel);
  window.addEventListener("blur", reset);
  return () => {
    reset();
    main.removeEventListener("wheel", wheel);
    main.removeEventListener("pointerdown", pointerDown);
    main.removeEventListener("touchstart", touchStart);
    main.removeEventListener("touchmove", touchMove);
    main.removeEventListener("touchend", touchEnd);
    main.removeEventListener("touchcancel", reset);
    main.removeEventListener("scroll", scroll);
    window.removeEventListener("keydown", key);
    window.removeEventListener("pointermove", pointerMove);
    window.removeEventListener("pointerup", pointerUp);
    window.removeEventListener("pointercancel", pointerCancel);
    window.removeEventListener("blur", reset);
  };
}
