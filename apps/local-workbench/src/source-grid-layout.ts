export function gridWindow(
  count: number,
  columns: number,
  rowHeight: number,
  scroll: number,
  viewport: number,
  overscan = 2,
) {
  const rows = Math.ceil(count / columns);
  const first = Math.max(0, Math.floor(scroll / rowHeight) - overscan);
  const last = Math.min(
    rows,
    Math.ceil((scroll + viewport) / rowHeight) + overscan,
  );
  return {
    first: Math.min(first, rows),
    last: Math.max(Math.min(first, rows), last),
    height: rows * rowHeight,
  };
}
