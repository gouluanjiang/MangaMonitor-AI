import type { ReaderChapter } from "./types.ts";

export type ReaderToolbarProps = {
  title: string;
  chapters: ReaderChapter[];
  chapterId: string;
  mode: "vertical" | "single";
  count: number;
  pageIndex: number;
  zoom: number;
  notice: string;
  visible: boolean;
  closing: boolean;
  downloading: boolean;
  fullscreen: boolean;
  onHover(value: boolean): void;
  onFocusChange(value: boolean): void;
  onClose(): void;
  onModeChange(mode: "vertical" | "single"): void;
  onChapterChange(chapterId: string): void;
  onJump(pageIndex: number): void;
  onResetZoom(): void;
  onDownload?: () => void;
  onFullscreen(): void;
};

/** Controls only. The reader owns chapter, position, fullscreen and persistence. */
export function ReaderToolbar(props: ReaderToolbarProps) {
  return (
    <div
      className={`reader-toolbar-zone${props.visible ? " is-visible" : ""}`}
      onPointerEnter={() => props.onHover(true)}
      onPointerLeave={() => props.onHover(false)}
    >
      <div
        className="reader-toolbar"
        role="toolbar"
        aria-label="阅读工具"
        onFocus={() => props.onFocusChange(true)}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget))
            props.onFocusChange(false);
        }}
      >
        <div className="reader-toolbar-title">
          <span title={props.title}>{props.title}</span>
          {props.notice && <small role="status">{props.notice}</small>}
        </div>
        <div className="reader-toolbar-controls">
          <button onClick={props.onClose} disabled={props.closing}>
            {props.closing ? "正在返回…" : "返回"}
          </button>
          <select
            aria-label="阅读模式"
            value={props.mode}
            onChange={(event) =>
              props.onModeChange(event.target.value as "vertical" | "single")
            }
          >
            <option value="vertical">纵向连续</option>
            <option value="single">单页</option>
          </select>
          <select
            aria-label="选择章节"
            value={props.chapterId}
            onChange={(event) => props.onChapterChange(event.target.value)}
          >
            {props.chapters.map((chapter) => (
              <option key={chapter.id} value={chapter.id}>
                {chapter.title}
              </option>
            ))}
          </select>
          <input
            aria-label="阅读进度"
            type="range"
            min={1}
            max={Math.max(1, props.count)}
            value={Math.min(props.count, props.pageIndex + 1) || 1}
            disabled={!props.count}
            onChange={(event) => props.onJump(Number(event.target.value) - 1)}
          />
          <output aria-label="当前页码">
            {props.count ? props.pageIndex + 1 : 0} / {props.count || "—"}
          </output>
          <button onClick={props.onResetZoom} title="Ctrl + 滚轮缩放">
            {Math.round(props.zoom * 100)}%
          </button>
          {props.onDownload && (
            <button disabled={props.downloading} onClick={props.onDownload}>
              下载这本
            </button>
          )}
          <button onClick={props.onFullscreen}>
            {props.fullscreen ? "退出全屏" : "全屏"}
          </button>
        </div>
      </div>
    </div>
  );
}
