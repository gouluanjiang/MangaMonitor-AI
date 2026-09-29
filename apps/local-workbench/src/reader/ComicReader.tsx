import { useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Ref } from "react";
import type {
  ReaderAdapter,
  ReaderBook,
  ReaderRequest,
  ReaderSourceRef,
  ReaderWindowControls,
} from "./types.ts";
import { nativeReaderAdapter, readerErrorMessage } from "./runtime.ts";
import { ReaderLifetime, ReaderProgressWriter } from "./model.ts";
import { ReaderSession } from "./ReaderSession.tsx";
import "./reader.css";
export type ComicReaderProps = {
  ref?: Ref<ComicReaderHandle>;
  request: ReaderRequest;
  adapter?: ReaderAdapter;
  onClose: () => void | Promise<void>;
  onDownload?: (
    sourceRef: ReaderSourceRef,
    readerId: string,
  ) => void | Promise<void>;
  windowControls?: ReaderWindowControls;
};
export type ComicReaderHandle = { close(): Promise<void> };
export type ReaderSessionState = {
  book: ReaderBook;
  writer: ReaderProgressWriter;
  close: () => Promise<void>;
};

export function ComicReader({
  ref,
  request,
  adapter = nativeReaderAdapter,
  onClose,
  onDownload,
  windowControls,
}: ComicReaderProps) {
  const key = JSON.stringify(request);
  const [loaded, setLoaded] = useState<{
    key: string;
    session: ReaderSessionState;
  } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [attempt, setAttempt] = useState(0);
  const [closing, setClosing] = useState(false);
  const [closeError, setCloseError] = useState("");
  const lifetimeRef = useRef<ReaderLifetime | null>(null);
  const closePromise = useRef<Promise<void> | null>(null);
  const callbacks = useRef({ onClose, onDownload });
  callbacks.current = { onClose, onDownload };
  useEffect(() => {
    let cancelled = false;
    const requestId = crypto.randomUUID();
    const lifetime = new ReaderLifetime(() => adapter.cancelOpen(requestId));
    lifetimeRef.current = lifetime;
    closePromise.current = null;
    setLoaded(null);
    setError(null);
    setClosing(false);
    setCloseError("");
    const previousFocus = document.activeElement;
    void adapter
      .open(request, requestId)
      .then((book) => {
        const writer = new ReaderProgressWriter((position) =>
          adapter.savePosition(book.readerId, position),
        );
        let closePromise: Promise<void> | null = null;
        const opened = {
          book,
          writer,
          close: () =>
            (closePromise ??= writer
              .flush()
              .catch(() => undefined)
              .then(() => adapter.close(book.readerId))
              .catch(() => undefined)),
        };
        if (lifetime.attach(opened.close) && !cancelled)
          setLoaded({ key, session: opened });
      })
      .catch((failure: unknown) => {
        if (!cancelled) setError(failure);
      });
    return () => {
      cancelled = true;
      void lifetime.close().catch(() => undefined);
      if (lifetimeRef.current === lifetime) lifetimeRef.current = null;
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected)
        previousFocus.focus({ preventScroll: true });
    };
  }, [adapter, key, attempt]);
  const session = loaded?.key === key ? loaded.session : null;
  const close = (): Promise<void> => {
    if (closePromise.current) return closePromise.current;
    setClosing(true);
    const pending = (async () => {
      try {
        await lifetimeRef.current?.close();
        await callbacks.current.onClose();
      } catch (failure) {
        setCloseError("暂时无法关闭阅读器，请重试。");
        setClosing(false);
        throw failure;
      }
    })();
    closePromise.current = pending;
    void pending.catch(() => {
      if (closePromise.current === pending) closePromise.current = null;
    });
    return pending;
  };
  useImperativeHandle(ref, () => ({ close }));
  const requestClose = () => {
    void close().catch(() => undefined);
  };
  return (
    <section
      className="comic-reader"
      role="dialog"
      aria-modal="true"
      aria-label="漫画阅读器"
      data-testid="comic-reader"
    >
      {closeError && (
        <div className="reader-close-error" role="alert">
          {closeError}
        </div>
      )}
      {session ? (
        <ReaderSession
          key={session.book.readerId}
          session={session}
          adapter={adapter}
          onClose={requestClose}
          onDownload={onDownload}
          closing={closing}
          windowControls={windowControls}
        />
      ) : (
        <div className="reader-message" role="status">
          <p>{error ? readerErrorMessage(error) : "正在打开漫画…"}</p>
          {Boolean(error) && (
            <button onClick={() => setAttempt((n) => n + 1)}>重试打开</button>
          )}
          {windowControls && (
            <button aria-label="显示主界面" onClick={windowControls.onShowMain}>
              主界面
            </button>
          )}
          <button onClick={requestClose} disabled={closing}>
            {closing ? "正在返回…" : "返回"}
          </button>
        </div>
      )}
    </section>
  );
}
