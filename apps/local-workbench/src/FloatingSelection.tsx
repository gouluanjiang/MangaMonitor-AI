import type { ReactNode } from "react";
import "./browse-controls.css";

export function FloatingSelection({
  active,
  selectedCount,
  onEnter,
  onCancel,
  onDownload,
  busy = false,
  disabled = false,
  children,
}: {
  active: boolean;
  selectedCount: number;
  onEnter(): void;
  onCancel(): void;
  onDownload(): void;
  busy?: boolean;
  disabled?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="floating-selection-space" data-active={active}>
      {active ? (
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
          <button type="button" className="button secondary" onClick={onCancel}>
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
      )}
    </div>
  );
}
