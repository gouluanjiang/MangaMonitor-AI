import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

const easeOut = "cubic-bezier(.23,1,.32,1)";
const immediate = () =>
  document.body.dataset.uiInput === "keyboard" ||
  document.body.dataset.uiReducedMotion === "true" ||
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** One input policy for the shell; independently opened readers keep their own controls. */
export function useUiMotion(reduced: boolean) {
  useEffect(() => {
    document.body.dataset.uiRefined = "true";
    const pointer = () => {
      document.body.dataset.uiInput = "pointer";
    };
    const keyboard = (event: KeyboardEvent) => {
      if (!event.metaKey && !event.ctrlKey && !event.altKey)
        document.body.dataset.uiInput = "keyboard";
    };
    document.addEventListener("pointerdown", pointer, true);
    document.addEventListener("keydown", keyboard, true);
    return () => {
      document.removeEventListener("pointerdown", pointer, true);
      document.removeEventListener("keydown", keyboard, true);
      delete document.body.dataset.uiRefined;
      delete document.body.dataset.uiInput;
      delete document.body.dataset.uiReducedMotion;
    };
  }, []);
  useLayoutEffect(() => {
    document.body.dataset.uiReducedMotion = String(reduced);
  }, [reduced]);
}

type Run = {
  sidebar: HTMLElement;
  originalWidth: string;
  fromWidth: number;
  toWidth: number;
  animations: Animation[];
};
/** The grid lays out once. Only the sidebar clip and workspace translation interpolate. */
export function useSidebarMotion(reduced: boolean) {
  const [collapsed, setCollapsed] = useState(false);
  const sidebarRef = useRef<HTMLElement>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const run = useRef<Run | null>(null);
  const from = useRef<{ width: number; left: number } | null>(null);
  const settle = useCallback(() => {
    const previous = run.current;
    run.current = null;
    if (!previous) return;
    previous.animations.forEach((animation) => animation.cancel());
    previous.sidebar.style.width = previous.originalWidth;
    previous.sidebar
      .closest(".app-shell")
      ?.classList.remove("ui-sidebar-moving");
  }, []);
  const toggle = () => {
    const sidebar = sidebarRef.current,
      workspace = workspaceRef.current;
    if (!sidebar || !workspace) return;
    const left = workspace.getBoundingClientRect().left;
    let width = sidebar.getBoundingClientRect().width;
    if (run.current) {
      const previous = run.current;
      const progress =
        previous.animations[0].effect?.getComputedTiming().progress ?? 0;
      width =
        previous.fromWidth + (previous.toWidth - previous.fromWidth) * progress;
    }
    settle();
    from.current = { width, left };
    setCollapsed((value) => !value);
  };
  useLayoutEffect(() => {
    const start = from.current;
    from.current = null;
    const sidebar = sidebarRef.current,
      workspace = workspaceRef.current;
    if (
      !start ||
      !sidebar ||
      !workspace ||
      immediate() ||
      reduced ||
      window.matchMedia("(max-width: 600px)").matches ||
      typeof sidebar.animate !== "function"
    )
      return;
    const width = sidebar.getBoundingClientRect().width,
      left = workspace.getBoundingClientRect().left;
    if (Math.abs(start.width - width) < 0.5) return;
    const paintWidth = Math.max(start.width, width),
      originalWidth = sidebar.style.width;
    sidebar.style.width = `${paintWidth}px`;
    sidebar.closest(".app-shell")?.classList.add("ui-sidebar-moving");
    const timing: KeyframeAnimationOptions = {
      duration: 260,
      easing: easeOut,
      fill: "both",
    };
    const animations = [
      sidebar.animate(
        [
          { clipPath: `inset(0 ${paintWidth - start.width}px 0 0)` },
          { clipPath: `inset(0 ${paintWidth - width}px 0 0)` },
        ],
        timing,
      ),
      workspace.animate(
        [
          { transform: `translateX(${start.left - left}px)` },
          { transform: "translateX(0)" },
        ],
        timing,
      ),
    ];
    const current = {
      sidebar,
      originalWidth,
      fromWidth: start.width,
      toWidth: width,
      animations,
    };
    run.current = current;
    void Promise.all(animations.map((animation) => animation.finished))
      .then(() => {
        if (run.current === current) settle();
      })
      .catch(() => {
        /* Cancellation preserves the latest React state. */
      });
  }, [collapsed, reduced, settle]);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const change = () => {
      if (media.matches) settle();
    };
    const key = () => settle();
    window.addEventListener("resize", settle, { passive: true });
    document.addEventListener("keydown", key, true);
    media.addEventListener("change", change);
    return () => {
      window.removeEventListener("resize", settle);
      document.removeEventListener("keydown", key, true);
      media.removeEventListener("change", change);
      settle();
    };
  }, [settle]);
  useEffect(() => {
    if (reduced) settle();
  }, [reduced, settle]);
  return { collapsed, toggle, sidebarRef, workspaceRef };
}
