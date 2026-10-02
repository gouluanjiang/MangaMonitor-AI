import { chromium, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  appendFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
  renameSync,
} from "node:fs";
import { resolve, extname, join } from "node:path";
import { execFileSync } from "node:child_process";
import { performance } from "node:perf_hooks";
import { installSoakFixture } from "./fixture.mjs";

const opt = Object.fromEntries(
  process.argv.slice(2).map((arg) => arg.replace(/^--/, "").split("=")),
);
const minutes = Number(opt.minutes ?? 240);
const seed = Number(opt.seed ?? 20261002) >>> 0;
const out = resolve(opt.output ?? "soak-results");
assert(minutes > 0 && minutes <= 360);
mkdirSync(out, { recursive: true });
const sha = execFileSync("git", ["rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
let randomState = seed;
const random = () => {
  randomState ^= randomState << 13;
  randomState ^= randomState >>> 17;
  randomState ^= randomState << 5;
  return (randomState >>> 0) / 4294967296;
};
const pick = (values) => values[Math.floor(random() * values.length)];
const log = (file, value) =>
  appendFileSync(join(out, file), JSON.stringify(value) + "\n");
const atomic = (file, value) => {
  writeFileSync(join(out, file + ".tmp"), JSON.stringify(value, null, 2));
  renameSync(join(out, file + ".tmp"), join(out, file));
};
const dist = resolve("dist");
const server = createServer((req, res) => {
  const relative = decodeURIComponent(
    new URL(req.url, "http://localhost").pathname,
  );
  const file = resolve(
    dist,
    "." + (relative === "/" ? "/index.html" : relative),
  );
  if (!file.startsWith(dist + "/")) {
    res.writeHead(403).end();
    return;
  }
  try {
    const data = readFileSync(file);
    res.setHeader(
      "content-type",
      {
        ".js": "text/javascript",
        ".css": "text/css",
        ".html": "text/html",
        ".svg": "image/svg+xml",
      }[extname(file)] ?? "application/octet-stream",
    );
    res.end(data);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const baseURL = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({
  executablePath: opt.chromium ?? process.env.SOAK_CHROMIUM ?? undefined,
  headless: true,
});
const context = await browser.newContext({
  baseURL,
  viewport: { width: 1440, height: 960 },
});
const blocked = [];
await context.route("**/*", (route) => {
  const url = new URL(route.request().url());
  if (url.origin === baseURL) return route.continue();
  blocked.push(url.origin);
  return route.abort();
});
const page = await context.newPage();
page.setDefaultTimeout(7000);
page.setDefaultNavigationTimeout(15000);
const pageErrors = [];
page.on("pageerror", (error) => {
  pageErrors.push(error.message);
  log("page-errors.ndjson", {
    at: new Date().toISOString(),
    error: error.stack,
  });
});
await installSoakFixture(page);
await page.getByTestId("nav-library").click();
await page.getByTestId("library-refresh").click();
await expect(page.getByTestId("library-refresh")).toBeEnabled();
const cdp = await context.newCDPSession(page);
await cdp.send("Performance.enable");
const started = new Date().toISOString();
const clockStart = performance.now();
const deadline = clockStart + minutes * 60000;
let operations = 0,
  passed = 0,
  failed = 0,
  nextSample = 0,
  small = true,
  stop = false;
const coverage = {},
  failures = [],
  quarantined = new Set();
const ring = [];
process.on("SIGINT", () => {
  stop = true;
});
process.on("SIGTERM", () => {
  stop = true;
});
const identity = {
  sha,
  seed,
  started,
  targetMinutes: minutes,
  browser: browser.version(),
  platform: process.platform,
  node: process.version,
  pid: process.pid,
  baseURL,
};
atomic("manifest.json", {
  ...identity,
  applicationBaseline: "9afbb4b0470b3939b9f8f8e0848dcc1c3a8b3827",
  preparationExcluded: true,
  nativeBoundary: "synthetic",
  continuousPage: true,
});
console.log(
  JSON.stringify({ event: "SOAK_STARTED", ...identity, output: out }),
);

function processSample() {
  // Sum RSS (shared pages may be double-counted); report CPU seconds, fd and
  // thread counts separately. No system-wide process names/arguments are logged.
  if (process.platform !== "linux")
    return { unavailable: "Linux /proc sampler only" };
  const all = [];
  for (const entry of readdirSync("/proc").filter((n) => /^\d+$/.test(n))) {
    try {
      const raw = readFileSync(`/proc/${entry}/stat`, "utf8");
      const fields = raw.slice(raw.lastIndexOf(")") + 2).split(" ");
      const status = readFileSync(`/proc/${entry}/status`, "utf8");
      all.push({
        pid: Number(entry),
        parent: Number(fields[1]),
        cpuTicks: Number(fields[11]) + Number(fields[12]),
        rssKb: Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0),
        threads: Number(status.match(/^Threads:\s+(\d+)/m)?.[1] ?? 0),
      });
    } catch {
      /* A process may exit between the two read-only probes. */
    }
  }
  const ids = new Set([process.pid]);
  for (let changed = true; changed;) {
    changed = false;
    for (const row of all)
      if (ids.has(row.parent) && !ids.has(row.pid)) {
        ids.add(row.pid);
        changed = true;
      }
  }
  return all
    .filter((row) => ids.has(row.pid))
    .map((row) => {
      try {
        return { ...row, fds: readdirSync(`/proc/${row.pid}/fd`).length };
      } catch {
        return { ...row, fds: null };
      }
    });
}
async function sample(final = false) {
  const elapsedMs = performance.now() - clockStart;
  const metrics = Object.fromEntries(
    (await cdp.send("Performance.getMetrics")).metrics.map(
      ({ name, value }) => [name, value],
    ),
  );
  const state = await page.evaluate(() => ({
    requests: window.soak.requests,
    failures: window.soak.failures,
    injected: window.soak.injected,
    retryAttempts: window.soak.retryAttempts,
    active: window.soak.active,
    maxActive: window.soak.maxActive,
    activeCovers: window.soak.activeCovers,
    maxCovers: window.soak.maxCovers,
    activeQueries: window.soak.activeQueries,
    maxQueries: window.soak.maxQueries,
    readerSessions: window.soak.readerSessions,
    counts: window.soak.counts,
    size: window.soak.size,
    tabs: document.querySelectorAll('[role="tab"]').length,
    history: JSON.parse(
      localStorage.getItem("synthetic-viewing-history") ?? '{"entries":[]}',
    ).entries.length,
    unexpected: window.workflowTest.unexpectedCommands,
  }));
  const value = {
    at: new Date().toISOString(),
    elapsedMs,
    operations,
    passed,
    failed,
    metrics,
    processes: processSample(),
    runnerMemory: process.memoryUsage(),
    runnerCpu: process.cpuUsage(),
    state,
    coverage,
  };
  log("metrics.ndjson", value);
  atomic("checkpoint.json", {
    ...identity,
    ...value,
    randomState: randomState >>> 0,
    stopped: stop,
    complete: final && !stop && elapsedMs >= minutes * 60000,
    quarantined: [...quarantined],
    failures,
    resumption:
      "Run with a new output directory and the recorded seed; a resumed browser is a NEW continuous segment. Never add segments to claim a single-process duration.",
  });
  console.log(
    JSON.stringify({
      event: final ? "SOAK_ENDED" : "CHECKPOINT",
      at: value.at,
      elapsedMinutes: +(elapsedMs / 60000).toFixed(2),
      operations,
      passed,
      failed,
      heapMiB: +(metrics.JSHeapUsedSize / 1048576).toFixed(1),
      active: state.active,
      history: state.history,
    }),
  );
  assert.equal(blocked.length, 0, "attempted non-loopback browser request");
  assert.equal(pageErrors.length, 0, "uncaught renderer exception");
  assert.equal(
    state.unexpected.length,
    0,
    "fixture received unsupported IPC; classify as harness failure",
  );
  assert(state.history <= 100, "history exceeded product capacity");
  assert(
    state.maxCovers <= 8,
    "combined local/remote cover concurrency exceeded two four-slot schedulers",
  );
}
const nav = async (name) => {
  await page.keyboard.press("Escape");
  await page.getByTestId("nav-" + name).click();
};
const library = async () => {
  await nav("library");
  const back = page.getByTestId("library-detail-back");
  if (await back.isVisible()) await back.click();
};
const search = async () => {
  await nav("discovery");
  await page
    .getByRole("group", { name: "搜索方式", exact: true })
    .getByRole("button", { name: "作者", exact: true })
    .click();
};
const showToolbar = async () => {
  const vp = page.viewportSize();
  await page.mouse.move(vp.width / 2, vp.height - 3);
};
const top = async () => {
  await page.getByRole("main").hover();
  // A wheel arriving on the first restored frame may be consumed by layout.
  // Several real input events model the user's continued upward scroll.
  for (let i = 0; i < 8; i++) {
    await page.mouse.wheel(0, -2000);
    await page.waitForTimeout(80);
    if (await page.getByRole("main").evaluate((el) => el.scrollTop === 0))
      break;
  }
  await expect
    .poll(() => page.getByRole("main").evaluate((el) => el.scrollTop))
    .toBe(0);
  await page.waitForTimeout(250);
};
const cases = {
  async author() {
    await search();
    const name = `合成作者${1 + Math.floor(random() * 16)}`;
    const t = performance.now();
    await page.getByLabel("搜索作者名").fill(name);
    await page.getByTestId("completion-start").click();
    await expect(page.getByRole("tab", { name, exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(
      page.locator('[data-testid^="author-update-"]').first(),
    ).toBeVisible();
    const firstMs = performance.now() - t;
    await expect(page.getByTestId("completion-counts")).toContainText(
      "当前检查范围已读完",
      { timeout: 10000 },
    );
    const completeMs = performance.now() - t;
    const titles = await page
      .locator('[data-testid^="author-update-"] h3')
      .allTextContents();
    assert(
      titles.length && titles.every((text) => text.includes(name)),
      "author tab contains another author's result",
    );
    log("latency.ndjson", {
      operation: operations,
      scenario: "author",
      name,
      firstMs,
      completeMs,
      size: small ? 24 : 2000,
    });
    await page
      .getByLabel("更新来源", { exact: true })
      .selectOption(pick(["JM", "Pica", "all"]));
    if ((await page.getByRole("tab").count()) > 8)
      await page
        .getByRole("button", { name: /^关闭作者标签 / })
        .first()
        .click();
  },
  async tabs() {
    await search();
    const tabs = page.getByRole("tab");
    const count = await tabs.count();
    if (!count) return cases.author();
    await tabs.nth(Math.floor(random() * count)).click();
    const name = await page.getByLabel("搜索作者名").inputValue();
    const before = await tabs.count();
    await page.getByTestId("completion-start").click();
    assert.equal(
      await tabs.count(),
      before,
      "same complete author name created duplicate tab",
    );
    assert.equal(await page.getByLabel("搜索作者名").inputValue(), name);
    if (count > 3 && random() < 0.5)
      await page
        .getByRole("button", { name: `关闭作者标签 ${name}`, exact: true })
        .click();
  },
  async recent() {
    await nav("recent");
    await page
      .getByLabel("最近更新来源")
      .selectOption(pick(["JM", "Pica", "both"]));
    await expect(
      page.locator('[data-testid^="recent-work-"]').first(),
    ).toBeVisible();
    const refresh = page.getByRole("button", {
      name: "刷新最近更新",
      exact: true,
    });
    if (random() < 0.3 && (await refresh.isEnabled())) await refresh.click();
    await expect(
      page.locator('[data-testid^="recent-work-"]').first(),
    ).toBeVisible();
    const main = page.getByRole("main");
    await main.hover();
    await page.mouse.wheel(0, pick([500, 900, -600, 1800]));
    await page.waitForTimeout(220);
    const before = await main.evaluate((el) => el.scrollTop);
    await nav("history");
    await nav("recent");
    await expect
      .poll(() => main.evaluate((el) => el.scrollTop))
      .toBeGreaterThanOrEqual(before - 25);
    const ids = await page
      .locator('[data-testid^="recent-work-"]')
      .evaluateAll((els) => els.map((el) => el.dataset.testid));
    assert.equal(new Set(ids).size, ids.length, "duplicate recent identity");
  },
  async detail() {
    await nav("recent");
    await top();
    const card = page.locator('[data-testid^="recent-work-"]').first();
    await expect(card).toBeVisible();
    await card
      .getByRole("button", { name: /打开《/ })
      .click({ button: "right" });
    await page.getByRole("menuitem", { name: "作品详细", exact: true }).click();
    await expect(page.getByTestId("source-detail-back")).toBeVisible();
    await expect(page.getByTestId("source-read")).toBeVisible();
    await page.getByTestId("source-detail-back").click();
    await expect(page.getByTestId("recent-panel")).toBeVisible();
  },
  async reader() {
    await library();
    await top();
    await page
      .locator('[data-testid^="library-open-"]')
      .first()
      .click({ button: "right" });
    await page
      .getByRole("menuitem", { name: "程序内阅读", exact: true })
      .click();
    await expect(page.getByTestId("comic-reader")).toBeVisible();
    await showToolbar();
    await page
      .getByLabel("阅读模式")
      .selectOption(pick(["single", "vertical"]));
    await page.getByLabel("选择章节").selectOption(pick(["one", "two"]));
    const viewport = page.getByTestId("reader-viewport");
    await viewport.focus();
    for (let i = 0, n = 3 + Math.floor(random() * 12); i < n; i++)
      await page.keyboard.press(pick(["ArrowRight", "ArrowLeft", "ArrowDown"]));
    await page.waitForTimeout(120);
    await viewport.focus();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("comic-reader")).toHaveCount(0);
    await expect
      .poll(() => page.evaluate(() => window.soak.readerSessions))
      .toBe(0);
  },
  async history() {
    await nav("history");
    const before = await page.locator(".viewing-history-list li").count();
    assert(before <= 100);
    if (operations % 7 === 0) {
      await page.getByLabel("记录浏览历史").uncheck();
      await library();
      await top();
      await page
        .locator('[data-testid^="library-open-"]')
        .first()
        .click({ button: "right" });
      await page
        .getByRole("menuitem", { name: "作品详细", exact: true })
        .click();
      await nav("history");
      assert.equal(
        await page.locator(".viewing-history-list li").count(),
        before,
      );
      await page.getByLabel("记录浏览历史").check();
    }
    if (operations % 101 === 0 && before) {
      await page.getByRole("button", { name: "清空历史", exact: true }).click();
      await page.getByRole("button", { name: "确认清空", exact: true }).click();
      await expect(page.locator(".viewing-history-list li")).toHaveCount(0);
    }
  },
  async special() {
    await nav("special");
    await expect(page.getByTestId("special-counts")).toBeVisible();
    const expected = await page.evaluate(
      () => window.soak.special.updates.filter((x) => x.readAt === null).length,
    );
    await expect(page.getByTestId("special-counts")).toContainText(
      `未读作品 ${expected} 部`,
    );
    await page.evaluate(() => {
      window.soak.specialFail = true;
    });
    await page.getByRole("button", { name: "刷新状态", exact: true }).click();
    await expect(
      page.getByTestId("special-panel").getByRole("alert"),
    ).toContainText("已有记录保留");
    await expect(page.getByTestId("special-counts")).toContainText(
      `未读作品 ${expected} 部`,
    );
    await page.evaluate(() => {
      window.soak.specialFail = false;
    });
    await page.getByRole("button", { name: "刷新状态", exact: true }).click();
  },
  async queue() {
    await page.evaluate(() => {
      for (const task of window.workflowTest.queue.tasks) {
        task.phase = "error";
        task.errorCode = "SOURCE_TIMEOUT";
        task.allowedActions = ["retry", "abandon"];
        task.revision++;
      }
      window.workflowTest.queue.revision++;
    });
    await nav("queue");
    await page.getByTestId("download-read").click();
    await expect(page.getByTestId("download-read")).toBeEnabled();
    await page.getByTestId("download-filter-error").click();
    const id = (700 + Math.floor(random() * 2)).toString(16).padStart(64, "0");
    const before = await page.evaluate(() =>
      JSON.stringify(window.workflowTest.library),
    );
    await page.getByTestId("download-retry-" + id).click();
    await page.getByTestId("download-filter-active").click();
    await expect(page.getByTestId("download-phase-" + id)).toHaveText(
      "正在下载",
    );
    await page.getByTestId("download-pause-" + id).click();
    await expect(page.getByTestId("download-phase-" + id)).toHaveText("已暂停");
    assert.equal(
      await page.evaluate(() => JSON.stringify(window.workflowTest.library)),
      before,
    );
  },
  async recycleCancel() {
    await library();
    await top();
    const cover = page.locator('[data-testid^="library-open-"]').first();
    const id = await cover.getAttribute("data-testid");
    const before = await page.evaluate(() =>
      JSON.stringify(window.workflowTest.library),
    );
    await cover.click({ button: "right" });
    await expect(
      page.getByTestId("reader-cover-actions").getByRole("menuitem"),
    ).toHaveCount(5);
    await page.getByRole("menuitem", { name: "删除漫画", exact: true }).click();
    await expect(page.getByTestId(id)).toBeVisible();
    assert.equal(
      await page.evaluate(() => JSON.stringify(window.workflowTest.library)),
      before,
    );
  },
  async scan() {
    await nav("completion");
    const start = page.getByTestId("completion-start");
    if (await start.isEnabled()) {
      await page.getByLabel("检查作者").selectOption("合成关注作者");
      await start.click();
      await expect(page.getByTestId("completion-progress")).toBeVisible();
      if (operations % 4 === 0) {
        await page
          .getByRole("button", { name: "停止本次检查", exact: true })
          .click();
        await expect
          .poll(() =>
            page.evaluate(() => window.workflowTest.discovery.run.phase),
          )
          .toBe("cancelled");
        return;
      }
      await nav("special");
      await page.getByRole("button", { name: "刷新状态", exact: true }).click();
      await page.evaluate(() => window.workflowTest.finishCheck());
      await nav("completion");
      await expect(page.getByTestId("author-update-JM:102")).toBeVisible();
    }
  },
  async library() {
    await library();
    const main = page.getByRole("main");
    await main.hover();
    await page.mouse.wheel(0, pick([1200, -1500, 4000]));
    await page.waitForTimeout(100);
    const before = await main.evaluate((el) => el.scrollTop);
    await nav("history");
    await library();
    await expect
      .poll(() => main.evaluate((el) => el.scrollTop))
      .toBeGreaterThanOrEqual(before - 25);
  },
  async late() {
    await search();
    const name = "合成迟到作者99";
    await page.evaluate(() =>
      window.soak.faults.push({
        command: "source_query",
        kind: "author",
        source: "JM",
        remaining: 1,
        delay: 900,
      }),
    );
    await page.getByLabel("搜索作者名").fill(name);
    await page.getByTestId("completion-start").click();
    await page
      .getByRole("button", { name: `关闭作者标签 ${name}`, exact: true })
      .click();
    await nav("history");
    await page.waitForTimeout(1100);
    await search();
    await expect(page.getByRole("tab", { name, exact: true })).toHaveCount(0);
    assert(
      !(
        await page
          .locator('[data-testid^="author-update-"] h3')
          .allTextContents()
      ).some((text) => text.includes(name)),
      "closed author wrote into active tab",
    );
  },
  async fault() {
    await nav("recent");
    await page.getByLabel("最近更新来源").selectOption("JM");
    await top();
    await expect(
      page.locator('[data-testid^="recent-work-"]').first(),
    ).toBeVisible();
    const old = await page
      .locator('[data-testid^="recent-work-"]')
      .first()
      .getAttribute("data-testid");
    const code = pick([
      "SOURCE_TIMEOUT",
      "SOURCE_UNAVAILABLE",
      "SOURCE_RATE_LIMITED",
      "SOURCE_RESPONSE_INVALID",
    ]);
    await page.evaluate(
      (code) =>
        window.soak.faults.push({
          command: "source_query",
          source: "JM",
          remaining: 1,
          code,
        }),
      code,
    );
    await page
      .getByRole("button", { name: "刷新最近更新", exact: true })
      .click();
    await expect(
      page.getByTestId("recent-panel").getByRole("alert").first(),
    ).toBeVisible();
    await expect(page.getByTestId(old)).toBeVisible();
    await page
      .getByTestId("recent-panel")
      .getByRole("button", { name: "重试读取", exact: true })
      .first()
      .click();
    await expect(
      page.locator('[data-testid^="recent-work-"]').first(),
    ).toBeVisible();
  },
};
const schedule = [
  "author",
  "tabs",
  "recent",
  "recent",
  "detail",
  "reader",
  "reader",
  "history",
  "special",
  "scan",
  "library",
  "late",
  "fault",
  "queue",
  "recycleCancel",
];
let completed = false;
try {
  await sample();
  while (!stop && performance.now() < deadline) {
    if (
      small &&
      performance.now() - clockStart > Math.min(15 * 60000, minutes * 6000)
    ) {
      await page.evaluate(() => window.soak.resize(2000));
      await library();
      await page.getByTestId("library-refresh").click();
      await expect(page.getByTestId("library-refresh")).toBeEnabled();
      small = false;
      log("events.ndjson", {
        at: new Date().toISOString(),
        event: "large-library",
        size: 2000,
      });
    }
    const scenario = pick(schedule.filter((name) => !quarantined.has(name)));
    assert(scenario, "no independent scenarios remain");
    const entry = {
      operation: ++operations,
      at: new Date().toISOString(),
      scenario,
      randomState: randomState >>> 0,
    };
    ring.push(entry);
    if (ring.length > 100) ring.shift();
    log("operations.ndjson", { ...entry, event: "begin" });
    const t = performance.now();
    try {
      await cases[scenario]();
      passed++;
      coverage[scenario] = (coverage[scenario] ?? 0) + 1;
      log("operations.ndjson", {
        ...entry,
        event: "pass",
        ms: performance.now() - t,
      });
    } catch (error) {
      failed++;
      const failure = {
        ...entry,
        error: String(error.stack ?? error),
        ms: performance.now() - t,
      };
      failures.push(failure);
      log("failures.ndjson", failure);
      atomic(`failure-${operations}.json`, {
        failure,
        precedingOperations: ring,
        boundary: await page.evaluate(() => ({
          calls: window.soak.recentCalls,
          unexpected: window.workflowTest.unexpectedCommands,
          active: window.soak.active,
        })),
      });
      await page.screenshot({ path: join(out, `failure-${operations}.png`) });
      // Retain the assertion and evidence. Stop repeating an affected scenario
      // after three occurrences; unrelated operations continue in the SAME page.
      if (failures.filter((f) => f.scenario === scenario).length >= 3)
        quarantined.add(scenario);
      await page.evaluate(() => {
        window.soak.faults = [];
        window.soak.specialFail = false;
      });
      const reader = page.getByTestId("comic-reader");
      if (await reader.isVisible()) {
        await page.getByTestId("reader-viewport").focus();
        await page.keyboard.press("Escape");
      }
      await page.keyboard.press("Escape");
    }
    if (performance.now() >= nextSample) {
      await sample();
      nextSample = performance.now() + 60000;
    }
    // Bounded human input cadence between active multi-step operations.
    await page.waitForTimeout(100 + Math.floor(random() * 250));
  }
  completed = !stop && performance.now() >= deadline;
  await sample(completed);
  await context.storageState({
    path: join(out, "synthetic-browser-state.json"),
  });
} finally {
  atomic("summary.json", {
    ...identity,
    ended: new Date().toISOString(),
    continuousMs: performance.now() - clockStart,
    operations,
    passed,
    failed,
    coverage,
    complete: completed,
    quarantined: [...quarantined],
    blockedRequests: blocked,
    pageErrors,
  });
  await context.close();
  await browser.close();
  await new Promise((done) => server.close(done));
}
process.exitCode = !completed ? 3 : failed ? 1 : 0;
