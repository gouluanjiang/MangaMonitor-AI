import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { ComicReader, type ComicReaderHandle } from "./reader/ComicReader.tsx";
import {
  nativeReaderAdapter,
  ReaderDownloadError,
  readerErrorMessage,
} from "./reader/runtime.ts";
import {
  listenReaderEvent,
  supportsReaderWindowEvents,
} from "./reader/window-runtime.ts";
import { invokeDesktop } from "./runtime.ts";
import type { ReaderRequest } from "./reader/types.ts";
import type { WorkReference } from "./booklists.ts";
import type { SourceScope, SourceWork } from "./source-types.ts";
import "./reader-access.css";

interface ReaderAccess {
  choose(
    request: ReaderRequest,
    title: string,
    details: () => void,
    point?: { x: number; y: number; keyboard?: boolean },
  ): void;
  read(request: ReaderRequest): void;
  readWindow(request: ReaderRequest): void;
  available: boolean;
}
const ReaderContext = createContext<ReaderAccess>({
  choose: (_request, _title, details) => details(),
  read: () => {},
  readWindow: () => {},
  available: false,
});
export const ReaderAccessProvider = ReaderContext.Provider;
export const useReaderAccess = () => useContext(ReaderContext);

/** A single pointer click focuses; it must never race a double click into details. */
export function CoverInteraction({
  request,
  title,
  onDetails,
  selectionMode = false,
  selected = false,
  onToggleSelection,
  children,
  className = "source-cover-button",
  testId,
}: {
  request: ReaderRequest | null;
  title: string;
  onDetails(): void;
  selectionMode?: boolean;
  selected?: boolean;
  onToggleSelection?(): void;
  children: ReactNode;
  className?: string;
  testId?: string;
}) {
  const access = useReaderAccess();
  const menu = (
    element: HTMLElement,
    x: number,
    y: number,
    keyboard = false,
  ) => {
    element.focus({ preventScroll: true });
    if (request && access.available)
      access.choose(request, title, onDetails, { x, y, keyboard });
    else onDetails();
  };
  return (
    <div
      role="button"
      tabIndex={0}
      className={`${className} cover-interaction`}
      data-testid={testId}
      aria-label={`打开《${title}》`}
      aria-haspopup={selectionMode ? undefined : "menu"}
      aria-pressed={selectionMode ? selected : undefined}
      onClick={(event) => {
        if ((event.target as HTMLElement).closest("button, input, a")) return;
        event.currentTarget.focus({ preventScroll: true });
        if (selectionMode && event.detail < 2) onToggleSelection?.();
      }}
      onDoubleClick={(event) => {
        if (
          selectionMode ||
          (event.target as HTMLElement).closest("button, input, a")
        )
          return;
        event.preventDefault();
        if (request && access.available) access.readWindow(request);
      }}
      onContextMenu={(event) => {
        if (
          selectionMode ||
          (event.target as HTMLElement).closest("button, input, a")
        )
          return;
        event.preventDefault();
        menu(event.currentTarget, event.clientX, event.clientY);
      }}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (selectionMode && (event.key === " " || event.key === "Enter")) {
          event.preventDefault();
          onToggleSelection?.();
          return;
        }
        if (
          !selectionMode &&
          (event.key === "ContextMenu" ||
            (event.key === "F10" && event.shiftKey) ||
            event.key === " ")
        ) {
          event.preventDefault();
          const rect = event.currentTarget.getBoundingClientRect();
          menu(event.currentTarget, rect.left + 12, rect.top + 12, true);
        } else if (!selectionMode && event.key === "Enter") {
          event.preventDefault();
          if (request && access.available) access.readWindow(request);
        }
      }}
    >
      {children}
    </div>
  );
}

export function sourceReaderRequest(
  scope: SourceScope,
  work: Pick<SourceWork, "source" | "workId">,
): ReaderRequest {
  return {
    kind: "source",
    source: work.source,
    sessionId: scope.sessionId,
    workId: work.workId,
  };
}

function CoverActions({
  title,
  onRead,
  onDetails,
  onReadWindow,
  onClose,
  busy,
  error,
  point,
}: {
  title: string;
  onRead(): void;
  onDetails(): void;
  onReadWindow(): void;
  onClose(): void;
  busy: boolean;
  error: string;
  point: { x: number; y: number; keyboard?: boolean };
}) {
  const menu = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useLayoutEffect(() => {
    const element = menu.current;
    if (!element) return;
    element.style.left =
      Math.max(
        8,
        Math.min(point.x, window.innerWidth - element.offsetWidth - 8),
      ) + "px";
    element.style.top =
      Math.max(
        8,
        Math.min(point.y, window.innerHeight - element.offsetHeight - 8),
      ) + "px";
    if (point.keyboard)
      element
        .querySelector<HTMLButtonElement>("[role=menuitem]")
        ?.focus({ preventScroll: true });
  }, [point]);
  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node)) close.current();
    };
    const dismiss = () => close.current();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape" || event.key === "Tab") {
        if (event.key === "Escape") event.preventDefault();
        close.current();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      const items = Array.from(
        menu.current?.querySelectorAll<HTMLButtonElement>(
          "[role=menuitem]:not(:disabled)",
        ) ?? [],
      );
      if (!items.length) return;
      event.preventDefault();
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? items.length - 1
            : event.key === "ArrowDown"
              ? (index + 1) % items.length
              : (index - 1 + items.length) % items.length;
      items[next].focus({ preventScroll: true });
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", keyboard, true);
    window.addEventListener("resize", dismiss);
    document
      .querySelector("main")
      ?.addEventListener("scroll", dismiss, { passive: true });
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", keyboard, true);
      window.removeEventListener("resize", dismiss);
      document.querySelector("main")?.removeEventListener("scroll", dismiss);
    };
  }, []);
  return createPortal(
    <div
      ref={menu}
      className="reader-cover-actions"
      role="menu"
      aria-label="打开漫画"
      data-testid="reader-cover-actions"
      data-keyboard={point.keyboard || undefined}
      style={{ left: point.x, top: point.y }}
      onContextMenu={(event) => event.preventDefault()}
    >
      <div className="reader-cover-actions-content">
        <p title={title}>{title}</p>
        <button role="menuitem" onClick={onDetails} disabled={busy}>
          作品详细
        </button>
        <button role="menuitem" onClick={onRead} disabled={busy}>
          程序内阅读
        </button>
        <button role="menuitem" onClick={onReadWindow} disabled={busy}>
          {busy ? "正在打开小窗…" : "小窗阅读"}
        </button>
        {error && <p role="alert">{error}</p>}
      </div>
    </div>,
    document.body,
  );
}

/** Keep the existing page mounted so closing a book preserves its list position. */
export function useReaderHost(
  enabled: boolean,
  onDownload: (reference: WorkReference) => Promise<void>,
  onProblem: (message: string) => void,
): { actions: ReaderAccess; layer: ReactNode; isOpen: boolean } {
  const [request, setRequest] = useState<ReaderRequest | null>(null);
  const [choice, setChoice] = useState<{
    request: ReaderRequest;
    title: string;
    details(): void;
    point: { x: number; y: number; keyboard?: boolean };
  } | null>(null);
  const [openingWindow, setOpeningWindow] = useState(false);
  const [choiceError, setChoiceError] = useState("");
  const currentChoice = useRef(choice);
  currentChoice.current = choice;
  const pendingWindows = useRef(new Set<string>());
  const reader = useRef<ComicReaderHandle>(null);
  const returnTo = useRef<{
    element: HTMLElement | null;
    main: HTMLElement | null;
    scroll: number;
  } | null>(null);
  const download = useRef(onDownload);
  download.current = onDownload;
  const report = useRef(onProblem);
  report.current = onProblem;
  useEffect(() => {
    if (!enabled || !supportsReaderWindowEvents()) return;
    let stopped = false;
    let closing = false;
    const unlisten: (() => void)[] = [];
    const install = async () => {
      unlisten.push(
        await listenReaderEvent("reader-main-close-requested", () => {
          if (stopped || closing) return;
          closing = true;
          setChoice(null);
          void (async () => {
            await reader.current?.close();
            await invokeDesktop("reader_main_close");
          })()
            .catch(() => report.current("暂时无法关闭主窗口，请稍后再试。"))
            .finally(() => {
              closing = false;
            });
        }),
      );
      if (stopped) return;
      unlisten.push(
        await listenReaderEvent<WorkReference>(
          "reader-window-download",
          (reference) => {
            if (
              stopped ||
              !reference ||
              (reference.source !== "JM" && reference.source !== "Pica") ||
              typeof reference.workId !== "string" ||
              !reference.workId.length ||
              reference.workId.length > 128
            )
              return;
            void download.current(reference).catch((error: unknown) => {
              report.current(
                error instanceof ReaderDownloadError
                  ? error.message
                  : "暂时无法准备下载，请在下载队列查看状态。",
              );
            });
          },
        ),
      );
      if (!stopped) await invokeDesktop("reader_main_ready");
    };
    void install()
      .catch(() => {
        if (!stopped)
          report.current("阅读窗口控制暂时无法连接，请重新打开程序。");
      })
      .finally(() => {
        if (stopped) unlisten.splice(0).forEach((stop) => stop());
      });
    return () => {
      stopped = true;
      unlisten.splice(0).forEach((stop) => stop());
    };
  }, [enabled]);
  const remember = () => {
    if (returnTo.current) return;
    const element =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const main = element?.closest("main") ?? document.querySelector("main");
    returnTo.current = { element, main, scroll: main?.scrollTop ?? 0 };
  };
  const restore = (focus = true) => {
    const previous = returnTo.current;
    returnTo.current = null;
    requestAnimationFrame(() => {
      if (previous?.main?.isConnected)
        previous.main.scrollTop = previous.scroll;
      if (focus && previous?.element?.isConnected)
        previous.element.focus({ preventScroll: true });
    });
  };
  const openWindow = (next: ReaderRequest, fromMenu = false) => {
    const key = JSON.stringify(next);
    if (!enabled || pendingWindows.current.has(key)) return;
    pendingWindows.current.add(key);
    const isCurrent = () =>
      currentChoice.current &&
      JSON.stringify(currentChoice.current.request) === key;
    if (fromMenu) {
      setOpeningWindow(true);
      setChoiceError("");
    }
    void invokeDesktop("reader_window_open", { request: next })
      .then(() => {
        if (fromMenu && isCurrent()) {
          setChoice(null);
          returnTo.current = null;
        }
      })
      .catch((error: unknown) => {
        if (fromMenu && isCurrent()) setChoiceError(readerErrorMessage(error));
        else report.current(readerErrorMessage(error));
      })
      .finally(() => {
        pendingWindows.current.delete(key);
        if (!currentChoice.current || isCurrent()) setOpeningWindow(false);
      });
  };
  const actions: ReaderAccess = {
    available: enabled,
    choose: (next, title, details, point) => {
      if (!enabled) return details();
      remember();
      setChoiceError("");
      setOpeningWindow(pendingWindows.current.has(JSON.stringify(next)));
      const rect = document.activeElement?.getBoundingClientRect();
      setChoice({
        request: next,
        title,
        details,
        point: point ?? {
          x: rect?.left ?? 24,
          y: rect?.top ?? 24,
          keyboard: true,
        },
      });
    },
    readWindow: (next) => openWindow(next),
    read: (next) => {
      if (!enabled) return;
      remember();
      setChoice(null);
      setRequest(next);
    },
  };
  const layer = (
    <>
      {choice && (
        <CoverActions
          title={choice.title}
          busy={openingWindow}
          error={choiceError}
          point={choice.point}
          onRead={() => actions.read(choice.request)}
          onReadWindow={() => openWindow(choice.request, true)}
          onDetails={() => {
            setChoice(null);
            returnTo.current = null;
            choice.details();
          }}
          onClose={() => {
            setChoice(null);
            const previous = returnTo.current;
            returnTo.current = null;
            if (previous?.element?.isConnected)
              previous.element.focus({ preventScroll: true });
          }}
        />
      )}
      {request && (
        <ComicReader
          ref={reader}
          request={request}
          adapter={nativeReaderAdapter}
          onClose={() => {
            setRequest(null);
            restore();
          }}
          onDownload={(reference) => download.current(reference)}
        />
      )}
    </>
  );
  return { actions, layer, isOpen: request !== null };
}
