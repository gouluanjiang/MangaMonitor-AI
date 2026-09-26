import test from "node:test";
import assert from "node:assert/strict";
import { setImmediate as flush } from "node:timers/promises";
import { CoverScheduler } from "../src/cover-scheduler.ts";
import { CoverSessionCache } from "../src/source-cover-cache.ts";
import { LibraryCoverCache } from "../src/library-cover-cache.ts";
import { observeCover } from "../src/cover-visibility.ts";

const image = "data:image/jpeg;base64,/9j/2Q==";

test("visible cards overtake nearby work; only one speculative read occupies spare slots", async () => {
  const queue = new CoverScheduler(4);
  const started = [];
  const releases = new Map();
  const enqueue = (id, priority) =>
    queue.enqueue(() => {
      started.push(id);
      return new Promise((resolve) => releases.set(id, () => resolve(id)));
    }, priority);
  const jobs = [enqueue("near-1", "nearby"), enqueue("near-2", "nearby")];
  await flush();
  assert.deepEqual(started, ["near-1"]);
  jobs.push(
    enqueue("visible-1", "visible"),
    enqueue("visible-2", "visible"),
    enqueue("visible-3", "visible"),
  );
  await flush();
  assert.deepEqual(started, ["near-1", "visible-1", "visible-2", "visible-3"]);
  jobs[1].setPriority("visible");
  releases.get("visible-1")();
  await flush();
  assert.equal(
    started.at(-1),
    "near-2",
    "an approaching card is promoted without restarting it",
  );
  for (const release of releases.values()) release();
  await Promise.all(jobs.map((job) => job.promise));
});

test("queued work cancels before loading and sync/async failures free capacity", async () => {
  const queue = new CoverScheduler(1, 3);
  let release;
  const running = queue.enqueue(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  await flush();
  const cancelled = queue.enqueue(async () => {
    throw new Error("must not run");
  });
  const syncFailure = queue.enqueue(() => {
    throw new Error("synthetic sync");
  });
  const asyncFailure = queue.enqueue(async () => {
    throw new Error("synthetic async");
  });
  const failure = Promise.allSettled([
    syncFailure.promise,
    asyncFailure.promise,
  ]);
  await assert.rejects(
    queue.enqueue(async () => "overflow").promise,
    /COVER_QUEUE_FULL/,
  );
  assert.equal(cancelled.cancel(), true);
  assert.equal(cancelled.cancel(), false);
  assert.equal(await cancelled.promise, null);
  release("done");
  assert.equal(await running.promise, "done");
  assert.deepEqual(
    (await failure).map((result) => result.status),
    ["rejected", "rejected"],
  );
  assert.equal(await queue.enqueue(async () => "retry").promise, "retry");
});

test("shared source requests adopt a visible lease's priority and remain reusable after release", async () => {
  const cache = new CoverSessionCache();
  const scope = { source: "JM", sessionId: "synthetic-priority" };
  let calls = 0;
  const load = async () => {
    calls++;
    return image;
  };
  const nearby = cache.acquire(scope, "1", load, "nearby");
  const visible = cache.acquire(scope, "1", load, "visible");
  nearby.release();
  visible.setPriority("nearby");
  visible.setPriority("visible");
  const result = await visible.promise;
  assert.equal(result.status, "ready");
  visible.release();
  const again = cache.acquire(scope, "1", load);
  assert.equal((await again.promise).url, result.url);
  assert.equal(calls, 1);
  again.release();
  cache.retainScopes([]);
});

test("local queued cancellation is immediate, and a late old generation cannot affect its replacement", async () => {
  const cache = new LibraryCoverCache();
  cache.setScope("root", 1);
  const started = [];
  const releases = [];
  const load = (id) => () => {
    started.push(id);
    return new Promise((resolve) =>
      releases.push(() => resolve({ dataUrl: image })),
    );
  };
  const a = cache.acquire("root", 1, "a", load("a"));
  const b = cache.acquire("root", 1, "b", load("b"));
  const cancelled = cache.acquire("root", 1, "c", load("c"));
  await flush();
  assert.deepEqual(started, ["a", "b"]);
  cancelled.release();
  assert.equal((await cancelled.promise).status, "cancelled");
  cache.setScope("root", 2);
  const next = cache.acquire("root", 2, "a", async () => ({ dataUrl: image }));
  releases.forEach((release) => release());
  assert.equal((await a.promise).status, "cancelled");
  assert.equal((await b.promise).status, "cancelled");
  assert.equal((await next.promise).status, "ready");
  a.release();
  b.release();
  next.release();
  assert.equal(cache.peek("root", 1, "a"), undefined);
  assert.equal(cache.peek("root", 2, "a").status, "ready");
  cache.clear();
});

test("shared visibility observes each card, turns preloading with scroll direction and ignores retired callbacks", async () => {
  const observers = [];
  const oldWindow = globalThis.window;
  const oldObserver = globalThis.IntersectionObserver;
  class Target extends EventTarget {
    scrollTop = 0;
    clientHeight = 800;
  }
  class Observer {
    elements = new Set();
    constructor(callback, options) {
      this.callback = callback;
      this.options = options;
      observers.push(this);
    }
    observe(element) {
      this.elements.add(element);
    }
    unobserve(element) {
      this.elements.delete(element);
    }
    disconnect() {
      this.disconnected = true;
      this.elements.clear();
    }
    emit(element, value) {
      this.callback([{ target: element, isIntersecting: value }]);
    }
  }
  const windowTarget = new Target();
  windowTarget.IntersectionObserver = Observer;
  windowTarget.innerHeight = 800;
  globalThis.window = windowTarget;
  globalThis.IntersectionObserver = Observer;
  const stops = [];
  try {
    const root = new Target();
    const first = { closest: () => root };
    const second = { closest: () => root };
    const states = [],
      other = [];
    stops.push(observeCover(first, (value) => states.push(value)));
    stops.push(observeCover(second, (value) => other.push(value)));
    assert.equal(observers.length, 2, "all cards share one observer pair");
    assert.equal(observers[1].options.rootMargin, "200px 0px 800px 0px");
    observers[1].emit(first, true);
    await flush();
    observers[0].emit(first, true);
    await flush();
    observers[0].emit(first, false);
    await flush();
    observers[1].emit(first, false);
    await flush();
    assert.deepEqual(states, ["nearby", "visible", "nearby", null]);
    root.scrollTop = 500;
    root.dispatchEvent(new Event("scroll"));
    root.scrollTop = 480;
    root.dispatchEvent(new Event("scroll"));
    assert.equal(
      observers.length,
      2,
      "small scroll jitter does not recreate observers",
    );
    root.scrollTop = 400;
    root.dispatchEvent(new Event("scroll"));
    await flush();
    assert.equal(observers.length, 3);
    assert.equal(observers[2].options.rootMargin, "800px 0px 200px 0px");
    observers[1].emit(second, true);
    await flush();
    assert.equal(
      other.at(-1),
      null,
      "retired preload results cannot revive old requests",
    );
    observers[2].emit(second, true);
    await flush();
    assert.equal(other.at(-1), "nearby");
    stops[0]();
    observers[0].emit(first, true);
    await flush();
    assert.equal(states.length, 4);
    assert.equal(observers[0].elements.size, 1);
    root.clientHeight = 1800;
    windowTarget.dispatchEvent(new Event("resize"));
    assert.equal(observers.at(-1).options.rootMargin, "1200px 0px 200px 0px");
    stops[1]();
    assert.ok(observers.every((observer) => observer.disconnected));
    root.scrollTop = 900;
    root.dispatchEvent(new Event("scroll"));
    assert.equal(
      observers.length,
      4,
      "last unmount removes the shared listeners",
    );
  } finally {
    stops.forEach((stop) => stop());
    if (oldWindow === undefined) delete globalThis.window;
    else globalThis.window = oldWindow;
    if (oldObserver === undefined) delete globalThis.IntersectionObserver;
    else globalThis.IntersectionObserver = oldObserver;
  }
});
test("two idle network slots prepare the next screen while visible arrivals keep capacity", async (t) => {
  const queue = new CoverScheduler(4, 64, 2);
  const started = [];
  const releases = new Map();
  const load = (id, priority) =>
    queue.enqueue(() => {
      started.push(id);
      return new Promise((resolve) => releases.set(id, () => resolve(id)));
    }, priority);
  const near = Array.from({ length: 12 }, (_, i) =>
    load("next-" + i, "nearby"),
  );
  await flush();
  assert.deepEqual(started, ["next-0", "next-1"]);
  const visible = [load("visible-1", "visible"), load("visible-2", "visible")];
  await flush();
  assert.deepEqual(started.slice(-2), ["visible-1", "visible-2"]);
  releases.get("visible-1")();
  releases.get("visible-2")();
  await Promise.all(visible.map((job) => job.promise));
  await flush();
  assert.equal(
    started.length,
    4,
    "prefetch remains capped even with two more free slots",
  );
  let waves = 0;
  while (near.some((_, i) => releases.has("next-" + i))) {
    const batch = [...releases].filter(([id]) => id.startsWith("next-"));
    batch.forEach(([id, resolve]) => {
      releases.delete(id);
      resolve();
    });
    waves++;
    await flush();
  }
  await Promise.all(near.map((job) => job.promise));
  assert.equal(waves, 6);
  t.diagnostic(
    JSON.stringify({
      prefetchCovers: 12,
      previousWaves: 12,
      currentWaves: waves,
      networkMaximum: 4,
      prefetchMaximum: 2,
      liveSpeedupMeasured: false,
    }),
  );
});

test("late local image errors do not discard a replacement thumbnail", async () => {
  const cache = new LibraryCoverCache();
  cache.setScope("root", 1);
  const first = cache.acquire("root", 1, "a", async () => ({ dataUrl: image }));
  const old = await first.promise;
  assert.equal(cache.invalidate("root", 1, "a", old.url), true);
  cache.retry("root", 1, "a");
  const second = cache.acquire("root", 1, "a", async () => ({
    dataUrl: image,
  }));
  const current = await second.promise;
  first.release();
  assert.notEqual(current.url, old.url);
  assert.equal(cache.invalidate("root", 1, "a", old.url), false);
  assert.equal(cache.peek("root", 1, "a").url, current.url);
  second.release();
  cache.clear();
});

test("a concurrent directory revision is deferred without caching failure; real file errors remain explicit", async () => {
  const cache = new LibraryCoverCache();
  cache.setScope("root", 1);
  const stale = cache.acquire("root", 1, "a", async () => {
    throw { code: "LIBRARY_STALE_SNAPSHOT" };
  });
  assert.equal((await stale.promise).status, "cancelled");
  stale.release();
  assert.equal(cache.peek("root", 1, "a"), undefined);
  const current = cache.acquire("root", 1, "a", async () => ({
    dataUrl: image,
  }));
  assert.equal((await current.promise).status, "ready");
  current.release();
  const changed = cache.acquire("root", 1, "b", async () => {
    throw { code: "LIBRARY_FILE_CHANGED" };
  });
  assert.equal((await changed.promise).status, "error");
  assert.equal(cache.peek("root", 1, "b").status, "error");
  changed.release();
  cache.clear();
});

test("controlled equal-latency covers require fewer waiting waves without increasing request count", async (t) => {
  async function waves(concurrency) {
    const queue = new CoverScheduler(concurrency);
    let running = [],
      count = 0,
      loads = 0;
    const jobs = Array.from({ length: 21 }, () =>
      queue.enqueue(() => {
        loads++;
        return new Promise((resolve) => running.push(resolve));
      }),
    );
    await flush();
    while (running.length) {
      const batch = running;
      running = [];
      count++;
      batch.forEach((resolve) => resolve(image));
      await flush();
    }
    await Promise.all(jobs.map((job) => job.promise));
    assert.equal(loads, 21);
    return count;
  }
  const single = await waves(1),
    double = await waves(2),
    four = await waves(4);
  assert.deepEqual([single, double, four], [21, 11, 6]);
  t.diagnostic(
    JSON.stringify({
      syntheticCovers: 21,
      networkWavesBefore: double,
      networkWavesAfter: four,
      localWavesBefore: single,
      localWavesAfter: double,
      liveSpeedupMeasured: false,
    }),
  );
});
