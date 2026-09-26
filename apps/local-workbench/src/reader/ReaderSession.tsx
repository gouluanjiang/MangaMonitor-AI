import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import type { ReaderAdapter, ReaderPosition } from "./types.ts";
import type { ComicReaderProps, ReaderSessionState } from "./ComicReader.tsx";
import {
  ReaderDownloadError,
  ReaderError,
  readerErrorMessage,
} from "./runtime.ts";
import { ReaderPageCache } from "./cache.ts";
import {
  clampPosition,
  pageAtOffset,
  pageLayout,
  pageSegment,
  readerWindow,
  visiblePages,
} from "./model.ts";
import { ReaderPage } from "./ReaderPage.tsx";
import { ReaderToolbar } from "./ReaderToolbar.tsx";
export function ReaderSession({
  session,
  adapter,
  onClose,
  onDownload,
  closing,
}: {
  session: ReaderSessionState;
  adapter: ReaderAdapter;
  onClose: () => void;
  onDownload?: ComicReaderProps["onDownload"];
  closing: boolean;
}) {
  const { book, writer } = session;
  const initialChapter =
    book.chapters.find((chapter) => chapter.id === book.position?.chapterId) ??
    book.chapters[0];
  const [chapterId, setChapterId] = useState(initialChapter.id);
  const [chapterCount, setChapterCount] = useState<{
    id: string;
    count: number;
  } | null>(null);
  const [chapterError, setChapterError] = useState<unknown>(null);
  const [chapterAttempt, setChapterAttempt] = useState(0);
  const [mode, setMode] = useState<"vertical" | "single">("vertical");
  const [zoom, setZoom] = useState(1);
  const [revision, setRevision] = useState(0);
  const [position, setPosition] = useState<ReaderPosition>(
    book.position?.chapterId === initialChapter.id
      ? book.position
      : { chapterId: initialChapter.id, pageIndex: 0, offset: 0 },
  );
  const [scrollTop, setScrollTop] = useState(0);
  const [segmentPage, setSegmentPage] = useState(position.pageIndex);
  const [viewport, setViewport] = useState({ width: 1000, height: 700 });
  const [toolbarHover, setToolbarHover] = useState(false);
  const [toolbarFocus, setToolbarFocus] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [notice, setNotice] = useState("");
  const [downloading, setDownloading] = useState(false);
  const viewportRef = useRef<HTMLDivElement>(null);
  const cacheRef = useRef<ReaderPageCache | null>(null);
  const ratios = useRef(new Map<number, number>());
  const currentPosition = useRef(position);
  const pendingAnchor = useRef<ReaderPosition | null>(position);
  const pendingChapterPosition = useRef<ReaderPosition>(position);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const gesture = useRef<{
    x: number;
    y: number;
    left: number;
    top: number;
    moved: boolean;
    drag: boolean;
    id: number;
    lastX: number;
    lastY: number;
  } | null>(null);
  const fullscreenBusy = useRef(false);
  const fullscreenRef = useRef(false);
  const count = chapterCount?.id === chapterId ? chapterCount.count : 0;
  const chapterIndex = book.chapters.findIndex(
    (chapter) => chapter.id === chapterId,
  );
  const layout = useMemo(
    () => pageLayout(count, viewport.width * zoom, ratios.current),
    [count, viewport.width, zoom, revision],
  );
  const segment = useMemo(
    () => pageSegment(layout, segmentPage),
    [layout, segmentPage],
  );
  const latest = useRef({
    layout,
    mode,
    position,
    count,
    zoom,
    chapterId,
    viewport,
    segment,
  });
  latest.current = {
    layout,
    mode,
    position,
    count,
    zoom,
    chapterId,
    viewport,
    segment,
  };
  const capturePosition = (): ReaderPosition => {
    const element = viewportRef.current;
    const state = latest.current;
    if (!element || !state.count) return currentPosition.current;
    if (state.mode === "single") {
      const ratio =
        ratios.current.get(currentPosition.current.pageIndex) ?? 1.45;
      const height =
        Math.min(state.viewport.width, state.viewport.height / ratio) *
        state.zoom *
        ratio;
      return {
        chapterId: state.chapterId,
        pageIndex: currentPosition.current.pageIndex,
        offset: Math.min(1, element.scrollTop / height),
      };
    }
    const logicalTop = state.segment.start + element.scrollTop;
    const pageIndex = pageAtOffset(state.layout, logicalTop);
    return {
      chapterId: state.chapterId,
      pageIndex,
      offset: Math.max(
        0,
        Math.min(
          1,
          (logicalTop - state.layout.tops[pageIndex]) /
            state.layout.heights[pageIndex],
        ),
      ),
    };
  };
  const remember = (value: ReaderPosition) => {
    currentPosition.current = value;
    setPosition(value);
    writer.set(value);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      void writer
        .flush()
        .catch(() => setNotice("阅读位置暂未保存，退出时会再次尝试。"));
    }, 700);
  };
  useEffect(
    () => () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    },
    [],
  );
  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    element.focus({ preventScroll: true });
    const observer = new ResizeObserver(() => {
      pendingAnchor.current = capturePosition();
      setSegmentPage(pendingAnchor.current.pageIndex);
      setViewport({
        width: Math.max(1, element.clientWidth),
        height: Math.max(1, element.clientHeight),
      });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    let cancelled = false;
    setChapterCount(null);
    setChapterError(null);
    ratios.current = new Map();
    const cache = new ReaderPageCache(
      async (index) => {
        const image = await adapter.page(book.readerId, chapterId, index);
        if (image.readerId !== book.readerId || image.chapterId !== chapterId)
          throw new ReaderError("READER_STALE_RESPONSE");
        return image;
      },
      () => {
        if (cancelled) return;
        const anchor = pendingAnchor.current ?? capturePosition();
        let changed = false;
        for (const [index, entry] of cache.entries)
          if (entry.state === "ready") {
            const ratio = entry.image.height / entry.image.width;
            if (ratios.current.get(index) !== ratio) {
              if ((ratios.current.get(index) ?? 1.45) !== ratio) changed = true;
              ratios.current.set(index, ratio);
            }
          }
        if (changed) {
          pendingAnchor.current = anchor;
          setSegmentPage(anchor.pageIndex);
        }
        setRevision((n) => n + 1);
      },
    );
    cacheRef.current = cache;
    void adapter
      .chapter(book.readerId, chapterId)
      .then((info) => {
        if (cancelled) return;
        if (
          info.readerId !== book.readerId ||
          info.chapterId !== chapterId ||
          !Number.isSafeInteger(info.pageCount) ||
          info.pageCount <= 0 ||
          info.pageCount > 50000
        )
          throw new ReaderError("READER_STALE_RESPONSE");
        const target = clampPosition(
          pendingChapterPosition.current,
          info.pageCount,
        );
        pendingAnchor.current = target;
        currentPosition.current = target;
        setPosition(target);
        setSegmentPage(target.pageIndex);
        setChapterCount({ id: chapterId, count: info.pageCount });
      })
      .catch((failure: unknown) => {
        if (!cancelled) setChapterError(failure);
      });
    return () => {
      cancelled = true;
      cache.dispose();
      if (cacheRef.current === cache) cacheRef.current = null;
    };
  }, [adapter, book.readerId, chapterId, chapterAttempt]);
  useLayoutEffect(() => {
    const element = viewportRef.current,
      anchor = pendingAnchor.current;
    if (!element || !anchor || !count) return;
    pendingAnchor.current = null;
    const target = clampPosition(anchor, count);
    const ratio = ratios.current.get(target.pageIndex) ?? 1.45;
    const singleHeight =
      Math.min(viewport.width, viewport.height / ratio) * zoom * ratio;
    element.scrollTop =
      mode === "vertical"
        ? layout.tops[target.pageIndex] -
          segment.start +
          layout.heights[target.pageIndex] * target.offset
        : singleHeight * target.offset;
    setScrollTop(element.scrollTop);
    const active = gesture.current;
    if (active?.drag) {
      active.x = active.lastX;
      active.y = active.lastY;
      active.top = element.scrollTop;
      active.left = element.scrollLeft;
    }
    currentPosition.current = target;
    setPosition(target);
  }, [layout, segment, count, mode, zoom, viewport.height]);
  const logicalTop =
    pendingAnchor.current && count && mode === "vertical"
      ? layout.tops[Math.min(count - 1, pendingAnchor.current.pageIndex)] +
        layout.heights[Math.min(count - 1, pendingAnchor.current.pageIndex)] *
          pendingAnchor.current.offset
      : segment.start + scrollTop;
  const visible = count
    ? mode === "vertical"
      ? visiblePages(layout, logicalTop, viewport.height).filter(
          (index) => index >= segment.first && index <= segment.last,
        )
      : [position.pageIndex]
    : [];
  const visibleKey = visible.join(",");
  useEffect(() => {
    if (!count) return;
    cacheRef.current?.request([
      position.pageIndex,
      ...visible,
      ...readerWindow(position.pageIndex, count),
    ]);
  }, [chapterId, count, position.pageIndex, visibleKey, mode]);

  const changeChapter = (id: string) => {
    if (id === chapterId) return;
    writer.set(capturePosition());
    void writer.flush().catch(() => setNotice("阅读位置暂未保存。"));
    pendingChapterPosition.current = { chapterId: id, pageIndex: 0, offset: 0 };
    pendingAnchor.current = pendingChapterPosition.current;
    currentPosition.current = pendingChapterPosition.current;
    setChapterId(id);
    setSegmentPage(0);
    setPosition(pendingChapterPosition.current);
    setScrollTop(0);
  };
  const jump = (pageIndex: number) => {
    if (!count) return;
    const value = {
      chapterId,
      pageIndex: Math.max(0, Math.min(count - 1, pageIndex)),
      offset: 0,
    };
    pendingAnchor.current = value;
    remember(value);
    if (
      mode === "vertical" &&
      (value.pageIndex < segment.first || value.pageIndex > segment.last)
    ) {
      setSegmentPage(value.pageIndex);
    } else if (mode === "vertical" && viewportRef.current) {
      viewportRef.current.scrollTop =
        layout.tops[value.pageIndex] - segment.start;
      setScrollTop(viewportRef.current.scrollTop);
      pendingAnchor.current = null;
    } else if (viewportRef.current) {
      viewportRef.current.scrollTop = 0;
      viewportRef.current.scrollLeft = 0;
      pendingAnchor.current = null;
    }
  };
  const changeMode = (next: "vertical" | "single") => {
    if (next === mode) return;
    const value = { ...capturePosition(), offset: 0 };
    pendingAnchor.current = value;
    setSegmentPage(value.pageIndex);
    setMode(next);
    setZoom(1);
    remember(value);
  };
  const zoomBy = (direction: number) => {
    const next = Math.max(
      0.25,
      Math.min(
        4,
        Math.round(latest.current.zoom * Math.exp(direction * 0.12) * 100) /
          100,
      ),
    );
    if (next === latest.current.zoom) return;
    pendingAnchor.current = capturePosition();
    setSegmentPage(pendingAnchor.current.pageIndex);
    setZoom(next);
  };
  const toggleFullscreen = async () => {
    if (fullscreenBusy.current) return;
    fullscreenBusy.current = true;
    try {
      const next = !fullscreenRef.current;
      await adapter.fullscreen(next);
      fullscreenRef.current = next;
      setFullscreen(next);
    } catch {
      setNotice("暂时无法切换全屏，请重试。");
    } finally {
      fullscreenBusy.current = false;
    }
  };
  const hasOtherDialog = () =>
    Array.from(
      document.querySelectorAll("dialog[open], [aria-modal='true']"),
    ).some((node) => !node.classList.contains("comic-reader"));
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (hasOtherDialog()) return;
      if (event.key === "F11") {
        event.preventDefault();
        void toggleFullscreen();
        return;
      }
      const target = event.target;
      const control =
        target instanceof HTMLElement &&
        target.closest(
          "button,input,select,textarea,a,[contenteditable='true']",
        );
      if (event.key === "Escape") {
        if (target instanceof HTMLSelectElement) return;
        event.preventDefault();
        if (fullscreenRef.current) void toggleFullscreen();
        else onClose();
        return;
      }
      if (event.key === "Tab") {
        const elements = Array.from(
          document.querySelectorAll<HTMLElement>(
            ".comic-reader button:not(:disabled),.comic-reader input,.comic-reader select,.reader-viewport",
          ),
        );
        const first = elements[0],
          last = elements[elements.length - 1];
        if (event.shiftKey && target === first) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && target === last) {
          event.preventDefault();
          first?.focus();
        }
        return;
      }
      if (
        control ||
        event.ctrlKey ||
        event.altKey ||
        event.metaKey ||
        mode !== "single"
      )
        return;
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault();
        jump(position.pageIndex + (event.key === "ArrowRight" ? 1 : -1));
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [mode, position.pageIndex, count, fullscreen, onClose]);
  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey || event.deltaY === 0 || hasOtherDialog()) return;
      event.preventDefault();
      zoomBy(event.deltaY < 0 ? 1 : -1);
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, []);
  const pointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (
      event.pointerType !== "mouse" ||
      event.button !== 0 ||
      (event.target instanceof Element &&
        event.target.closest("button,input,select"))
    )
      return;
    const element = viewportRef.current;
    if (!element) return;
    const box = element.getBoundingClientRect();
    if (event.clientX >= box.left + element.clientWidth) return;
    gesture.current = {
      x: event.clientX,
      y: event.clientY,
      left: element.scrollLeft,
      top: element.scrollTop,
      moved: false,
      drag: zoom > 1,
      id: event.pointerId,
      lastX: event.clientX,
      lastY: event.clientY,
    };
    if (zoom > 1) {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
    }
  };
  const pointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = gesture.current,
      element = viewportRef.current;
    if (!active || !element) return;
    active.lastX = event.clientX;
    active.lastY = event.clientY;
    if (Math.hypot(event.clientX - active.x, event.clientY - active.y) > 5)
      active.moved = true;
    if (active.drag && active.moved) {
      pendingAnchor.current = null;
      element.scrollLeft = active.left - (event.clientX - active.x);
      element.scrollTop = active.top - (event.clientY - active.y);
    }
  };
  const pointerUp = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = gesture.current,
      element = viewportRef.current;
    gesture.current = null;
    if (!active || !element) return;
    if (element.hasPointerCapture(event.pointerId))
      element.releasePointerCapture(event.pointerId);
    if (!active.moved && mode === "single") {
      const box = element.getBoundingClientRect();
      jump(
        position.pageIndex +
          (event.clientX - box.left < element.clientWidth / 2 ? -1 : 1),
      );
    }
  };
  const renderPage = (index: number) => {
    const entry = cacheRef.current?.entries.get(index);
    const ratio = ratios.current.get(index) ?? 1.45;
    const singleWidth =
      Math.min(viewport.width, viewport.height / ratio) * zoom;
    const style: CSSProperties =
      mode === "vertical"
        ? {
            position: "absolute",
            top: layout.tops[index] - segment.start,
            width: Math.min(
              viewport.width * zoom,
              layout.heights[index] / ratio,
            ),
            height: layout.heights[index],
          }
        : {
            width: singleWidth,
            minHeight: Math.min(viewport.height, singleWidth * ratio),
            height: singleWidth * ratio,
          };
    return (
      <ReaderPage
        key={index}
        index={index}
        entry={entry}
        style={style}
        statusStyle={{
          position: "absolute",
          left: 0,
          width: "100%",
          height: Math.min(viewport.height, Number(style.height)),
          top: Math.max(
            0,
            Math.min(
              Number(style.height) - viewport.height,
              mode === "vertical" ? logicalTop - layout.tops[index] : scrollTop,
            ),
          ),
        }}
        onDecodeError={() => {
          cacheRef.current?.entries.set(index, {
            state: "error",
            error: "READER_IMAGE_DECODE",
          });
          setRevision((n) => n + 1);
        }}
        onRetry={() => {
          cacheRef.current?.retry(index);
          setRevision((n) => n + 1);
        }}
      />
    );
  };
  const atEnd =
    count > 0 &&
    position.pageIndex === count - 1 &&
    (mode === "single" || logicalTop + viewport.height >= layout.total - 36);
  return (
    <>
      <div
        ref={viewportRef}
        className={`reader-viewport reader-${mode}`}
        tabIndex={0}
        aria-label="漫画页面"
        data-testid="reader-viewport"
        data-mode={mode}
        data-zoom={zoom}
        onScroll={() => {
          if (!pendingAnchor.current && count) {
            const value = capturePosition();
            remember(value);
            const element = viewportRef.current!;
            if (
              mode === "vertical" &&
              value.pageIndex !== segmentPage &&
              ((segment.first > 0 && element.scrollTop < viewport.height * 2) ||
                (segment.last < count - 1 &&
                  element.scrollTop + viewport.height * 3 > segment.total))
            ) {
              pendingAnchor.current = value;
              setSegmentPage(value.pageIndex);
            }
          }
          setScrollTop(viewportRef.current?.scrollTop ?? 0);
        }}
        onPointerDown={pointerDown}
        onPointerMove={pointerMove}
        onPointerUp={pointerUp}
        onPointerCancel={() => {
          gesture.current = null;
        }}
      >
        {!count ? (
          <div className="reader-message" role="status">
            <p>
              {chapterError
                ? readerErrorMessage(chapterError)
                : "正在读取章节…"}
            </p>
            {Boolean(chapterError) && (
              <button onClick={() => setChapterAttempt((n) => n + 1)}>
                重试章节
              </button>
            )}
          </div>
        ) : (
          <div
            className="reader-pages"
            data-first-page={segment.first + 1}
            data-last-page={segment.last + 1}
            style={
              mode === "vertical"
                ? {
                    height: segment.total,
                    width: Math.max(viewport.width, viewport.width * zoom),
                  }
                : { minHeight: viewport.height, minWidth: viewport.width }
            }
          >
            {visible.map(renderPage)}
          </div>
        )}
      </div>
      {atEnd && (
        <div className="reader-chapter-end" role="status">
          <span>
            {chapterIndex === book.chapters.length - 1
              ? "本书已到最后一页"
              : "本章已到最后一页"}
          </span>
          {chapterIndex < book.chapters.length - 1 && (
            <button
              onClick={() => changeChapter(book.chapters[chapterIndex + 1].id)}
            >
              下一章
            </button>
          )}
        </div>
      )}
      <ReaderToolbar
        title={book.title}
        chapters={book.chapters}
        chapterId={chapterId}
        mode={mode}
        count={count}
        pageIndex={position.pageIndex}
        zoom={zoom}
        notice={notice}
        visible={toolbarHover || toolbarFocus}
        closing={closing}
        downloading={downloading}
        fullscreen={fullscreen}
        onHover={setToolbarHover}
        onFocusChange={setToolbarFocus}
        onClose={onClose}
        onModeChange={changeMode}
        onChapterChange={changeChapter}
        onJump={jump}
        onResetZoom={() => {
          if (zoom !== 1) {
            pendingAnchor.current = capturePosition();
            setSegmentPage(pendingAnchor.current.pageIndex);
            setZoom(1);
          }
        }}
        onDownload={
          onDownload && book.origin !== "library" && book.sourceRef
            ? async () => {
                setDownloading(true);
                try {
                  await onDownload(book.sourceRef!);
                } catch (failure) {
                  setNotice(
                    failure instanceof ReaderDownloadError
                      ? failure.message
                      : "暂时无法准备下载，请稍后重试。",
                  );
                } finally {
                  setDownloading(false);
                }
              }
            : undefined
        }
        onFullscreen={() => void toggleFullscreen()}
      />
    </>
  );
}
