import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ComicReader } from "./reader/ComicReader.tsx";
import { nativeReaderAdapter } from "./reader/runtime.ts";
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
  onClose,
}: {
  title: string;
  onRead(): void;
  onDetails(): void;
  onClose(): void;
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
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="reader-cover-actions-content">
        <p title={title}>{title}</p>
        <button className="button primary" onClick={onRead}>
          直接阅读
        </button>
        <button className="button secondary" onClick={onDetails}>
          作品详情
        </button>
        <button className="text-button" onClick={onClose}>
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
): { actions: ReaderAccess; layer: ReactNode; isOpen: boolean } {
  const [request, setRequest] = useState<ReaderRequest | null>(null);
  const [choice, setChoice] = useState<{
    request: ReaderRequest;
    title: string;
    details(): void;
  } | null>(null);
  const returnTo = useRef<{
    element: HTMLElement | null;
    main: HTMLElement | null;
    scroll: number;
  } | null>(null);
  const download = useRef(onDownload);
  download.current = onDownload;
  const remember = () => {
    if (returnTo.current) return;
    const element =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const main = element?.closest("main") ?? document.querySelector("main");
    returnTo.current = { element, main, scroll: main?.scrollTop ?? 0 };
  };
  const restore = () => {
    const previous = returnTo.current;
    returnTo.current = null;
    requestAnimationFrame(() => {
      if (previous?.main?.isConnected)
        previous.main.scrollTop = previous.scroll;
      if (previous?.element?.isConnected)
        previous.element.focus({ preventScroll: true });
    });
  };
  const actions: ReaderAccess = {
    available: enabled,
    choose: (next, title, details) => {
      if (!enabled) return details();
      remember();
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
          onRead={() => actions.read(choice.request)}
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
