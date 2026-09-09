export function gridWindow(
  count: number,
  columns: number,
  rowHeight: number,
  scroll: number,
  viewport: number,
  overscan = 2,
) {
  const rows = Math.ceil(count / columns);
  const first = Math.min(
    Math.max(0, rows - 1),
    Math.max(0, Math.floor(scroll / rowHeight) - overscan),
  );
  const last = Math.min(
    rows,
    Math.ceil((scroll + viewport) / rowHeight) + overscan,
  );
  return {
    first,
    last: rows ? Math.max(first + 1, last) : 0,
    height: rows * rowHeight,
  };
}
