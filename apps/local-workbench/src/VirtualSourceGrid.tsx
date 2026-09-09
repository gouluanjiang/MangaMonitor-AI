import {
  forwardRef,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { ForwardedRef, ReactNode, Ref } from "react";
import { gridWindow } from "./source-grid-layout.ts";
export interface GridAnchor {
  key: string;
  offset: number;
}
export interface SourceGridHandle {
  capture(preferred?: string): GridAnchor | null;
  restore(anchor: GridAnchor | null): void;
}
interface Props<T> {
  items: T[];
  density: 5 | 7 | 9;
  itemKey(item: T): string;
  renderItem(item: T): ReactNode;
  testId?: string;
}
function Grid<T>(
  { items, density, itemKey, renderItem, testId = "source-grid" }: Props<T>,
  forwarded: ForwardedRef<SourceGridHandle>,
) {
  const element = useRef<HTMLDivElement>(null);
  const firstRow = useRef<HTMLDivElement>(null);
  const metrics = useRef({
    columns: density as number,
    rowHeight: 330,
    offset: 0,
    scroll: 0,
    viewport: 800,
    gap: 26,
  });
  const [layout, setLayout] = useState(metrics.current);
  const current = useRef({ items, itemKey });
  current.current = { items, itemKey };
  const pending = useRef<GridAnchor | null>(null);
  const restoreFrame = useRef(0);
  const measure = useRef<() => void>(() => {});
  function capture(preferred?: string): GridAnchor | null {
    const main = element.current?.closest("main");
    if (!main || !element.current || !items.length) return null;
    const value = metrics.current;
    const relative = main.scrollTop - value.offset;
    const preferredIndex = preferred
      ? items.findIndex((item) => itemKey(item) === preferred)
      : -1;
    const row =
      preferredIndex >= 0
        ? Math.floor(preferredIndex / value.columns)
        : Math.max(0, Math.floor(Math.max(0, relative) / value.rowHeight));
    const index = Math.min(items.length - 1, row * value.columns);
    return {
      key:
        preferredIndex >= 0
          ? itemKey(items[preferredIndex])
          : itemKey(items[index]),
      offset: value.offset + row * value.rowHeight - main.scrollTop,
    };
  }
  function restore(anchor: GridAnchor | null) {
    pending.current = anchor;
    cancelAnimationFrame(restoreFrame.current);
    let attempts = 0;
    const settle = () => {
      if (!pending.current) return;
      measure.current();
      applyAnchor(pending.current);
      if (++attempts < 4) restoreFrame.current = requestAnimationFrame(settle);
      else pending.current = null;
    };
    applyAnchor(anchor);
    restoreFrame.current = requestAnimationFrame(settle);
  }
  function cancelRestore() {
    pending.current = null;
    cancelAnimationFrame(restoreFrame.current);
  }
  function applyAnchor(anchor: GridAnchor | null) {
    const main = element.current?.closest("main");
    if (!main || !anchor) return;
    const index = current.current.items.findIndex(
      (item) => current.current.itemKey(item) === anchor.key,
    );
    if (index < 0) return;
    const value = metrics.current;
    main.scrollTop =
      value.offset +
      Math.floor(index / value.columns) * value.rowHeight -
      anchor.offset;
    measure.current();
  }
  useImperativeHandle(forwarded, () => ({ capture, restore }), [
    items,
    density,
  ]);
  useLayoutEffect(() => {
    const root = element.current;
    const main = root?.closest("main");
    if (!root || !main) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const css = getComputedStyle(root);
      const columns = Math.max(
        1,
        css.gridTemplateColumns.split(" ").filter(Boolean).length,
      );
      const gap = parseFloat(css.rowGap) || 26;
      const columnGap = parseFloat(css.columnGap) || 16;
      const width = (root.clientWidth - columnGap * (columns - 1)) / columns;
      const measured = firstRow.current?.getBoundingClientRect().height;
      const rowHeight =
        columns === metrics.current.columns && measured && measured > 50
          ? measured + gap
          : (width * 7) / 5 + 90 + gap;
      const offset =
        main.scrollTop +
        root.getBoundingClientRect().top -
        main.getBoundingClientRect().top;
      const next = {
        columns,
        gap,
        rowHeight,
        offset,
        scroll: main.scrollTop,
        viewport: main.clientHeight,
      };
      metrics.current = next;
      setLayout((old) =>
        Object.keys(next).some(
          (key) =>
            Math.abs(
              old[key as keyof typeof old] - next[key as keyof typeof next],
            ) > 0.5,
        )
          ? next
          : old,
      );
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    const cancelForScrollKey = (event: KeyboardEvent) => {
      if (
        [
          "ArrowUp",
          "ArrowDown",
          "PageUp",
          "PageDown",
          "Home",
          "End",
          " ",
        ].includes(event.key)
      )
        cancelRestore();
    };
    measure.current = update;
    const observer = new ResizeObserver(schedule);
    observer.observe(root);
    observer.observe(main);
    main.addEventListener("scroll", schedule, { passive: true });
    // New user input takes precedence over the remaining layout-settling frames.
    main.addEventListener("wheel", cancelRestore, {
      passive: true,
      capture: true,
    });
    main.addEventListener("touchstart", cancelRestore, {
      passive: true,
      capture: true,
    });
    main.addEventListener("pointerdown", cancelRestore, {
      passive: true,
      capture: true,
    });
    main.addEventListener("keydown", cancelForScrollKey, true);
    update();
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(restoreFrame.current);
      observer.disconnect();
      main.removeEventListener("scroll", schedule);
      main.removeEventListener("wheel", cancelRestore, true);
      main.removeEventListener("touchstart", cancelRestore, true);
      main.removeEventListener("pointerdown", cancelRestore, true);
      main.removeEventListener("keydown", cancelForScrollKey, true);
    };
  }, [density]);
  useLayoutEffect(() => {
    measure.current();
    if (pending.current) applyAnchor(pending.current);
  }, [items, density, layout.rowHeight]);
  useLayoutEffect(() => {
    if (firstRow.current) {
      const observer = new ResizeObserver(() => measure.current());
      observer.observe(firstRow.current);
      return () => observer.disconnect();
    }
  }, [layout.columns, items.length]);
  const range = gridWindow(
    items.length,
    layout.columns,
    layout.rowHeight,
    layout.scroll - layout.offset,
    layout.viewport,
  );
  const rows = [];
  for (let row = range.first; row < range.last; row++) rows.push(row);
  return (
    <div
      ref={element}
      className="source-grid source-virtual-grid"
      data-testid={testId}
      data-density={density}
      data-total-items={items.length}
      style={{ height: range.height, position: "relative" }}
    >
      {rows.map((row, index) => (
        <div
          key={row}
          ref={index === 0 ? firstRow : undefined}
          className="source-virtual-row"
          style={{
            position: "absolute",
            top: row * layout.rowHeight,
            left: 0,
            right: 0,
            display: "grid",
            gridTemplateColumns:
              "repeat(" + layout.columns + ", minmax(0,1fr))",
            columnGap: "inherit",
          }}
        >
          {items
            .slice(row * layout.columns, (row + 1) * layout.columns)
            .map((item) => (
              <div key={itemKey(item)} data-virtual-key={itemKey(item)}>
                {renderItem(item)}
              </div>
            ))}
        </div>
      ))}
    </div>
  );
}
export const VirtualSourceGrid = forwardRef(Grid) as <T>(
  props: Props<T> & { ref?: Ref<SourceGridHandle> },
) => ReactNode;
