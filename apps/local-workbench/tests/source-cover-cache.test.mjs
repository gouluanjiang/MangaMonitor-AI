import test from "node:test";
import assert from "node:assert/strict";
import {
  CoverSessionCache,
  coverErrorMessage,
} from "../src/source-cover-cache.ts";
import {
  queueCover,
  SOURCE_COVER_CONCURRENCY,
  SOURCE_COVER_MAX_WAITING,
} from "../src/source-cover-queue.ts";
const jm = { source: "JM", sessionId: "synthetic-jm-session" };
const pica = { source: "Pica", sessionId: "synthetic-pica-session" };
const image =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
function assets(size = 80 * 1024) {
  const created = [],
    revoked = [];
  return {
    created,
    revoked,
    encode(dataUrl) {
      const url = "blob:synthetic/" + (created.length + 1);
      created.push(url);
      return {
        url,
        size: typeof size === "function" ? size(dataUrl) : size,
        revoke() {
          revoked.push(url);
        },
      };
    },
  };
}
async function read(cache, scope, workId, load) {
  const lease = cache.acquire(scope, workId, load);
  try {
    return await lease.promise;
  } finally {
    lease.release();
  }
}

test("1876 successful thumbnails survive card releases and subsequent reads without another loader call", async () => {
  // Charge realistic 80 KiB compressed covers without allocating 150 MiB of test images.
  const encoded = assets();
  const cache = new CoverSessionCache({ encode: encoded.encode });
  let calls = 0;
  const load = async () => {
    calls++;
    return image;
  };
  for (let i = 1; i <= 1876; i++) {
    const lease = cache.acquire(jm, String(i), load);
    await lease.promise;
    lease.release();
  }
  for (let i = 1; i <= 1876; i++) {
    assert.equal(cache.peek(jm, String(i))?.status, "ready");
    assert.equal((await read(cache, jm, String(i), load)).status, "ready");
  }
  assert.equal(calls, 1876);
  assert.equal(encoded.created.length, 1876);
  assert.deepEqual(encoded.revoked, []);
  cache.retainScopes([]);
  assert.equal(encoded.revoked.length, 1876);
});

test("the default encoder exposes a Blob URL instead of retaining a data URL", async () => {
  const cache = new CoverSessionCache();
  const result = await cache.acquire(jm, "blob", async () => image).promise;
  assert.equal(result.status, "ready");
  assert.match(result.url, /^blob:/);
  assert.doesNotMatch(JSON.stringify(result), /base64|dataUrl/);
  cache.retainScopes([]);
  assert.equal(cache.peek(jm, "blob"), undefined);
});

test("same-key consumers share in-flight work and unmounting all consumers retains a successful response", async () => {
  const cache = new CoverSessionCache();
  let release,
    calls = 0;
  const waiting = new Promise((resolve) => {
    release = resolve;
  });
  const load = async () => {
    calls++;
    await waiting;
    return image;
  };
  const a = cache.acquire(jm, "1", load),
    b = cache.acquire(jm, "1", load);
  await Promise.resolve();
  a.release();
  b.release();
  release();
  await Promise.all([a.promise, b.promise]);
  assert.equal(calls, 1);
  assert.equal(cache.peek(jm, "1")?.status, "ready");
  await cache.acquire(jm, "1", load).promise;
  assert.equal(calls, 1);
});

test("a last consumer leaving cancels queued work without creating a failed cover", async () => {
  const cache = new CoverSessionCache();
  let release;
  const waiting = new Promise((resolve) => {
    release = resolve;
  });
  const blockers = Array.from({ length: SOURCE_COVER_CONCURRENCY }, () =>
    queueCover(() => waiting),
  );
  await Promise.resolve();
  let calls = 0;
  const lease = cache.acquire(jm, "queued", async () => {
    calls++;
    return image;
  });
  lease.release();
  assert.deepEqual(await lease.promise, {
    status: "deferred",
    reason: "cancelled",
  });
  assert.equal(cache.peek(jm, "queued"), undefined);
  release(null);
  await Promise.all(blockers.map((task) => task.promise));
  await cache.acquire(jm, "queued", async () => {
    calls++;
    return image;
  }).promise;
  assert.equal(calls, 1);
});

test("queue pressure is not negatively cached and a later visible request can succeed", async () => {
  const cache = new CoverSessionCache();
  let release;
  const waiting = new Promise((resolve) => {
    release = resolve;
  });
  const occupied = Array.from({ length: SOURCE_COVER_CONCURRENCY }, () =>
    queueCover(() => waiting),
  );
  await Promise.resolve();
  occupied.push(
    ...Array.from({ length: SOURCE_COVER_MAX_WAITING }, () =>
      queueCover(() => waiting),
    ),
  );
  const settled = Promise.allSettled(occupied.map((task) => task.promise));
  assert.deepEqual(
    await cache.acquire(jm, "overflow", async () => image).promise,
    { status: "deferred", reason: "busy" },
  );
  assert.equal(cache.peek(jm, "overflow"), undefined);
  release(null);
  await settled;
  assert.equal(
    (await cache.acquire(jm, "overflow", async () => image).promise).status,
    "ready",
  );
});

test("fixed errors persist across mounts, explicit retry clears failures, and malformed diagnostic text never leaks", async () => {
  const cache = new CoverSessionCache();
  let calls = 0;
  const failed = async () => {
    calls++;
    throw {
      code: "SOURCE_COVER_ACCESS_DENIED",
      message: "SECRET URL AND TOKEN",
    };
  };
  const result = await cache.acquire(pica, "denied", failed).promise;
  assert.deepEqual(result, {
    status: "error",
    code: "SOURCE_COVER_ACCESS_DENIED",
  });
  assert.match(coverErrorMessage(result.code), /401\/403/);
  assert.doesNotMatch(JSON.stringify(result), /SECRET/);
  await cache.acquire(pica, "denied", failed).promise;
  assert.equal(calls, 1);
  cache.retryFailures(pica);
  assert.equal(
    (await cache.acquire(pica, "denied", async () => image).promise).status,
    "ready",
  );
  const unknown = await cache.acquire(pica, "unknown", async () => {
    throw { code: "token-secret", message: "SECRET" };
  }).promise;
  assert.deepEqual(unknown, { status: "error", code: "SOURCE_UNAVAILABLE" });
});

test("transient failures retry three times with increasing jittered delays and remain exhausted across remounts", async () => {
  let now = 1000,
    calls = 0;
  const delays = [];
  const cache = new CoverSessionCache({
    now: () => now,
    random: () => 0.5,
    wait: async (delay) => {
      delays.push(delay);
      now += delay;
    },
  });
  const load = async () => {
    calls++;
    throw { code: "SOURCE_TIMEOUT" };
  };
  const a = cache.acquire(jm, "retry", load),
    b = cache.acquire(jm, "retry", load);
  assert.deepEqual(await a.promise, {
    status: "error",
    code: "SOURCE_TIMEOUT",
  });
  await b.promise;
  a.release();
  b.release();
  assert.equal(calls, 4);
  assert.deepEqual(delays, [1150, 2150, 4150]);
  now += 3600000;
  await read(cache, jm, "retry", load);
  assert.equal(calls, 4, "scroll and elapsed time cannot start another cycle");
  cache.retryFailure(jm, "retry");
  assert.equal(
    (await read(cache, jm, "retry", async () => image)).status,
    "ready",
  );
});

test("retry waits release network capacity, stop offscreen and retain their remaining budget", async () => {
  let now = 0,
    calls = 0,
    waiting;
  const cache = new CoverSessionCache({
    now: () => now,
    random: () => 0,
    wait: (delay, signal) =>
      new Promise((resolve) => {
        waiting = () => {
          now += delay;
          resolve();
        };
        signal.addEventListener("abort", resolve, { once: true });
      }),
  });
  const failed = async () => {
    calls++;
    throw { code: "SOURCE_CONNECTION_FAILED" };
  };
  const a = cache.acquire(jm, "paused", failed);
  while (!waiting) await new Promise((resolve) => setImmediate(resolve));
  assert.equal(
    (await read(cache, jm, "other", async () => image)).status,
    "ready",
  );
  a.release();
  assert.equal((await a.promise).status, "deferred");
  assert.equal(calls, 1);
  waiting = undefined;
  const b = cache.acquire(jm, "paused", failed);
  while (calls < 4) {
    while (!waiting) await new Promise((resolve) => setImmediate(resolve));
    const resume = waiting;
    waiting = undefined;
    resume();
    await new Promise((resolve) => setImmediate(resolve));
  }
  assert.equal((await b.promise).status, "error");
  b.release();
  assert.equal(calls, 4);
});

test("server deadlines apply to automatic and manual retry, while auth, absent and invalid resources do not auto-loop", async () => {
  let now = 0,
    calls = 0;
  const delays = [];
  const cache = new CoverSessionCache({
    now: () => now,
    random: () => 0,
    wait: async (delay) => {
      delays.push(delay);
      now += delay;
    },
  });
  const result = await read(cache, jm, "limited", async () => {
    calls++;
    if (calls < 4)
      throw { code: "SOURCE_COVER_RATE_LIMITED", retryAfterMs: 15000 };
    return image;
  });
  assert.equal(result.status, "ready");
  assert.deepEqual(delays, [15000, 15000, 15000]);
  for (const code of [
    "AUTH_REQUIRED",
    "SESSION_EXPIRED",
    "SOURCE_COVER_ACCESS_DENIED",
    "SOURCE_COVER_NOT_FOUND",
    "SOURCE_COVER_INVALID",
  ]) {
    let attempts = 0;
    await read(cache, jm, code, async () => {
      attempts++;
      throw { code };
    });
    await read(cache, jm, code, async () => {
      attempts++;
      return image;
    });
    assert.equal(attempts, 1);
  }
  await read(cache, jm, "manual", async () => {
    throw { code: "SOURCE_COVER_ACCESS_DENIED", retryAfterMs: 9000 };
  });
  cache.retryFailure(jm, "manual");
  await read(cache, jm, "manual", async () => image);
  assert.equal(
    delays.at(-1),
    9000,
    "explicit retry cannot bypass a server deadline",
  );
});

test("a clicked failed cover clears only that source, session and work error", async () => {
  const cache = new CoverSessionCache();
  const nextSession = { ...jm, sessionId: "another-session" };
  for (const [scope, id] of [
    [jm, "1"],
    [jm, "2"],
    [pica, "1"],
    [nextSession, "1"],
  ])
    await read(cache, scope, id, async () => null);
  await read(cache, jm, "ready", async () => image);
  cache.retryFailure(jm, "1");
  assert.equal(cache.peek(jm, "1"), undefined);
  for (const [scope, id] of [
    [jm, "2"],
    [pica, "1"],
    [nextSession, "1"],
  ])
    assert.equal(cache.peek(scope, id)?.status, "error");
  assert.equal(cache.peek(jm, "ready")?.status, "ready");
  assert.equal((await read(cache, jm, "1", async () => image)).status, "ready");
});

test("source and opaque account sessions are isolated and logout discards late old-session results", async () => {
  const encoded = assets(128);
  const cache = new CoverSessionCache({ encode: encoded.encode });
  const old = await cache.acquire(jm, "1", async () => image).promise;
  await cache.acquire(pica, "1", async () => image).promise;
  const next = { ...jm, sessionId: "synthetic-jm-next" };
  assert.equal(cache.peek(next, "1"), undefined);
  let release;
  const late = cache.acquire(
    jm,
    "2",
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  await Promise.resolve();
  cache.retainScopes([next, pica]);
  release(image);
  assert.deepEqual(await late.promise, {
    status: "deferred",
    reason: "cancelled",
  });
  assert.equal(cache.peek(jm, "1"), undefined);
  assert.equal(cache.peek(jm, "2"), undefined);
  assert.equal(cache.peek(pica, "1")?.status, "ready");
  assert.deepEqual(encoded.revoked, [old.url]);
  assert.equal(
    encoded.created.length,
    2,
    "late logged-out responses are not even encoded into Blobs",
  );
});

test("bounded retention uses entry and compressed Blob budgets, revokes evicted URLs, and retries image decoding", async () => {
  const encoded = assets((value) => (value === "oversized" ? 4096 : 128));
  const cache = new CoverSessionCache({
    maximumEntries: 2,
    maximumBytes: 2048,
    encode: encoded.encode,
  });
  for (const id of ["1", "2", "1", "3"])
    await read(cache, jm, id, async () => image);
  assert.equal(cache.peek(jm, "1")?.status, "ready");
  assert.equal(cache.peek(jm, "2"), undefined);
  assert.deepEqual(encoded.revoked, ["blob:synthetic/2"]);
  await cache.acquire(jm, "oversized", async () => "oversized").promise;
  assert.deepEqual(cache.peek(jm, "oversized"), {
    status: "error",
    code: "COVER_CACHE_LIMIT",
  });
  assert.ok(encoded.revoked.includes("blob:synthetic/4"));
  const failedUrl = cache.peek(jm, "1").url;
  cache.decodeFailed(jm, "1", failedUrl);
  assert.deepEqual(cache.peek(jm, "1"), {
    status: "error",
    code: "COVER_DECODE_FAILED",
  });
  cache.retryFailures(jm);
  assert.equal(
    (await cache.acquire(jm, "1", async () => image).promise).status,
    "ready",
  );
  cache.decodeFailed(jm, "1", failedUrl);
  assert.equal(cache.peek(jm, "1")?.status, "ready");
});

test("visible leases pin their Blob URLs and stale errors after eviction do not poison the replacement", async () => {
  const encoded = assets(128);
  const cache = new CoverSessionCache({
    maximumEntries: 2,
    encode: encoded.encode,
  });
  const a = cache.acquire(jm, "1", async () => image),
    b = cache.acquire(jm, "2", async () => image);
  const [first, second] = await Promise.all([a.promise, b.promise]);
  const waiting = await read(cache, jm, "3", async () => image);
  assert.deepEqual(waiting, { status: "deferred", reason: "busy" });
  assert.ok(!encoded.revoked.includes(first.url));
  assert.ok(!encoded.revoked.includes(second.url));
  a.release();
  await read(cache, jm, "3", async () => image);
  assert.ok(encoded.revoked.includes(first.url));
  assert.ok(!encoded.revoked.includes(second.url));
  cache.decodeFailed(jm, "1", first.url);
  assert.equal(cache.peek(jm, "1"), undefined);
  b.release();
  cache.retainScopes([]);
});
