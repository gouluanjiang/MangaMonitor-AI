// Production frontend diagnostic: unrelated selection renders must not sort the
// complete catalog again. The counter is scoped to generated synthetic titles.
import { preview } from "vite";
import { chromium, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { installSoakFixture } from "./fixture.mjs";

const options = Object.fromEntries(
  process.argv.slice(2).map((value) => value.replace(/^--/, "").split("=")),
);
const out = resolve(options.output ?? "source-render-cost-results");
mkdirSync(out, { recursive: true });
const server = await preview({
  build: { outDir: resolve(options.dist ?? "dist") },
  preview: { host: "127.0.0.1", port: 0 },
  logLevel: "error",
});
const baseURL = `http://127.0.0.1:${server.httpServer.address().port}`;
const browser = await chromium.launch({
  executablePath: options.chromium ?? process.env.SOAK_CHROMIUM,
  headless: true,
});
const context = await browser.newContext({
  baseURL,
  viewport: { width: 1440, height: 960 },
});
const blocked = [],
  errors = [],
  rows = [];
await context.route("**/*", (route) => {
  if (new URL(route.request().url()).origin === baseURL)
    return route.continue();
  blocked.push(route.request().url());
  return route.abort();
});
const page = await context.newPage();
page.on("pageerror", (error) => errors.push(error.message));
let failure;
try {
  await installSoakFixture(page);
  await page.evaluate(() => {
    const h = (window.renderCost = {
      comparisons: 0,
      compareMs: 0,
      observed: [],
    });
    const compare = String.prototype.localeCompare;
    String.prototype.localeCompare = function (...args) {
      if (!String(this).startsWith("Synthetic render "))
        return compare.apply(this, args);
      const start = performance.now();
      try {
        h.comparisons++;
        return compare.apply(this, args);
      } finally {
        h.compareMs += performance.now() - start;
      }
    };
    const items = Array.from({ length: 2000 }, (_, i) => ({
      source: "JM",
      workId: String(i + 1),
      title: `Synthetic render ${String((i * 7919) % 2000).padStart(4, "0")}`,
      authors: ["Synthetic author"],
      tags: [],
      description: null,
      favorite: null,
      chapterCount: null,
      pageCount: null,
      coverAvailable: false,
    }));
    const snapshot = {
      items,
      page: 20,
      pages: 20,
      total: 2000,
      hasMore: false,
      folders: [],
      complete: true,
      updatedAt: Date.now(),
      firstPageIds: items.slice(0, 100).map((w) => w.workId),
      pageEnds: Array.from({ length: 20 }, (_, i) => (i + 1) * 100),
    };
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      if (command === "source_catalog" && args.source === "JM")
        return { ...args, snapshot, completeSnapshot: snapshot };
      if (
        command === "source_query" &&
        args.kind === "favorites" &&
        args.source === "JM"
      ) {
        const page = Number(args.page ?? 1);
        return {
          ...args,
          items: items.slice((page - 1) * 100, page * 100),
          page,
          pages: 20,
          total: 2000,
          hasMore: page < 20,
          folders: [],
        };
      }
      return original(command, args);
    };
  });
  await page.getByTestId("nav-favorites").click();
  await expect(page.getByTestId("source-grid")).toHaveAttribute(
    "data-total-items",
    "2000",
  );
  await page.getByTestId("source-sort").selectOption("title");
  await page.waitForTimeout(300);
  const titles = await page
    .locator('[data-testid^="source-card-"] h3')
    .allTextContents();
  assert(
    titles.length > 0 &&
      titles.every((title) => title.startsWith("Synthetic render ")),
  );
  const cdp = await context.newCDPSession(page);
  await cdp.send("Profiler.enable");
  await cdp.send("Profiler.start");
  for (let round = 0; round < 20; round++) {
    await page.evaluate(() => {
      window.renderCost.comparisons = 0;
      window.renderCost.compareMs = 0;
    });
    const started = performance.now();
    await page.getByRole("button", { name: "多选", exact: true }).click();
    await page
      .getByRole("toolbar", { name: "批量下载操作", exact: true })
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    rows.push({
      round,
      ms: performance.now() - started,
      ...(await page.evaluate(() => ({
        comparisons: window.renderCost.comparisons,
        compareMs: window.renderCost.compareMs,
      }))),
    });
  }
  const { profile } = await cdp.send("Profiler.stop");
  writeFileSync(resolve(out, "renderer.cpuprofile"), JSON.stringify(profile));
  assert.deepEqual(
    await page.locator('[data-testid^="source-card-"] h3').allTextContents(),
    titles,
    "unrelated selection changes the catalog order",
  );
  assert.deepEqual(blocked, []);
  assert.deepEqual(errors, []);
  assert.equal(
    rows.reduce((sum, row) => sum + row.comparisons, 0),
    0,
    "unrelated selection repeatedly sorts the unchanged full catalog",
  );
} catch (error) {
  failure = error;
  await page.screenshot({ path: resolve(out, "failure.png") });
} finally {
  writeFileSync(
    resolve(out, "summary.json"),
    JSON.stringify(
      {
        rows,
        blocked,
        errors,
        failed: !!failure,
        error: failure?.stack,
        limit:
          "Generated 2000-work catalog and actual production render; comparator timing includes the identical diagnostic wrapper overhead before/after. No network speed claim.",
      },
      null,
      2,
    ),
  );
  await context.close();
  await browser.close();
  await new Promise((resolve) => server.httpServer.close(resolve));
}
if (failure) throw failure;
