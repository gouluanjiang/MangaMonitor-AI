// A white-box browser regression using the application's own module instance.
// A separate production diagnostic build exposes two read-only functions.
// All source/native calls use the synthetic soak fixture.
import { build, preview } from "vite";
import { chromium, expect } from "@playwright/test";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { installSoakFixture } from "./fixture.mjs";

const options = Object.fromEntries(
  process.argv.slice(2).map((value) => value.replace(/^--/, "").split("=")),
);
const rounds = Number(options.rounds ?? 40);
assert(Number.isInteger(rounds) && rounds > 0 && rounds <= 5000);
const out = resolve(options.output ?? "tab-lifetime-results");
mkdirSync(out, { recursive: true });
const buildDir = resolve(out, "diagnostic-build");
await build({
  build: { outDir: buildDir, emptyOutDir: false },
  logLevel: "error",
  plugins: [
    {
      name: "read-only-browse-diagnostic",
      transformIndexHtml: {
        order: "pre",
        handler: () => [
          {
            tag: "script",
            attrs: { type: "module" },
            children:
              'import { readBrowsePosition } from "/src/browse-session.ts"; import { accountScope } from "/src/source-types.ts"; window.browseLifetimeDiagnostic = { readBrowsePosition, accountScope };',
            injectTo: "head",
          },
        ],
      },
    },
  ],
});
const server = await preview({
  build: { outDir: buildDir },
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
  scopes = [],
  checkpoints = [];
await context.route("**/*", (route) => {
  if (new URL(route.request().url()).origin === baseURL)
    return route.continue();
  blocked.push(route.request().url());
  return route.abort();
});
const page = await context.newPage();
page.on("pageerror", (error) => errors.push(error.message));
const cdp = await context.newCDPSession(page);
const read = () =>
  page.evaluate(async (scopes) => {
    const module = window.browseLifetimeDiagnostic;
    return scopes.map((scope) => {
      const value = module.readBrowsePosition(scope);
      return {
        scope,
        retained: value !== undefined,
        keys: value?.keys.length ?? 0,
      };
    });
  }, scopes);
let failure;
try {
  await installSoakFixture(page);
  await page.getByTestId("nav-discovery").click();
  await page
    .getByRole("group", { name: "搜索方式", exact: true })
    .getByRole("button", { name: "作者", exact: true })
    .click();
  for (let i = 0; i < rounds; i++) {
    const name = `合成保留核验${i % 16}`;
    await page.getByLabel("搜索作者名").fill(name);
    await page.getByTestId("completion-start").click();
    await expect(page.getByTestId("completion-counts")).toContainText(
      "当前检查范围已读完",
    );
    const id = await page
      .getByRole("tab", { name, exact: true })
      .getAttribute("id");
    const scope = await page.evaluate(async (id) => {
      const { accountScope } = window.browseLifetimeDiagnostic;
      const accounts = window.workflowTest.accounts
        .map(accountScope)
        .filter(Boolean);
      return JSON.stringify([
        "search",
        id,
        JSON.stringify(accounts),
        "",
        "all",
        "missing",
        "",
        false,
        "updated-desc",
        false,
      ]);
    }, id);
    scopes.push(scope);
    // A real scroll event makes this a captured populated list, not an empty
    // scope created solely by the diagnostic.
    await page.getByRole("main").hover();
    await page.mouse.wheel(0, 100);
    await expect
      .poll(async () => (await read()).at(-1).keys)
      .toBeGreaterThan(0);
    await page
      .getByRole("button", { name: `关闭作者标签 ${name}`, exact: true })
      .click();
    await expect(page.getByRole("tab")).toHaveCount(0);
    // Flush the unmount and its position-saving layout-effect cleanup.
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    if ((i + 1) % 10 === 0 || i + 1 === rounds) {
      await cdp.send("HeapProfiler.collectGarbage");
      const retained = await read();
      checkpoints.push({
        closed: i + 1,
        retainedScopes: retained.filter((value) => value.retained).length,
        retainedKeys: retained.reduce((sum, value) => sum + value.keys, 0),
        heap: await cdp.send("Runtime.getHeapUsage"),
      });
      writeFileSync(
        resolve(out, "checkpoints.json"),
        JSON.stringify(checkpoints, null, 2),
      );
    }
  }
  assert.deepEqual(blocked, []);
  assert.deepEqual(errors, []);
  assert.equal(
    checkpoints.at(-1).retainedScopes,
    0,
    "closed author tabs retain browsing snapshots after unmount and garbage collection",
  );
} catch (error) {
  failure = error;
  await page.screenshot({ path: resolve(out, "failure.png") });
} finally {
  writeFileSync(
    resolve(out, "summary.json"),
    JSON.stringify(
      {
        rounds,
        checkpoints,
        blocked,
        errors,
        failed: !!failure,
        error: failure?.stack,
        retained: await read(),
        limitation:
          "Production diagnostic entry exposes read-only snapshot access; no native/network latency claim",
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
