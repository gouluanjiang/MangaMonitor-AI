import { useEffect, useRef, useState } from "react";
import { ComicReader } from "./ComicReader.tsx";
import type { ComicReaderHandle } from "./ComicReader.tsx";
import { nativeReaderWindowAdapter } from "./window-runtime.ts";
import type { ReaderWindowAdapter } from "./window-runtime.ts";
import { ReaderDownloadError, readerErrorMessage } from "./runtime.ts";
import type { ReaderRequest } from "./types.ts";

/** Only native context supplies the book. The URL selects this view, not access. */
export function ReaderWindow({
  adapter = nativeReaderWindowAdapter,
}: {
  adapter?: ReaderWindowAdapter;
}) {
  const [request, setRequest] = useState<ReaderRequest | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [attempt, setAttempt] = useState(0);
  const [pinned, setPinned] = useState(false);
  const [pinBusy, setPinBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const reader = useRef<ComicReaderHandle>(null);
  const pinFlight = useRef(false);
  const closing = useRef(false);
  const closeWindow = async () => {
    closing.current = true;
    try {
      await adapter.close();
    } catch (failure) {
      closing.current = false;
      setNotice("暂时无法关闭窗口，请重试。");
      throw failure;
    }
  };
  const requestClose = async () => {
    try {
      if (reader.current) await reader.current.close();
      else await closeWindow();
    } catch {
      setNotice("暂时无法关闭窗口，请重试。");
    }
  };
  const actions = useRef({ requestClose });
  actions.current = { requestClose };
  useEffect(() => {
    let cancelled = false;
    let contextRevision = 0;
    const cleanups: (() => void)[] = [];
    const refresh = async () => {
      const revision = ++contextRevision;
      try {
        const value = await adapter.context();
        if (!cancelled && !closing.current && revision === contextRevision) {
          setError(null);
          setRequest(value.request);
        }
      } catch (failure) {
        if (!cancelled && revision === contextRevision) setError(failure);
      }
    };
    const subscribe = async (name: string, callback: () => void) => {
      const unlisten = await adapter.listen(name, callback);
      if (cancelled) unlisten();
      else cleanups.push(unlisten);
    };
    void (async () => {
      try {
        await subscribe("reader-window-close-requested", () => {
          void actions.current.requestClose();
        });
        await subscribe("reader-window-context-changed", () => {
          void refresh();
        });
        if (!cancelled) await refresh();
      } catch (failure) {
        if (!cancelled) setError(failure);
      }
    })();
    return () => {
      cancelled = true;
      cleanups.forEach((unlisten) => unlisten());
    };
  }, [adapter, attempt]);
  const showMain = () => {
    void adapter
      .showMain()
      .catch(() => setNotice("暂时无法显示主界面，请重试。"));
  };
  const pin = async (value: boolean) => {
    if (pinFlight.current) return;
    pinFlight.current = true;
    setPinBusy(true);
    try {
      await adapter.pin(value);
      setPinned(value);
      setNotice("");
    } catch {
      setNotice("暂时无法更改置顶状态，请重试。");
    } finally {
      pinFlight.current = false;
      setPinBusy(false);
    }
  };
  return request ? (
    <ComicReader
      ref={reader}
      request={request}
      onClose={closeWindow}
      onDownload={async (_source, readerId) => {
        try {
          await adapter.download(readerId);
        } catch {
          throw new ReaderDownloadError("暂时无法在主界面准备下载，请重试。");
        }
      }}
      windowControls={{
        pinned,
        pinBusy,
        notice: notice || (error ? readerErrorMessage(error) : ""),
        onPinnedChange: (value) => {
          void pin(value);
        },
        onShowMain: showMain,
      }}
    />
  ) : (
    <section
      className="comic-reader"
      aria-label="独立阅读窗口"
      data-testid="reader-window-loading"
    >
      <div className="reader-message" role="status">
        <p>{error ? readerErrorMessage(error) : "正在打开阅读窗口…"}</p>
        {notice && <p role="alert">{notice}</p>}
        {Boolean(error) && (
          <button onClick={() => setAttempt((value) => value + 1)}>
            重试打开
          </button>
        )}
        <button aria-label="显示主界面" onClick={showMain}>
          主界面
        </button>
        <button
          onClick={() => {
            void requestClose();
          }}
        >
          关闭阅读窗口
        </button>
      </div>
    </section>
  );
}
