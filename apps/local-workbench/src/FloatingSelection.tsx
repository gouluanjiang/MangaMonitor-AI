import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./browse-controls.css";

export function FloatingSelection({
  active,
  visible = true,
  selectedCount,
  onEnter,
  onCancel,
  onDownload,
  busy = false,
  disabled = false,
  children,
}: {
  active: boolean;
  visible?: boolean;
  selectedCount: number;
  onEnter(): void;
  onCancel(): void;
  onDownload(): void;
  busy?: boolean;
  disabled?: boolean;
  children?: ReactNode;
}) {
  const mount = useRef<HTMLSpanElement>(null);
  const [dock, setDock] = useState<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    if (!visible) return;
    const main = mount.current?.closest("main");
    const shell = main?.parentElement;
    if (!shell) return;
    // Outside the scrolling canvas: the flex layout reserves the actual height,
    // including wrapped controls, instead of covering a row of manga cards.
    const host = document.createElement("div");
    host.className = "floating-selection-host";
    host.dataset.testid = "browse-selection-dock";
    shell.insertBefore(host, shell.querySelector(":scope > .statusbar"));
    setDock(host);
    return () => host.remove();
  }, [visible]);
  return (
    <>
      <span ref={mount} hidden />
      {visible &&
        dock &&
        createPortal(
          active ? (
            <div
              className="floating-selection-bar"
              role="toolbar"
              aria-label="批量下载操作"
            >
              <strong aria-live="polite">已选 {selectedCount} 本</strong>
              {children && (
                <div className="floating-selection-options">{children}</div>
              )}
              <button
                type="button"
                className="button primary"
                disabled={disabled || busy || selectedCount === 0}
                onClick={onDownload}
              >
                {busy ? "正在加入…" : "下载"}
              </button>
              <button
                type="button"
                className="button secondary"
                onClick={onCancel}
              >
                取消
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="button secondary floating-selection-entry"
              onClick={onEnter}
              disabled={disabled}
            >
              多选
            </button>
          ),
          dock,
        )}
    </>
  );
}
