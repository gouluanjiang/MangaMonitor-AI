// Supplemental synthetic loading/pressure diagnostic; run after the baseline.
import { chromium, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { resolve, join, extname } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { performance } from "node:perf_hooks";
import { createHash } from "node:crypto";
import { installSoakFixture } from "./fixture.mjs";

const options = Object.fromEntries(
  process.argv.slice(2).map((arg) => arg.replace(/^--/, "").split("=")),
);
const rounds = Number(options.rounds ?? 5);
const pressureMinutes = Number(options["pressure-minutes"] ?? 60);
const seed = Number(options.seed ?? 20261002) >>> 0;
assert(Number.isInteger(rounds) && rounds > 0 && rounds <= 10);
assert(pressureMinutes >= 0 && pressureMinutes <= 90);
const out = resolve(options.output ?? "performance-results");
assert(!existsSync(out), "use a fresh evidence directory");
mkdirSync(out, { recursive: true });
const runFile = promisify(execFile);
const sha = (
  await runFile("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
  })
).stdout.trim();
const workspaceStatus = (
  await runFile("git", ["status", "--porcelain=v1"], {
    encoding: "utf8",
  })
).stdout.trim();
const harnessSha256 = createHash("sha256")
  .update(readFileSync(new URL(import.meta.url)))
  .digest("hex");
const emit = (name, value) =>
  appendFileSync(join(out, name), JSON.stringify(value) + "\n");
const save = (name, value) =>
  writeFileSync(join(out, name), JSON.stringify(value, null, 2) + "\n");
let state = seed;
const random = () => {
  state ^= state << 13;
  state ^= state >>> 17;
  state ^= state << 5;
  return (state >>> 0) / 4294967296;
};
const dist = resolve(options.dist ?? "dist");
const server = createServer((request, response) => {
  const path = decodeURIComponent(
    new URL(request.url, "http://localhost").pathname,
  );
  const file = resolve(dist, "." + (path === "/" ? "/index.html" : path));
  if (!file.startsWith(dist + "/")) return response.writeHead(403).end();
  try {
    response.setHeader(
      "content-type",
      {
        ".js": "text/javascript",
        ".css": "text/css",
        ".html": "text/html",
        ".svg": "image/svg+xml",
      }[extname(file)] ?? "application/octet-stream",
    );
    response.end(readFileSync(file));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const baseURL = `http://127.0.0.1:${server.address().port}`;
const launch = () =>
  chromium.launch({
    executablePath: options.chromium ?? process.env.SOAK_CHROMIUM,
    headless: true,
  });

// Reuse the existing trusted fixture functions in one ordered init script, so
// a requested catalog is present before production React mounts. No app code is
// patched and no navigation/setup time is accidentally counted as warm loading.
const bootstrap = [];
const capture = (fn, arg) => {
  assert.equal(typeof fn, "function");
  bootstrap.push(`(${fn.toString()})(${JSON.stringify(arg) ?? "undefined"});`);
};
await installSoakFixture({
  addInitScript: capture,
  evaluate: capture,
  goto: async () => {},
});

// Prepare a finite, varied JPEG working set before measurement. The Node driver
// holds it outside the measured renderer and returns it over the mock IPC path.
const producer = await launch();
const producerPage = await producer.newPage();
const images = await producerPage.evaluate(() => {
  const make = (width, height, index, kind) => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = `hsl(${(index * 137.508) % 360} 55% 35%)`;
    ctx.fillRect(0, 0, width, height);
    for (let row = 0; row < 18; row++) {
      ctx.fillStyle = `hsl(${(index * 71 + row * 23) % 360} 60% ${30 + row * 2}%)`;
      ctx.fillRect(
        (row * 17 + index) % 70,
        (row * height) / 18,
        width - 80,
        height / 24,
      );
    }
    ctx.fillStyle = "white";
    ctx.font = "28px sans-serif";
    ctx.fillText(`SYNTHETIC ${kind} ${index}`, 12, 48);
    return canvas.toDataURL("image/jpeg", 0.7);
  };
  return {
    cover: Array.from({ length: 256 }, (_, i) => make(384, 512, i, "COVER")),
    page: Array.from({ length: 256 }, (_, i) => make(720, 1000, i, "PAGE")),
  };
});
await producer.close();
const imageKey = (key) => {
  let hash = 2166136261;
  for (const c of key) hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
  return hash >>> 0;
};

function configure({ size, background }) {
  window.soak.resize(size);
  const h = window.workflowTest;
  const p = (window.performanceProbe = {
    calls: 0,
    failures: 0,
    active: 0,
    maxActive: 0,
    covers: 0,
    maxCovers: 0,
    counts: {},
    errorsByCommand: {},
    queryPages: {},
    background,
    tick: 0,
  });
  const original = window.__TAURI_INTERNALS__.invoke;
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
    p.calls++;
    p.active++;
    p.maxActive = Math.max(p.maxActive, p.active);
    p.counts[command] = (p.counts[command] ?? 0) + 1;
    const cover = command === "source_cover" || command === "library_cover";
    if (cover) p.maxCovers = Math.max(p.maxCovers, ++p.covers);
    if (command === "source_query")
      p.queryPages[`${args.kind}:${args.source}:${args.page}`] =
        (p.queryPages[`${args.kind}:${args.source}:${args.page}`] ?? 0) + 1;
    try {
      if (cover) {
        await sleep(35);
        const identity = args.workId ?? args.entryId;
        if (typeof identity !== "string" || !identity)
          throw new Error("missing synthetic cover identity");
        return {
          ...args,
          dataUrl: await window.syntheticBenchmarkImage(
            "cover",
            `${args.source ?? "library"}:${identity}`,
          ),
        };
      }
      const value = await original(command, args);
      if (command === "reader_page")
        return {
          ...value,
          dataUrl: await window.syntheticBenchmarkImage(
            "page",
            `${args.chapterId}:${args.pageIndex}`,
          ),
          width: 720,
          height: 1000,
        };
      return value;
    } catch (error) {
      p.failures++;
      p.errorsByCommand[command] = (p.errorsByCommand[command] ?? 0) + 1;
      throw error;
    } finally {
      p.active--;
      if (cover) p.covers--;
    }
  };
  window.setBenchmarkBackground = (mode) => {
    p.background = mode;
    h.discovery.run =
      mode === "scan" || mode === "both"
        ? {
            id: "synthetic-check",
            phase: "checking",
            currentAuthor: "合成关注作者",
            currentSource: "JM",
            currentPage: 1,
            requestsUsed: 1,
            completedScopes: 0,
            totalScopes: 2,
            errorCode: null,
          }
        : null;
    for (const [index, task] of h.queue.tasks.entries()) {
      task.phase =
        (mode === "download" || mode === "both") && index === 0
          ? "downloading"
          : "paused";
      task.destinationDisplay = `C:\\Synthetic\\${task.source}-${task.workId}.zip`;
      task.filesDone = 0;
      task.filesTotal = 100000;
      task.errorCode = null;
      task.allowedActions =
        task.phase === "downloading" ? ["pause"] : ["resume", "abandon"];
      task.revision++;
    }
    h.queue.revision++;
    h.discovery.revision++;
  };
  window.setBenchmarkBackground(background);
  setInterval(() => {
    p.tick++;
    if (h.discovery.run) {
      h.discovery.run.requestsUsed++;
      h.discovery.run.currentPage = 1 + (p.tick % 6);
      h.discovery.revision++;
    }
    if (p.background === "download" || p.background === "both") {
      for (const task of h.queue.tasks)
        if (task.phase === "downloading") {
          task.filesDone++;
          task.bytesDone += 16384;
          task.updatedAt = Date.now();
        }
      h.queue.revision++;
    }
  }, 250);
}

async function newSession(spec) {
  const browserStart = performance.now();
  const browser = await launch();
  const launchMs = performance.now() - browserStart;
  const context = await browser.newContext({
    baseURL,
    viewport: { width: 1440, height: 960 },
  });
  const blocked = [],
    errors = [];
  await context.route("**/*", (route) => {
    if (new URL(route.request().url()).origin === baseURL)
      return route.continue();
    blocked.push(route.request().url());
    return route.abort();
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.on("pageerror", (error) => errors.push(error.stack));
  await page.exposeFunction(
    "syntheticBenchmarkImage",
    (kind, key) => images[kind][imageKey(key) % 256],
  );
  await page.addInitScript({
    content: `window.syntheticSetupStarted = performance.now();\n${bootstrap.join("\n")}\n(${configure.toString()})(${JSON.stringify(spec)});\nwindow.syntheticSetupMs = performance.now() - window.syntheticSetupStarted;`,
  });
  const cdp = await context.newCDPSession(page);
  await cdp.send("Performance.enable");
  return { browser, context, page, cdp, blocked, errors, launchMs, spec };
}

const nav = (page, name) => page.getByTestId("nav-" + name).click();
async function top(page) {
  const main = page.getByRole("main");
  await main.hover();
  for (let i = 0; i < 8; i++) {
    await page.mouse.wheel(0, -4000);
    await page.waitForTimeout(80);
    if (await main.evaluate((el) => el.scrollTop === 0)) break;
  }
  await expect.poll(() => main.evaluate((el) => el.scrollTop)).toBe(0);
  await page.waitForTimeout(200);
}
async function library(page) {
  await nav(page, "library");
  const back = page.getByTestId("library-detail-back");
  if (await back.isVisible()) await back.click();
}
async function imageReady(page, selector, all = true) {
  await expect
    .poll(
      () =>
        page.evaluate(
          ({ selector, all }) => {
            const visible = [...document.querySelectorAll(selector)].filter(
              (element) => {
                const r = element.getBoundingClientRect();
                return (
                  r.width > 0 &&
                  r.height > 0 &&
                  r.bottom > 0 &&
                  r.top < innerHeight
                );
              },
            );
            const ready = (element) => {
              const image =
                element instanceof HTMLImageElement
                  ? element
                  : element.querySelector("img");
              return image?.complete && image.naturalWidth > 0;
            };
            return (
              visible.length > 0 &&
              (all ? visible.every(ready) : visible.some(ready))
            );
          },
          { selector, all },
        ),
      { timeout: 15000 },
    )
    .toBe(true);
}
async function counters(page) {
  return page.evaluate(() => ({
    ...window.performanceProbe,
    readerSessions: window.soak.readerSessions,
    fixtureMs: window.syntheticSetupMs,
    unexpected: window.workflowTest.unexpectedCommands.slice(),
  }));
}
let latencyRows = 0;
async function timed(session, scenario, phase, action, detail = {}) {
  const before = await counters(session.page);
  const start = performance.now();
  const firstMs = await action(start);
  const completeMs = performance.now() - start;
  const after = await counters(session.page);
  const row = {
    at: new Date().toISOString(),
    ...session.spec,
    scenario,
    phase,
    firstMs,
    completeMs,
    ipcCalls: after.calls - before.calls,
    ...detail,
  };
  latencyRows++;
  emit("latency.ndjson", row);
  return row;
}
async function startSession(session) {
  const { page, spec } = session;
  const start = performance.now();
  await page.goto(baseURL);
  await expect(page.getByTestId("nav-library")).toBeVisible();
  const shellMs = performance.now() - start;
  await library(page);
  await expect(
    page.locator('[data-testid^="library-open-"]').first(),
  ).toBeVisible();
  const firstMs = performance.now() - start;
  await expect(page.getByTestId("library-progress")).toContainText(
    `${spec.size} 个电脑作品`,
  );
  await imageReady(page, '[data-testid^="library-cover-"]');
  const row = {
    at: new Date().toISOString(),
    ...spec,
    scenario: "startup-and-first-library",
    phase: "cold-process",
    browserLaunchMs: session.launchMs,
    shellMs,
    firstMs,
    completeMs: performance.now() - start,
    fixtureMs: (await counters(page)).fixtureMs,
    completeDefinition:
      "all catalog metadata and visible covers; offscreen covers remain bounded",
  };
  latencyRows++;
  emit("latency.ndjson", row);
}
async function libraryVisit(session, phase) {
  const { page } = session;
  await nav(page, "history");
  return timed(session, "library", phase, async (start) => {
    await library(page);
    await expect(
      page.locator('[data-testid^="library-open-"]').first(),
    ).toBeVisible();
    const first = performance.now() - start;
    await expect(page.getByTestId("library-progress")).toContainText(
      `${session.spec.size} 个电脑作品`,
    );
    await imageReady(page, '[data-testid^="library-cover-"]');
    return first;
  });
}
async function author(session, phase, name = "合成作者15") {
  const { page } = session;
  await nav(page, "discovery");
  await page
    .getByRole("group", { name: "搜索方式", exact: true })
    .getByRole("button", { name: "作者", exact: true })
    .click();
  await timed(
    session,
    "author",
    phase,
    async (start) => {
      await page.getByLabel("搜索作者名").fill(name);
      await page.getByTestId("completion-start").click();
      await expect(
        page.getByRole("tab", { name, exact: true }),
      ).toHaveAttribute("aria-selected", "true");
      await expect(
        page.locator('[data-testid^="author-update-"]').first(),
      ).toBeVisible();
      const first = performance.now() - start;
      await expect(page.getByTestId("completion-counts")).toContainText(
        "当前检查范围已读完",
      );
      const titles = await page
        .locator('[data-testid^="author-update-"] h3')
        .allTextContents();
      assert(
        titles.length && titles.every((title) => title.includes(name)),
        "cross-author result overwrite",
      );
      return first;
    },
    {
      name,
      completeDefinition:
        "all three pages from both synthetic sources, source + work-ID identity retained",
    },
  );
  if ((await page.getByRole("tab").count()) > 8)
    await page
      .getByRole("button", { name: /^关闭作者标签 / })
      .first()
      .click();
}
async function recent(session, phase) {
  const { page } = session;
  return timed(
    session,
    "recent",
    phase,
    async (start) => {
      await nav(page, "recent");
      await page.getByLabel("最近更新来源").selectOption("both");
      await expect(
        page.locator('[data-testid^="recent-work-"]').first(),
      ).toBeVisible();
      const first = performance.now() - start;
      await imageReady(page, '[data-testid^="recent-work-"] .source-cover');
      return first;
    },
    {
      completeDefinition:
        "first usable rows and visible covers, not a claim that the full feed was fetched",
    },
  );
}
async function detail(session, phase) {
  const { page } = session;
  await nav(page, "recent");
  await top(page);
  return timed(session, "detail", phase, async (start) => {
    await page
      .locator('[data-testid^="recent-work-"]')
      .first()
      .getByRole("button", { name: /打开《/ })
      .click({ button: "right" });
    await page.getByRole("menuitem", { name: "作品详细", exact: true }).click();
    await expect(page.getByTestId("source-detail-back")).toBeVisible();
    const first = performance.now() - start;
    await expect(page.getByTestId("source-read")).toBeVisible();
    return first;
  });
}
async function reader(session, phase, rapid = false) {
  const { page } = session;
  await library(page);
  await top(page);
  await timed(
    session,
    "reader",
    phase,
    async (start) => {
      await page
        .locator('[data-testid^="library-open-"]')
        .first()
        .click({ button: "right" });
      await page
        .getByRole("menuitem", { name: "程序内阅读", exact: true })
        .click();
      await expect(page.getByTestId("comic-reader")).toBeVisible();
      const first = performance.now() - start;
      await imageReady(page, "[data-reader-page] img", false);
      return first;
    },
    { completeDefinition: "a visible synthetic page decoded" },
  );
  if (rapid) {
    await page.mouse.move(720, 957);
    await page
      .getByLabel("阅读模式")
      .selectOption(random() < 0.5 ? "single" : "vertical");
    await page
      .getByLabel("选择章节")
      .selectOption(random() < 0.2 ? "two" : "one");
    await page.getByTestId("reader-viewport").focus();
    for (let i = 0; i < 12; i++)
      await page.keyboard.press(random() < 0.3 ? "ArrowLeft" : "ArrowRight");
    await imageReady(page, "[data-reader-page] img", false);
  }
  await page.getByTestId("reader-viewport").focus();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("comic-reader")).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => window.soak.readerSessions))
    .toBe(0);
}
async function tailProof(session) {
  const { page } = session;
  await nav(page, "recent");
  await page.getByLabel("最近更新来源").selectOption("both");
  for (let step = 0; step < 24; step++) {
    await page.getByRole("main").hover();
    await page.mouse.wheel(0, 16000);
    await page.waitForTimeout(300);
    const pages = (await counters(page)).queryPages;
    if (
      ["JM", "Pica"].every((source) =>
        Object.keys(pages).some((key) => key.endsWith(`:${source}:6`)),
      )
    )
      break;
  }
  const before = await counters(page);
  assert(
    ["JM", "Pica"].every((source) =>
      Object.keys(before.queryPages).some((key) =>
        key.endsWith(`:${source}:6`),
      ),
    ),
    "repeated sixth tail page was not exercised for both sources",
  );
  await page.waitForTimeout(1200);
  const after = await counters(page);
  assert.deepEqual(
    after.queryPages,
    before.queryPages,
    "completed repeated tails triggered more requests",
  );
  assert(
    !Object.keys(after.queryPages).some((key) => /:(?:JM|Pica):7$/.test(key)),
    "tail triggered unbounded extra pagination",
  );
  const ids = await page
    .locator('[data-testid^="recent-work-"]')
    .evaluateAll((elements) => elements.map((el) => el.dataset.testid));
  assert.equal(new Set(ids).size, ids.length);
  emit("coverage.ndjson", {
    at: new Date().toISOString(),
    kind: "repeated-tail",
    ...session.spec,
    queryPages: after.queryPages,
    duplicateVisibleIdentities: 0,
  });
  await top(page);
}

function processes() {
  if (process.platform !== "linux") return null;
  const all = [];
  for (const pid of readdirSync("/proc").filter((x) => /^\d+$/.test(x))) {
    try {
      const raw = readFileSync(`/proc/${pid}/stat`, "utf8");
      const fields = raw.slice(raw.lastIndexOf(")") + 2).split(" ");
      const status = readFileSync(`/proc/${pid}/status`, "utf8");
      all.push({
        pid: +pid,
        parent: +fields[1],
        startTicks: +fields[19],
        cpuTicks: +fields[11] + +fields[12],
        rssKb: +(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0),
        rssAnonKb: +(status.match(/^RssAnon:\s+(\d+)/m)?.[1] ?? 0),
        threads: +(status.match(/^Threads:\s+(\d+)/m)?.[1] ?? 0),
      });
    } catch {
      /* process exited during sampling */
    }
  }
  const ids = new Set([process.pid]);
  for (let changed = true; changed;) {
    changed = false;
    for (const p of all)
      if (ids.has(p.parent) && !ids.has(p.pid)) {
        ids.add(p.pid);
        changed = true;
      }
  }
  return all
    .filter((p) => ids.has(p.pid))
    .map((p) => {
      let fds = null,
        proportionalMemory = null;
      try {
        fds = readdirSync(`/proc/${p.pid}/fd`).length;
      } catch {
        /* process exited */
      }
      try {
        const raw = readFileSync(`/proc/${p.pid}/smaps_rollup`, "utf8");
        proportionalMemory = Object.fromEntries(
          [
            "Rss",
            "Pss",
            "Pss_Anon",
            "Private_Clean",
            "Private_Dirty",
            "Shared_Clean",
            "Shared_Dirty",
          ].map((key) => {
            const match = raw.match(new RegExp("^" + key + ":\\s+(\\d+)", "m"));
            return [key + "Kb", match ? +match[1] : null];
          }),
        );
      } catch {
        /* report unavailable rather than treating shared RSS as unique memory */
      }
      return { ...p, fds, proportionalMemory };
    });
}
async function sample(session, elapsedMs, operations) {
  const metrics = Object.fromEntries(
    (await session.cdp.send("Performance.getMetrics")).metrics.map(
      ({ name, value }) => [name, value],
    ),
  );
  const state = await counters(session.page);
  assert.equal(session.errors.length, 0, "page exception");
  assert.equal(session.blocked.length, 0, "non-local request");
  assert.deepEqual(state.unexpected, []);
  assert(state.maxCovers <= 8);
  const result = {
    at: new Date().toISOString(),
    elapsedMs,
    operations,
    ...session.spec,
    metrics,
    state,
    processes: processes(),
    runnerMemory: process.memoryUsage(),
  };
  emit(
    session.spec.round === "pressure"
      ? "pressure-metrics.ndjson"
      : "matrix-metrics.ndjson",
    result,
  );
  save("checkpoint.json", { ...result, sha, seed, complete: false });
  return result;
}

async function inspectOutsideMeasurement(session, position) {
  const driverBeforeGc = process.memoryUsage();
  const driverGcAvailable = typeof global.gc === "function";
  if (driverGcAvailable) global.gc();
  const driverAfterGc = process.memoryUsage();
  const result = {
    at: new Date().toISOString(),
    measuredDurationExcluded: true,
    position,
    driverBeforeGc,
    driverAfterGc,
    driverGcAvailable,
  };
  try {
    result.beforeGc = {
      performance: await session.cdp.send("Performance.getMetrics"),
      processes: processes(),
    };
    await session.cdp.send("HeapProfiler.collectGarbage");
    result.afterGc = {
      performance: await session.cdp.send("Performance.getMetrics"),
      processes: processes(),
      dom: await session.cdp.send("Memory.getDOMCounters"),
    };
    if (options["heap-snapshot"] !== "false") {
      const file = join(out, `pressure-${position}.heapsnapshot`);
      const writeChunk = ({ chunk }) => appendFileSync(file, chunk);
      session.cdp.on("HeapProfiler.addHeapSnapshotChunk", writeChunk);
      try {
        await session.cdp.send("HeapProfiler.takeHeapSnapshot", {
          reportProgress: false,
        });
        result.heapSnapshot = file;
      } finally {
        session.cdp.off("HeapProfiler.addHeapSnapshotChunk", writeChunk);
      }
    }
  } catch (error) {
    result.unavailable = error.message;
  }
  save(`${position}-measurement-heap.json`, result);
  return result;
}

let stop = false,
  active = null,
  failure = null,
  operations = 0;
let pressureStarted = null,
  pressureClock = null,
  pressureElapsedMs = 0,
  pressureOperations = 0;
const heapDiagnostics = {};
process.on("SIGINT", () => {
  stop = true;
});
process.on("SIGTERM", () => {
  stop = true;
});
const started = new Date().toISOString(),
  startedClock = performance.now();
const assets = readdirSync(join(dist, "assets"))
  .filter((name) => /\.(?:js|css)$/.test(name))
  .map((name) => ({
    name,
    sha256: createHash("sha256")
      .update(readFileSync(join(dist, "assets", name)))
      .digest("hex"),
  }));
const limits = [
  "Actual built frontend with synthetic IPC; this is not native Windows startup or native download/scan CPU throughput.",
  "Background modes model bounded IPC progress traffic. Native image decoding and actual source/network timing are not represented.",
  "Cold trials use a new Chromium process and app caches; host OS file caching is not controlled. Fixture initialization time is reported separately.",
  "The fixture returns 256 distinct covers and 256 distinct reader JPEGs; identities remain source + work ID even when imagery repeats.",
  "List completion means all requested metadata and visible covers; offscreen image fetching remains bounded.",
  "Garbage collection and browser heap snapshots occur only outside the timed pressure interval, before and after it. Run Node with --expose-gc for numeric driver before/after-GC measurements; no driver heap dump is written.",
  "Matrix processes are separate continuous segments. Only the pressure stage remains in one process/page for its reported duration.",
];
save("manifest.json", {
  sha,
  seed,
  started,
  rounds,
  pressureMinutes,
  node: process.version,
  platform: process.platform,
  assets,
  preparationExcluded: true,
  workspaceStatus,
  harnessSha256,
  limits,
});
console.log(
  JSON.stringify({
    event: "PERFORMANCE_STARTED",
    sha,
    started,
    rounds,
    pressureMinutes,
    output: out,
  }),
);
const trials = [];
for (let round = 1; round <= rounds; round++)
  for (const size of [24, 2000])
    for (const background of ["idle", "scan", "download", "both"])
      trials.push({ round, size, background });
for (let i = trials.length - 1; i > 0; i--) {
  const j = Math.floor(random() * (i + 1));
  [trials[i], trials[j]] = [trials[j], trials[i]];
}
let completedTrials = 0;
try {
  for (const trial of trials) {
    if (stop) break;
    active = await newSession(trial);
    await startSession(active);
    operations++;
    await libraryVisit(active, "warm");
    operations++;
    await author(active, "cold");
    operations++;
    await author(active, "warm");
    operations++;
    await recent(active, "cold");
    operations++;
    await nav(active.page, "history");
    await recent(active, "warm");
    operations++;
    await detail(active, "cold");
    operations++;
    await active.page.getByTestId("source-detail-back").click();
    await detail(active, "warm");
    operations++;
    await active.page.getByTestId("source-detail-back").click();
    await reader(active, "cold");
    operations++;
    await reader(active, "warm");
    operations++;
    await sample(active, performance.now() - startedClock, operations);
    await active.context.close();
    await active.browser.close();
    active = null;
    completedTrials++;
    console.log(
      JSON.stringify({
        event: "MATRIX_TRIAL_PASSED",
        ...trial,
        completedTrials,
        totalTrials: trials.length,
      }),
    );
  }
  if (pressureMinutes > 0 && !stop) {
    active = await newSession({
      round: "pressure",
      size: 2000,
      background: "both",
    });
    await startSession(active);
    await recent(active, "pressure-setup");
    await tailProof(active);
    heapDiagnostics.before = await inspectOutsideMeasurement(active, "before");
    pressureStarted = new Date().toISOString();
    pressureClock = performance.now();
    let nextSample = 0,
      lastMode = -1;
    console.log(
      JSON.stringify({
        event: "PRESSURE_STARTED",
        pressureStarted,
        targetMinutes: pressureMinutes,
      }),
    );
    while (
      !stop &&
      performance.now() - pressureClock < pressureMinutes * 60000
    ) {
      const elapsed = performance.now() - pressureClock;
      const modeIndex = Math.floor(elapsed / 300000) % 4;
      if (modeIndex !== lastMode) {
        lastMode = modeIndex;
        const mode = ["idle", "scan", "download", "both"][modeIndex];
        active.spec.background = mode;
        await active.page.evaluate(
          (mode) => window.setBenchmarkBackground(mode),
          mode,
        );
        emit("events.ndjson", {
          at: new Date().toISOString(),
          event: "background",
          elapsedMs: elapsed,
          mode,
        });
      }
      const choice = Math.floor(random() * 6);
      emit("operations.ndjson", {
        event: "begin",
        operation: pressureOperations + 1,
        choice,
        state,
        at: new Date().toISOString(),
        elapsedMs: elapsed,
      });
      if (choice === 0)
        await author(
          active,
          "pressure",
          `合成作者${1 + Math.floor(random() * 16)}`,
        );
      else if (choice === 1 || choice === 2)
        await reader(active, "pressure", true);
      else if (choice === 3) {
        await detail(active, "pressure");
        await active.page.getByTestId("source-detail-back").click();
      } else if (choice === 4) {
        await library(active.page);
        await top(active.page);
        await libraryVisit(active, "pressure");
        await active.page.getByRole("main").hover();
        await active.page.mouse.wheel(0, 500 + Math.floor(random() * 12000));
      } else {
        await recent(active, "pressure");
        await top(active.page);
      }
      pressureOperations++;
      operations++;
      emit("operations.ndjson", {
        event: "pass",
        operation: pressureOperations,
        choice,
        at: new Date().toISOString(),
        elapsedMs: performance.now() - pressureClock,
      });
      if (performance.now() - pressureClock >= nextSample) {
        const sampled = await sample(
          active,
          performance.now() - pressureClock,
          pressureOperations,
        );
        nextSample += 60000;
        save("pressure-checkpoint.json", {
          ...sampled,
          sha,
          seed,
          pressureStarted,
          targetMinutes: pressureMinutes,
          complete: false,
        });
        console.log(
          JSON.stringify({
            event: "PRESSURE_CHECKPOINT",
            elapsedMs: sampled.elapsedMs,
            operations: pressureOperations,
            heapMiB: sampled.metrics.JSHeapUsedSize / 1048576,
            active: sampled.state.active,
          }),
        );
      }
      await active.page.waitForTimeout(100 + Math.floor(random() * 200));
    }
    pressureElapsedMs = performance.now() - pressureClock;
    await sample(active, pressureElapsedMs, pressureOperations);
    heapDiagnostics.after = await inspectOutsideMeasurement(active, "after");
  }
} catch (error) {
  if (pressureClock !== null)
    pressureElapsedMs = performance.now() - pressureClock;
  failure = {
    at: new Date().toISOString(),
    message: error.message,
    stack: error.stack,
    spec: active?.spec,
  };
  if (active) {
    try {
      failure.state = await counters(active.page);
      failure.pageErrors = active.errors;
      await active.page.screenshot({
        path: join(out, "failure.png"),
        fullPage: true,
      });
    } catch (captureError) {
      failure.captureError = captureError.message;
    }
  }
  save("failure.json", failure);
  console.error(JSON.stringify(failure));
} finally {
  if (active) {
    await active.context.close();
    await active.browser.close();
  }
  await new Promise((done) => server.close(done));
  const ended = new Date().toISOString();
  const summary = {
    sha,
    seed,
    started,
    ended,
    durationMs: performance.now() - startedClock,
    completedTrials,
    expectedTrials: trials.length,
    operations,
    pressureStarted,
    pressureElapsedMs,
    pressureOperations,
    pressureTargetMinutes: pressureMinutes,
    complete:
      !stop &&
      !failure &&
      completedTrials === trials.length &&
      pressureElapsedMs >= pressureMinutes * 60000,
    failed: failure !== null,
    stopped: stop,
    latencyRows,
    heapDiagnostics: Object.fromEntries(
      Object.entries(heapDiagnostics).map(([name, result]) => [
        name,
        {
          available: !result.unavailable,
          unavailable: result.unavailable ?? null,
          heapSnapshot: result.heapSnapshot ?? null,
        },
      ]),
    ),
    limits,
  };
  save("summary.json", summary);
  console.log(JSON.stringify(summary));
  if (!summary.complete || failure) process.exitCode = 1;
}
