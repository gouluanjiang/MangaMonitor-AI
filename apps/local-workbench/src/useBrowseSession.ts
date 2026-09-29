import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { Dispatch, RefObject, SetStateAction } from "react";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import {
  readBrowsePosition,
  readBrowseState,
  resolveBrowseAnchor,
  saveBrowsePosition,
  saveBrowseState,
} from "./browse-session.ts";
import type { BrowsePosition } from "./browse-session.ts";

export function useBrowseSessionState<T>(
  scope: string,
  initial: T | (() => T),
): [T, Dispatch<SetStateAction<T>>] {
  const makeInitial = () =>
    typeof initial === "function" ? (initial as () => T)() : initial;
  const [state, setState] = useState(() => ({
    scope,
    value: readBrowseState(scope, makeInitial),
  }));
  const value =
    state.scope === scope ? state.value : readBrowseState(scope, makeInitial);
  const valueRef = useRef(value);
  valueRef.current = value;
  const update = useCallback<Dispatch<SetStateAction<T>>>(
    (next) => {
      const value =
        typeof next === "function"
          ? (next as (old: T) => T)(valueRef.current)
          : next;
      valueRef.current = value;
      saveBrowseState(scope, value);
      setState({ scope, value });
    },
    [scope],
  );
  return [value, update];
}

/** Mount on every manga list, including lists that remain mounted while hidden.
 * `enabled=false` suspends list capture while its detail view is open.
 * The scope identifies page/source/query; item changes preserve a work anchor.
 */
export function useBrowseSession({
  scope,
  active,
  enabled = true,
  root,
  grid,
  itemKeys,
}: {
  scope: string;
  active: boolean;
  enabled?: boolean;
  root: RefObject<HTMLElement | null>;
  grid?: RefObject<SourceGridHandle | null>;
  itemKeys: readonly string[];
}): void {
  const live = useRef({ active, enabled, itemKeys });
  live.current = { active, enabled, itemKeys };
  const last = useRef<BrowsePosition | undefined>(undefined);
  const restoring = useRef(false);
  const waitingForItems = useRef(false);
  const restoreFrame = useRef(0);
  const capture = useRef<() => void>(() => {});
  const restore = useRef<(position: BrowsePosition) => void>(() => {});
  useLayoutEffect(() => {
    last.current = readBrowsePosition(scope);
    const main = root.current?.closest("main");
    if (!main || !active || !enabled) return;
    // The grid settles its own anchor across several frames. Those frames must
    // not continue against the items of a different source, filter or sort.
    const activeGrid = grid?.current;
    activeGrid?.restore(null);
    let frame = 0;
    const domAnchor = () => {
      const top = main.getBoundingClientRect().top;
      const entries = Array.from(
        root.current?.querySelectorAll<HTMLElement>("[data-browse-key]") ?? [],
      );
      const entry =
        entries.find(
          (element) => element.getBoundingClientRect().bottom > top,
        ) ?? entries.at(-1);
      return entry
        ? {
            key: entry.dataset.browseKey!,
            offset: entry.getBoundingClientRect().top - top,
          }
        : null;
    };
    const remember = () => {
      if (restoring.current || !live.current.active || !live.current.enabled)
        return;
      // Hidden panels share one scroll container and must never capture another page.
      if (!root.current?.getClientRects().length) return;
      const next = {
        anchor: grid?.current?.capture() ?? domAnchor(),
        keys: live.current.itemKeys,
        scroll: main.scrollTop,
      };
      last.current = next;
      saveBrowsePosition(scope, next);
    };
    capture.current = remember;
    const schedule = () => {
      if (!frame)
        frame = requestAnimationFrame(() => {
          frame = 0;
          remember();
        });
    };
    const apply = (position: BrowsePosition) => {
      restoring.current = true;
      cancelAnimationFrame(restoreFrame.current);
      grid?.current?.restore(null);
      // A stored list can be awaiting its cached items. Preserve its anchor until
      // that data arrives instead of replacing it with an empty-list position.
      waitingForItems.current =
        !live.current.itemKeys.length && position.keys.length > 0;
      if (waitingForItems.current) return;
      const anchor = resolveBrowseAnchor(position, live.current.itemKeys);
      if (anchor && grid?.current) grid.current.restore(anchor);
      else if (anchor) {
        const entry = Array.from(
          root.current?.querySelectorAll<HTMLElement>("[data-browse-key]") ??
            [],
        ).find((element) => element.dataset.browseKey === anchor.key);
        if (entry)
          main.scrollTop +=
            entry.getBoundingClientRect().top -
            main.getBoundingClientRect().top -
            anchor.offset;
        else main.scrollTop = 0;
      } else main.scrollTop = position.anchor ? 0 : position.scroll;
      let settling = 0;
      const settle = () => {
        if (++settling < 6)
          restoreFrame.current = requestAnimationFrame(settle);
        else {
          restoring.current = false;
          remember();
        }
      };
      restoreFrame.current = requestAnimationFrame(settle);
    };
    restore.current = apply;
    const interrupt = () => {
      cancelAnimationFrame(restoreFrame.current);
      restoring.current = false;
      waitingForItems.current = false;
      remember();
      schedule();
    };
    main.addEventListener("scroll", schedule, { passive: true });
    main.addEventListener("wheel", interrupt, { passive: true });
    main.addEventListener("pointerdown", interrupt, { passive: true });
    main.addEventListener("touchstart", interrupt, { passive: true });
    if (last.current) apply(last.current);
    else {
      main.scrollTop = 0;
      schedule();
    }
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(restoreFrame.current);
      activeGrid?.restore(null);
      restoring.current = false;
      waitingForItems.current = false;
      // Last visible snapshot survives unmount and hidden/detail transitions.
      if (last.current) saveBrowsePosition(scope, last.current);
      capture.current = () => {};
      restore.current = () => {};
      main.removeEventListener("scroll", schedule);
      main.removeEventListener("wheel", interrupt);
      main.removeEventListener("pointerdown", interrupt);
      main.removeEventListener("touchstart", interrupt);
    };
  }, [scope, active, enabled, root, grid]);
  useLayoutEffect(() => {
    if (!active || !enabled) return;
    const previous = last.current;
    if (
      previous &&
      (waitingForItems.current ||
        previous.keys.length !== itemKeys.length ||
        previous.keys.some((key, index) => key !== itemKeys[index]))
    )
      restore.current(previous);
    else if (!restoring.current) capture.current();
  }, [scope, active, enabled, itemKeys]);
}
