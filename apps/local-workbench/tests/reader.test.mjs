import test from "node:test";
import assert from "node:assert/strict";
import { ReaderPageCache } from "../src/reader/cache.ts";
import {
  ReaderProgressWriter,
  clampPosition,
  pageAtOffset,
  pageLayout,
  pageSegment,
  maxReaderPageHeight,
  readerWindow,
  visiblePages,
} from "../src/reader/model.ts";
import {
  parseReaderBook,
  parseReaderChapter,
  parseReaderImage,
  readerErrorMessage,
} from "../src/reader/runtime.ts";

const image = (pageIndex, overrides = {}) => ({
  readerId: "reader",
  chapterId: "chapter",
  pageIndex,
  dataUrl: "data:image/png;base64,AAAA",
  width: 10,
  height: 15,
  ...overrides,
});
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

test("large chapters render a bounded viewport window, preserving page-relative anchors as image dimensions resolve", () => {
  assert.deepEqual(readerWindow(0, 10000), [0, 1, 2]);
  assert.deepEqual(readerWindow(9999, 10000), [9999, 9998, 9997]);
  assert.deepEqual(readerWindow(20, 10000), [20, 21, 19, 22, 18]);
  const estimated = pageLayout(10000, 800, new Map());
  const pageIndex = 4321,
    offset = 0.4;
  const before =
    estimated.tops[pageIndex] + estimated.heights[pageIndex] * offset;
  assert.equal(pageAtOffset(estimated, before), pageIndex);
  assert.ok(visiblePages(estimated, before, 100000).length <= 12);
  const measured = pageLayout(
    10000,
    800,
    new Map([
      [4320, 2],
      [4321, 1],
    ]),
  );
  const after = measured.tops[pageIndex] + measured.heights[pageIndex] * offset;
  assert.notEqual(after, before);
  assert.equal(pageAtOffset(measured, after), pageIndex);
  assert.equal(
    (after - measured.tops[pageIndex]) / measured.heights[pageIndex],
    offset,
  );
  assert.deepEqual(
    clampPosition({ chapterId: "c", pageIndex: 90000, offset: NaN }, 10),
    { chapterId: "c", pageIndex: 9, offset: 0 },
  );
});

test("long chapter bands preserve logical anchors within bounded browser coordinates, including extreme aspect ratios", () => {
  const layout = pageLayout(50000, 1280, new Map([[100, 20000]]));
  assert.ok(layout.total > 16_777_216);
  assert.equal(layout.heights[100], maxReaderPageHeight);
  for (const index of [0, 100, 5000, 9999, 25000, 49999]) {
    const band = pageSegment(layout, index);
    assert.ok(band.total <= 1_000_000);
    assert.ok(band.first <= index && band.last >= index);
    const logical = layout.tops[index] + layout.heights[index] * 0.35;
    const physical = logical - band.start;
    assert.ok(physical >= 0 && physical < band.total);
    assert.equal(pageAtOffset(layout, band.start + physical), index);
    const shifted = pageSegment(layout, Math.min(49999, index + 1));
    const rebasedPhysical = logical - shifted.start;
    assert.equal(shifted.start + rebasedPhysical, logical);
  }
});

test("page cache prioritizes current pages, caps concurrent reads and discards stale chapter work on disposal", async () => {
  const held = new Map(),
    called = [];
  let changes = 0;
  const cache = new ReaderPageCache(
    (index) => {
      called.push(index);
      const request = deferred();
      held.set(index, request);
      return request.promise;
    },
    () => changes++,
  );
  cache.request(readerWindow(50, 10000));
  assert.deepEqual(called, [50, 51]);
  cache.request(readerWindow(400, 10000));
  held.get(50).resolve(image(50));
  await tick();
  assert.deepEqual(called, [50, 51, 400]);
  assert.equal(cache.entries.has(50), false);
  held.get(51).resolve(image(51));
  await tick();
  assert.deepEqual(called, [50, 51, 400, 401]);
  held.get(400).resolve(image(400));
  await tick();
  assert.equal(cache.entries.get(400).state, "ready");
  assert.ok(cache.entries.size <= 5);
  const beforeDispose = changes;
  cache.dispose();
  held.get(401).resolve(image(401));
  held.get(399).resolve(image(399));
  await tick();
  assert.equal(cache.entries.size, 0);
  assert.equal(changes, beforeDispose);
  assert.equal(called.includes(402), false);
});

test("memory pressure keeps the current page and does not create an endless preload retry loop", async () => {
  const called = [];
  const oneImage = image(0);
  const bytes = oneImage.dataUrl.length * 2 + 10 * 15 * 4;
  const cache = new ReaderPageCache(
    async (index) => {
      called.push(index);
      return image(index);
    },
    () => {},
    bytes + 5,
  );
  cache.request([2, 3, 1, 4, 0]);
  await tick();
  await tick();
  assert.equal(cache.entries.get(2).state, "ready");
  assert.ok(cache.bytes <= bytes + 5);
  const loaded = called.length;
  cache.request([2, 3, 1, 4, 0]);
  await tick();
  assert.equal(called.length, loaded);
  cache.request([3, 4, 2]);
  await tick();
  await tick();
  assert.equal(cache.entries.get(3).state, "ready");
  assert.ok(cache.bytes <= bytes + 5);
  cache.dispose();
});

test("failed and mismatched pages require explicit retry and cannot replace a valid page", async () => {
  let fail = true,
    requests = 0;
  const cache = new ReaderPageCache(
    async (index) => {
      requests++;
      if (fail) return image(index + 1);
      return image(index);
    },
    () => {},
  );
  cache.request([5]);
  await tick();
  assert.equal(cache.entries.get(5).state, "error");
  cache.request([5]);
  await tick();
  assert.equal(requests, 1);
  fail = false;
  cache.retry(5);
  await tick();
  assert.equal(requests, 2);
  assert.equal(cache.entries.get(5).image.pageIndex, 5);
  cache.dispose();
});

test("position writes serialize and coalesce, and a failed write retains the newest position for exit retry", async () => {
  const first = deferred(),
    saved = [];
  const position = (index) => ({
    chapterId: "chapter",
    pageIndex: index,
    offset: 0.25,
  });
  const writer = new ReaderProgressWriter(async (value) => {
    saved.push(value);
    if (saved.length === 1) await first.promise;
  });
  writer.set(position(1));
  const running = writer.flush();
  writer.set(position(2));
  writer.set(position(3));
  first.resolve();
  await running;
  assert.deepEqual(saved, [position(1), position(3)]);
  let fail = true;
  const retry = new ReaderProgressWriter(async (value) => {
    if (fail) throw new Error("save unavailable");
    saved.push(value);
  });
  retry.set(position(4));
  await assert.rejects(retry.flush());
  retry.set(position(5));
  fail = false;
  await retry.flush();
  assert.deepEqual(saved.at(-1), position(5));
  const failedFlight = deferred();
  const closing = new ReaderProgressWriter(async (value) => {
    if (value.pageIndex === 6) await failedFlight.promise;
    else saved.push(value);
  });
  closing.set(position(6));
  const earlier = closing.flush();
  const rejected = assert.rejects(earlier);
  closing.set(position(7));
  const exitFlush = closing.flush();
  failedFlight.reject(new Error("temporary failure"));
  await rejected;
  await exitFlush;
  assert.deepEqual(saved.at(-1), position(7));
});

test("native reader parsing rejects crossed identities and active media while preserving unknown chapter lengths", () => {
  const book = {
    readerId: "reader",
    title: "Synthetic book",
    origin: "library",
    sourceRef: null,
    chapters: [{ id: "c", title: "Chapter", pageCount: null }],
    position: null,
  };
  assert.deepEqual(parseReaderBook(book), book);
  assert.throws(() =>
    parseReaderBook({
      ...book,
      chapters: [book.chapters[0], book.chapters[0]],
    }),
  );
  assert.throws(() =>
    parseReaderChapter(
      { readerId: "other", chapterId: "c", pageCount: 1 },
      "reader",
      "c",
    ),
  );
  assert.deepEqual(
    parseReaderImage(image(0), "reader", "chapter", 0),
    image(0),
  );
  assert.throws(() =>
    parseReaderImage(
      image(0, { chapterId: "different" }),
      "reader",
      "chapter",
      0,
    ),
  );
  assert.throws(() =>
    parseReaderImage(
      image(0, { dataUrl: "data:image/svg+xml;base64,AAAA" }),
      "reader",
      "chapter",
      0,
    ),
  );
  assert.throws(() =>
    parseReaderImage(
      image(0, { width: 100000, height: 100000 }),
      "reader",
      "chapter",
      0,
    ),
  );
  assert.match(
    readerErrorMessage({ code: "SOURCE_SESSION_EXPIRED" }),
    /账号需要重新连接/,
  );
  assert.match(
    readerErrorMessage(new Error("SOURCE_SESSION_EXPIRED")),
    /账号需要重新连接/,
  );
  assert.equal(
    readerErrorMessage(new Error("C:\\Private\\book.zip unavailable")).includes(
      "Private",
    ),
    false,
  );
});
