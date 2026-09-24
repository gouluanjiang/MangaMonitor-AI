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

test("actual viewport entry promotes a prefetched cover and leaving both regions releases it", async () => {
  const observers = [];
  const oldWindow = globalThis.window;
  const oldObserver = globalThis.IntersectionObserver;
  class Observer {
    constructor(callback, options) {
      this.callback = callback;
      this.options = options;
      observers.push(this);
    }
    observe() {}
    disconnect() {
      this.disconnected = true;
    }
    emit(value) {
      this.callback([{ isIntersecting: value }]);
    }
  }
  globalThis.window = { IntersectionObserver: Observer };
  globalThis.IntersectionObserver = Observer;
  try {
    const root = {};
    const states = [];
    const stop = observeCover({ closest: () => root }, (value) =>
      states.push(value),
    );
    assert.ok(observers.every((observer) => observer.options.root === root));
    observers[1].emit(true);
    await flush();
    observers[0].emit(true);
    await flush();
    observers[0].emit(false);
    await flush();
    observers[1].emit(false);
    await flush();
    assert.deepEqual(states, ["nearby", "visible", "nearby", null]);
    stop();
    observers[0].emit(true);
    await flush();
    assert.equal(states.length, 4);
    assert.ok(observers.every((observer) => observer.disconnected));
  } finally {
    if (oldWindow === undefined) delete globalThis.window;
    else globalThis.window = oldWindow;
    if (oldObserver === undefined) delete globalThis.IntersectionObserver;
    else globalThis.IntersectionObserver = oldObserver;
  }
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
