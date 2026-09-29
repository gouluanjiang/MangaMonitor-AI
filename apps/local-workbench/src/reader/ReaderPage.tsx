import type { CSSProperties } from "react";
import type { CachedPage } from "./cache.ts";
import { readerErrorMessage } from "./runtime.ts";

/** One visible page; fetching and bounded retention belong to ReaderSession. */
export function ReaderPage({
  index,
  entry,
  style,
  statusStyle,
  onDecodeError,
  onRetry,
}: {
  index: number;
  entry: CachedPage | undefined;
  style: CSSProperties;
  statusStyle: CSSProperties;
  onDecodeError(): void;
  onRetry(): void;
}) {
  return (
    <div className="reader-page" style={style} data-reader-page={index + 1}>
      {entry?.state === "ready" ? (
        <img
          src={entry.image.dataUrl}
          alt={`第 ${index + 1} 页`}
          draggable={false}
          onError={onDecodeError}
        />
      ) : entry?.state === "error" ? (
        <div className="reader-page-status" role="status" style={statusStyle}>
          <p>
            第 {index + 1} 页 · {readerErrorMessage(entry.error)}
          </p>
          <button onClick={onRetry}>重试此页</button>
        </div>
      ) : (
        <span className="reader-page-status" style={statusStyle}>
          正在读取第 {index + 1} 页…
        </span>
      )}
    </div>
  );
}
