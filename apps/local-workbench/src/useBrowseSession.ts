import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { Dispatch, RefObject, SetStateAction } from "react";
import type { SourceGridHandle } from "./VirtualSourceGrid.tsx";
import {
  readBrowsePosition,
  readBrowseState,
  resolveBrowseAnchor,
  saveBrowsePosition,
  saveBrowseState,
  forgetBrowseOwner,
  browseSessionKeys,
  browseSessionsCurrent,
} from "./browse-session.ts";
import type { BrowsePosition } from "./browse-session.ts";
import type { SourceScope } from "./source-types.ts";

const noSessions: readonly SourceScope[] = [];

export function useBrowseSessionState<T>(
  scope: string,
  initial: T | (() => T),
  sessions: readonly SourceScope[] = noSessions,
): [T, Dispatch<SetStateAction<T>>] {
  const sessionKeys = browseSessionKeys(sessions);
  const sessionKey = JSON.stringify(sessionKeys);
  const makeInitial = () =>
    typeof initial === "function" ? (initial as () => T)() : initial;
  const [state, setState] = useState(() => ({
    scope,
    value: readBrowseState(scope, makeInitial, { sessions: sessionKeys }),
  }));
  const value =
    state.scope === scope
      ? state.value
      : readBrowseState(scope, makeInitial, { sessions: sessionKeys });
  const valueRef = useRef(value);
  valueRef.current = value;
  const update = useCallback<Dispatch<SetStateAction<T>>>(
    (next) => {
      const value =
        typeof next === "function"
          ? (next as (old: T) => T)(valueRef.current)
          : next;
      valueRef.current = value;
      saveBrowseState(scope, value, { sessions: sessionKeys });
      setState({ scope, value });
    },
    [scope, sessionKey],
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
  retainOnUnmount = true,
  sessions = noSessions,
  root,
  grid,
  itemKeys,
}: {
  scope: string;
  active: boolean;
  enabled?: boolean;
  retainOnUnmount?: boolean;
  sessions?: readonly SourceScope[];
  root: RefObject<HTMLElement | null>;
  grid?: RefObject<SourceGridHandle | null>;
  itemKeys: readonly string[];
}): void {
  const owner = useRef(Symbol("browse-list"));
  const sessionKeys = browseSessionKeys(sessions);
  const sessionKey = JSON.stringify(sessionKeys);
  useEffect(() => {
    if (retainOnUnmount) return;
    const owned = owner.current;
    // Passive unmount cleanup follows the layout cleanup below, which saves
    // the final position. Deleting earlier would let that save resurrect it.
    return () => {
      forgetBrowseOwner(owned);
    };
  }, [retainOnUnmount]);
  const live = useRef({ active, enabled, itemKeys });
  live.current = { active, enabled, itemKeys };
  const last = useRef<BrowsePosition | undefined>(undefined);
  const lastScope = useRef(scope);
  const previousItems = useRef(itemKeys);
  const restoring = useRef(false);
  const waitingForItems = useRef(false);
  const restoreFrame = useRef(0);
  const capture = useRef<() => void>(() => {});
  const restore = useRef<(position: BrowsePosition) => void>(() => {});
  useLayoutEffect(() => {
    last.current = readBrowsePosition(
      scope,
      last.current
        ? { scope: lastScope.current, position: last.current }
        : undefined,
      sessionKeys,
    );
    lastScope.current = scope;
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
      if (
        restoring.current ||
        !live.current.active ||
        !live.current.enabled ||
        !browseSessionsCurrent(sessionKeys)
      )
        return;
      // Hidden panels share one scroll container and must never capture another page.
      if (!root.current?.getClientRects().length) return;
      const next = {
        anchor: grid?.current?.capture() ?? domAnchor(),
        keys: live.current.itemKeys,
        scroll: main.scrollTop,
      };
      last.current = saveBrowsePosition(scope, next, {
        owner: owner.current,
        sessions: sessionKeys,
      });
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
        // A remounted grid can still be waiting for its real row measurement
        // and committed extent. Do not replace its saved anchor with a clamp.
        if (++settling < 6 || grid?.current?.isRestoring())
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
      if (last.current)
        saveBrowsePosition(scope, last.current, {
          owner: owner.current,
          sessions: sessionKeys,
        });
      capture.current = () => {};
      restore.current = () => {};
      main.removeEventListener("scroll", schedule);
      main.removeEventListener("wheel", interrupt);
      main.removeEventListener("pointerdown", interrupt);
      main.removeEventListener("touchstart", interrupt);
    };
  }, [scope, active, enabled, root, grid, retainOnUnmount, sessionKey]);
  useLayoutEffect(() => {
    const before = previousItems.current;
    previousItems.current = itemKeys;
    if (!active || !enabled) return;
    const previous = last.current;
    if (
      previous &&
      (waitingForItems.current ||
        before.length !== itemKeys.length ||
        before.some((key, index) => key !== itemKeys[index]))
    )
      restore.current(previous);
    else if (!restoring.current) capture.current();
  }, [scope, active, enabled, itemKeys]);
}
