import { useEffect, useRef, useState } from "react";
import type {
  ReaderAdapter,
  ReaderBook,
  ReaderRequest,
  ReaderSourceRef,
} from "./types.ts";
import { nativeReaderAdapter, readerErrorMessage } from "./runtime.ts";
import { ReaderProgressWriter } from "./model.ts";
import { ReaderSession } from "./ReaderSession.tsx";
import "./reader.css";
export type ComicReaderProps = {
  request: ReaderRequest;
  adapter?: ReaderAdapter;
  onClose: () => void;
  onDownload?: (sourceRef: ReaderSourceRef) => void | Promise<void>;
};
export type ReaderSessionState = {
  book: ReaderBook;
  writer: ReaderProgressWriter;
  close: () => Promise<void>;
};

export function ComicReader({
  request,
  adapter = nativeReaderAdapter,
  onClose,
  onDownload,
}: ComicReaderProps) {
  const key = JSON.stringify(request);
  const [loaded, setLoaded] = useState<{
    key: string;
    session: ReaderSessionState;
  } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [attempt, setAttempt] = useState(0);
  const [closing, setClosing] = useState(false);
  const callbacks = useRef({ onClose, onDownload });
  callbacks.current = { onClose, onDownload };
  useEffect(() => {
    let cancelled = false;
    let opened: ReaderSessionState | null = null;
    const requestId = crypto.randomUUID();
    setLoaded(null);
    setError(null);
    setClosing(false);
    const previousFocus = document.activeElement;
    void adapter
      .open(request, requestId)
      .then((book) => {
        const writer = new ReaderProgressWriter((position) =>
          adapter.savePosition(book.readerId, position),
        );
        let closePromise: Promise<void> | null = null;
        opened = {
          book,
          writer,
          close: () =>
            (closePromise ??= writer
              .flush()
              .catch(() => undefined)
              .then(() => adapter.close(book.readerId))
              .catch(() => undefined)),
        };
        if (cancelled) void opened.close();
        else setLoaded({ key, session: opened });
      })
      .catch((failure: unknown) => {
        if (!cancelled) setError(failure);
      });
    return () => {
      cancelled = true;
      if (opened) void opened.close();
      else void adapter.cancelOpen(requestId).catch(() => undefined);
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected)
        previousFocus.focus({ preventScroll: true });
    };
  }, [adapter, key, attempt]);
  const session = loaded?.key === key ? loaded.session : null;
  const close = async () => {
    if (closing) return;
    setClosing(true);
    if (session) await session.close();
    callbacks.current.onClose();
  };
  return (
    <section
      className="comic-reader"
      role="dialog"
      aria-modal="true"
      aria-label="漫画阅读器"
      data-testid="comic-reader"
    >
      {session ? (
        <ReaderSession
          key={session.book.readerId}
          session={session}
          adapter={adapter}
          onClose={close}
          onDownload={onDownload}
          closing={closing}
        />
      ) : (
        <div className="reader-message" role="status">
          <p>{error ? readerErrorMessage(error) : "正在打开漫画…"}</p>
          {Boolean(error) && (
            <button onClick={() => setAttempt((n) => n + 1)}>重试打开</button>
          )}
          <button onClick={close}>返回</button>
        </div>
      )}
    </section>
  );
}
