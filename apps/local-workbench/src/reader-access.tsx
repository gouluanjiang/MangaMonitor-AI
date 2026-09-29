import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
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
  choose(request: ReaderRequest, title: string, details: () => void): void;
  read(request: ReaderRequest): void;
  available: boolean;
}
const ReaderContext = createContext<ReaderAccess>({
  choose: (_request, _title, details) => details(),
  read: () => {},
  available: false,
});
export const ReaderAccessProvider = ReaderContext.Provider;
export const useReaderAccess = () => useContext(ReaderContext);

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
}: {
  title: string;
  onRead(): void;
  onDetails(): void;
  onReadWindow(): void;
  onClose(): void;
  busy: boolean;
  error: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="reader-cover-actions"
      aria-label="打开漫画"
      data-testid="reader-cover-actions"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
      onClick={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div className="reader-cover-actions-content">
        <p title={title}>{title}</p>
        <button
          className="button secondary"
          onClick={onDetails}
          disabled={busy}
        >
          漫画详细
        </button>
        <button className="button primary" onClick={onRead} disabled={busy}>
          程序内阅读
        </button>
        <button
          className="button secondary"
          onClick={onReadWindow}
          disabled={busy}
        >
          {busy ? "正在打开小窗…" : "手机小框阅读"}
        </button>
        {error && <p role="alert">{error}</p>}
        <button className="text-button" onClick={onClose} disabled={busy}>
          取消
        </button>
      </div>
    </dialog>
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
  } | null>(null);
  const [openingWindow, setOpeningWindow] = useState(false);
  const [choiceError, setChoiceError] = useState("");
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
  const actions: ReaderAccess = {
    available: enabled,
    choose: (next, title, details) => {
      if (!enabled) return details();
      remember();
      setChoiceError("");
      setChoice({ request: next, title, details });
    },
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
          onRead={() => actions.read(choice.request)}
          onReadWindow={() => {
            if (openingWindow) return;
            setOpeningWindow(true);
            setChoiceError("");
            void invokeDesktop("reader_window_open", {
              request: choice.request,
            })
              .then(() => {
                setChoice(null);
                restore(false);
              })
              .catch((error: unknown) =>
                setChoiceError(readerErrorMessage(error)),
              )
              .finally(() => setOpeningWindow(false));
          }}
          onDetails={() => {
            setChoice(null);
            returnTo.current = null;
            choice.details();
          }}
          onClose={() => {
            setChoice(null);
            restore();
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
