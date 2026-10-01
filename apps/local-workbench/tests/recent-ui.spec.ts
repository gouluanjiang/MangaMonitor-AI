import { openUnifiedSearch } from "./browse-ui-helpers.ts";
import { installBrowsingMarkerFixture } from "./browsing-marker-fixture.ts";
import type { BrowsingBaseline } from "../src/browsing-markers.ts";
import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { initialPreferences } from "../src/preferences.ts";
import { emptyLibrary } from "../src/library-types.ts";
import type { DownloadSnapshot, DownloadPlan } from "../src/download-types.ts";
import type {
  AccountSummary,
  Source,
  SourceWork,
} from "../src/source-types.ts";

declare global {
  interface Window {
    recentTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      accounts: AccountSummary[];
      version: number;
      failPage: number | null;
      holdPage: number | null;
      failSource: Source | null;
      holdSource: Source | null;
      release?: () => void;
      total: number;
      blIds: number[];
      aiIds: number[];
      catalogIds: number[];
      catalogOtherIds: number[];
      unverified: boolean;
      detailFailIds: number[];
      holdDetails: boolean;
      releaseDetails: (() => void)[];
      queue: DownloadSnapshot;
      observedWheelDelta?: number;
      directionInputs?: { x: number; y: number; shift: boolean }[];
    };
  }
}

test.use({ storageState: { cookies: [], origins: [] } });
const nativeScrollbarTest = test.extend({
  // Headless Chromium otherwise hides the native scrollbar but retains the
  // reserved gutter, so a mouse drag selects content instead of moving a thumb.
  launchOptions: { ignoreDefaultArgs: ["--hide-scrollbars"] },
});
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const captured: string[] = [];
  errors.set(page, captured);
  page.on("pageerror", (error) => captured.push(error.message));
  await page.setViewportSize({ width: 1672, height: 1020 });
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.recentTest.calls.filter((call) =>
        /discovery_start|source_favorite|source_follow$|source_matches_|phone_library_|library_scan/.test(
          call.command,
        ),
      ),
    ),
  ).toEqual([]);
});

// Synthetic IPC only: no accounts, source traffic, files or download execution.
async function install(
  page: Page,
  options: {
    blIds?: number[];
    aiIds?: number[];
    catalogIds?: number[];
    catalogOtherIds?: number[];
    holdPage?: number;
    unverified?: boolean;
    holdDetails?: boolean;
    detailFailIds?: number[];
    detailTags?: Record<string, string[]>;
    covers?: boolean;
    femaleIds?: number[];
    retainedCount?: number;
    total?: number;
    recentPages?: number[][];
    recentPagesBySource?: Partial<Record<Source, number[][]>>;
    datesBySource?: Partial<Record<Source, Record<string, string | null>>>;
    browsingBaselines?: Partial<Record<Source, BrowsingBaseline>>;
  } = {},
) {
  await installBrowsingMarkerFixture(page);
  await page.addInitScript(
    ({ preferences, library, options }) => {
      const hooks = (window.recentTest = {
        calls: [],
        accounts: (["JM", "Pica"] as const).map((source) => ({
          source,
          sessionId: "fixture-" + source,
          accountId: "fixture-account-" + source,
          displayName: "合成账号",
          state: "connected",
          remembered: false,
          errorCode: null,
        })),
        version: 0,
        failPage: null,
        holdPage: options.holdPage ?? null,
        failSource: null,
        holdSource: null,
        total: options.total ?? 40,
        blIds: options.blIds ?? [],
        aiIds: options.aiIds ?? [],
        catalogIds: options.catalogIds ?? [],
        catalogOtherIds: options.catalogOtherIds ?? [],
        unverified: options.unverified ?? false,
        detailFailIds: options.detailFailIds ?? [],
        holdDetails: options.holdDetails ?? false,
        releaseDetails: [],
        queue: { revision: 0, tasks: [] },
      } as Window["recentTest"]);
      for (const account of hooks.accounts) {
        const baseline = options.browsingBaselines?.[account.source];
        const key =
          "synthetic-browsing-markers:" +
          JSON.stringify([account.source, account.accountId, "recent"]);
        if (baseline && !localStorage.getItem(key))
          localStorage.setItem(
            key,
            JSON.stringify({ revision: 1, value: { version: 1, baseline } }),
          );
      }
      const work = (
        source: Source,
        id: number,
        session: string,
      ): SourceWork => ({
        source,
        workId: source === "JM" ? String(id) : String(id).padStart(24, "0"),
        title: source + " 合成最近更新 " + id + " · " + session,
        authors: ["合成作者"],
        description: null,
        tags: hooks.blIds.includes(id)
          ? ["耽美花園"]
          : id % 3 === 1
            ? ["中文"]
            : id % 3 === 2
              ? ["生肉"]
              : [],
        categories: hooks.aiIds.includes(id)
          ? ["AI"]
          : options.femaleIds?.includes(id)
            ? ["女性向"]
            : undefined,
        favorite: null,
        chapterCount: 1,
        pageCount: 20,
        coverAvailable: options.covers ?? false,
        sourceUpdatedAt:
          options.datesBySource?.[source]?.[id] !== undefined
            ? options.datesBySource![source]![id]
            : id % 3 === 0
              ? null
              : new Date(1800000000000 - id * 1000).toISOString(),
      });
      const boundaryEdge = async (item: SourceWork | undefined) =>
        item
          ? {
              workId: item.workId,
              fingerprint: Array.from(
                new Uint8Array(
                  await crypto.subtle.digest(
                    "SHA-256",
                    new TextEncoder().encode(JSON.stringify(item)),
                  ),
                ),
                (byte) => byte.toString(16).padStart(2, "0"),
              ).join(""),
            }
          : null;
      let plan: DownloadPlan | null = null;
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        value: {
          invoke: async (
            command: string,
            args: Record<string, unknown> = {},
          ) => {
            hooks.calls.push({ command, args: structuredClone(args) });
            const browsing = window.syntheticBrowsingMarkers.call(
              command,
              args,
              hooks.accounts,
            );
            if (browsing !== undefined) return browsing;
            if (command === "source_recent_history")
              return {
                source: args.source,
                sessionId: args.sessionId,
                items: Array.from(
                  { length: options.retainedCount ?? 0 },
                  (_, i) =>
                    work(args.source as Source, i + 1, String(args.sessionId)),
                ),
                revision: 0,
                coverage: {
                  headIds: [],
                  checkedAt: null,
                  pagesRead: 0,
                  reachedEnd: false,
                  joinedPrevious: false,
                  initialWindow: false,
                  errorCode: null,
                },
              };
            if (command === "read_preferences")
              return { revision: 0, value: preferences };
            if (command === "library_read") return structuredClone(library);
            if (command === "source_accounts")
              return structuredClone(hooks.accounts);
            if (command === "jm_download_read")
              return structuredClone(hooks.queue);
            if (command === "discovery_read")
              return {
                scopes: args.scopes,
                revision: 1,
                run: null,
                authors: [],
                records: [...hooks.catalogIds, ...hooks.catalogOtherIds].map(
                  (id) => ({
                    work: {
                      ...work("Pica", id, "fixture-Pica"),
                      authors: hooks.catalogOtherIds.includes(id)
                        ? ["其他作者"]
                        : ["合成作者"],
                    },
                    matchedAuthors: ["合成作者"],
                    authorVerified: !hooks.catalogOtherIds.includes(id),
                    observedAt: 1800000000000,
                    scanId: "synthetic-catalog",
                  }),
                ),
              };
            if (command === "download_inventory_read")
              return {
                rootId: library.rootId,
                revision: 1,
                libraryRevision: 1,
                items: [
                  {
                    source: "JM",
                    workId: "1",
                    libraryEntryId: "b".repeat(64),
                    localFiles: "present",
                  },
                ],
              };
            if (command === "source_following")
              return {
                source: args.source,
                sessionId: args.sessionId,
                revision: 0,
                works: [],
                authors: [],
              };
            if (command === "source_rank_options")
              return {
                source: args.source,
                sessionId: args.sessionId,
                options:
                  args.source === "JM"
                    ? {
                        categories: [{ id: "42", label: "合成第42期" }],
                        periods: [{ id: "manga", label: "日漫" }],
                      }
                    : {
                        categories: [],
                        periods: [{ id: "week", label: "周榜" }],
                      },
              };
            if (command === "source_cover")
              return {
                source: args.source,
                sessionId: args.sessionId,
                workId: args.workId,
                dataUrl:
                  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
              };
            if (command === "source_query") {
              const source = args.source as Source;
              const session = args.sessionId as string;
              const pageNumber = Number(args.page);
              const offset = hooks.version * 100;
              const total = hooks.total;
              const start = pageNumber === 1 ? 1 : pageNumber === 2 ? 20 : 39;
              const ids =
                args.kind === "detail"
                  ? [Number(args.query)]
                  : args.kind === "ranking"
                    ? [901, 902]
                    : (options.recentPagesBySource?.[source]?.[
                        pageNumber - 1
                      ] ??
                      options.recentPages?.[pageNumber - 1] ??
                      Array.from(
                        {
                          length: Math.max(0, Math.min(20, total - start + 1)),
                        },
                        (_, i) => offset + start + i,
                      ));
              const response = {
                source,
                sessionId: session,
                items: ids.map((id) => ({
                  ...work(source, id, session),
                  ...(args.kind === "detail" && options.detailTags?.[String(id)]
                    ? { tags: options.detailTags[String(id)] }
                    : {}),
                })),
                contentVerifiedIds: hooks.unverified
                  ? []
                  : ids.map((id) =>
                      source === "JM"
                        ? String(id)
                        : String(id).padStart(24, "0"),
                    ),
                contentVerifiedUntil: hooks.unverified
                  ? null
                  : Date.now() + 86_400_000,
                page: pageNumber,
                total: args.kind === "recent" ? total : ids.length,
                pages:
                  args.kind === "recent"
                    ? source === "JM"
                      ? null
                      : Math.ceil((total + 1) / 20)
                    : 1,
                hasMore:
                  args.kind === "recent"
                    ? source === "JM"
                      ? null
                      : start + ids.length - 1 < total
                    : false,
                folders: [],
              };
              if (args.kind === "detail") {
                if (hooks.holdDetails)
                  await new Promise<void>((resolve) =>
                    hooks.releaseDetails.push(resolve),
                  );
                if (hooks.detailFailIds.includes(Number(args.query)))
                  throw { code: "SOURCE_TIMEOUT" };
              }
              if (
                args.kind === "recent" &&
                hooks.holdPage === pageNumber &&
                (!hooks.holdSource || hooks.holdSource === source)
              ) {
                hooks.holdPage = null;
                await new Promise<void>((resolve) => {
                  hooks.release = resolve;
                });
              }
              if (
                args.kind === "recent" &&
                hooks.failPage === pageNumber &&
                (!hooks.failSource || hooks.failSource === source)
              )
                throw { code: "SOURCE_TIMEOUT" };
              // The native JM recent endpoint includes these raw edge proofs.
              // Keeping them in every JM fixture catches IPC contract drift.
              return source === "JM" && args.kind === "recent"
                ? {
                    ...response,
                    jmSearchBoundary: {
                      first: await boundaryEdge(response.items[0]),
                      last: await boundaryEdge(response.items.at(-1)),
                    },
                  }
                : response;
            }
            if (command === "jm_download_prepare") {
              const scope = args.scope as { source: Source; sessionId: string };
              const value = work(
                scope.source,
                Number(args.input),
                scope.sessionId,
              );
              plan = {
                planId: "c".repeat(64),
                revision: 0,
                source: scope.source,
                workId: value.workId,
                title: value.title,
                authors: value.authors,
                destinationDisplay: "C:\\Synthetic\\" + value.title + ".zip",
                rootId: library.rootId,
                generation: 1,
              };
              return structuredClone(plan);
            }
            if (command === "jm_download_confirm") {
              if (!plan || args.planId !== plan.planId)
                throw { code: "DOWNLOAD_PLAN_STALE" };
              hooks.queue = {
                revision: hooks.queue.revision + 1,
                tasks: [
                  {
                    id: "d".repeat(64),
                    revision: 1,
                    source: plan.source,
                    workId: plan.workId,
                    title: plan.title,
                    destinationDisplay: plan.destinationDisplay,
                    phase: "queued",
                    filesDone: 0,
                    filesTotal: null,
                    bytesDone: 0,
                    errorCode: null,
                    allowedActions: ["pause"],
                    libraryEntryId: null,
                    localFiles: null,
                    updatedAt: 1800000000000,
                  },
                ],
              };
              return structuredClone(hooks.queue);
            }
            if (command === "jm_download_cancel_plan") return null;
            throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
          },
        },
      });
    },
    {
      options,
      preferences: initialPreferences(),
      library: {
        ...emptyLibrary(),
        rootId: "a".repeat(64),
        rootPath: "C:\\Synthetic",
        revision: 1,
        generation: 1,
        phase: "complete",
        freshness: "live",
      },
    },
  );
  await page.goto("/");
  await openUnifiedSearch(page, "作品关键词");
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "最近更新", exact: true })
    .click();
}

test("combined first heads sort by website date, keep source identity and share single-source reads", async ({
  page,
}) => {
  await install(page, {
    recentPagesBySource: { JM: [[1, 2]], Pica: [[1, 2]] },
    datesBySource: {
      JM: { 1: "2026-09-30", 2: null },
      Pica: { 1: "2026-10-01", 2: "2026-09-29" },
    },
  });
  await page.getByLabel("最近更新来源").selectOption("both");
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 4 部");
  await expect
    .poll(() =>
      page
        .getByTestId("recent-grid")
        .locator("article")
        .evaluateAll((items) =>
          items.map((item) => item.getAttribute("data-testid")),
        ),
    )
    .toEqual([
      "recent-work-Pica:" + "1".padStart(24, "0"),
      "recent-work-JM:1",
      "recent-work-Pica:" + "2".padStart(24, "0"),
      "recent-work-JM:2",
    ]);
  await expect(recentCard(page, "JM", 1)).toContainText("JM ·");
  await expect(recentCard(page, "Pica", 1)).toContainText("哔咔 ·");
  await page.getByLabel("最近更新来源").selectOption("Pica");
  await page.getByLabel("最近更新来源").selectOption("both");
  expect(
    (await recentCalls(page)).map((args) => [args.source, args.page]),
  ).toEqual([
    ["Pica", 1],
    ["JM", 1],
  ]);
  expect(await detailCalls(page)).toEqual([]);
});

test("combined late source head keeps the current visible anchor and settles without flicker", async ({
  page,
}) => {
  // End Pica's first page so this gesture exercises the late JM head only;
  // combined continuation/partial paging has its own separate regressions.
  await install(page, { total: 20 });
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await page.evaluate(() => {
    window.recentTest.holdPage = 1;
    window.recentTest.holdSource = "JM";
  });
  await page.getByLabel("最近更新来源").selectOption("both");
  await expect
    .poll(() => page.evaluate(() => !!window.recentTest.release))
    .toBe(true);
  const main = page.getByRole("main");
  // A source switch can still be restoring across frames. Real wheel input
  // cancels that restore; assigning scrollTop bypasses the user-input path.
  await main.hover();
  await page.mouse.wheel(0, 700);
  // Native wheel/compositor delivery is asynchronous. Keep JM held until the
  // actual displacement arrives, then capture the same visible work as before.
  await expect
    .poll(() => main.evaluate((element) => Math.abs(element.scrollTop - 700)))
    .toBeLessThanOrEqual(2);
  await expect(recentCard(page, "Pica", 8)).toBeInViewport();
  const anchor = await captureRecentAnchor(page);
  await page.evaluate(() => window.recentTest.release!());
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 40 部");
  await expectRecentAnchor(page, anchor);
  await expectStationaryRecentGrid(page);
  expect(
    (await recentCalls(page)).map((args) => [args.source, args.page]),
  ).toEqual([
    ["Pica", 1],
    ["JM", 1],
  ]);
});

test("combined partial failure preserves the successful source and retries only the failed page", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    window.recentTest.failPage = 1;
    window.recentTest.failSource = "JM";
  });
  await page.getByLabel("最近更新来源").selectOption("both");
  await expect(page.getByTestId("recent-source-progress-JM")).toContainText(
    "已读内容保留",
  );
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  await expect(page.getByTestId("recent-progress")).toContainText(
    "部分来源未完成",
  );
  await page.evaluate(() => {
    window.recentTest.failPage = null;
  });
  await page.getByRole("button", { name: "重试JM", exact: true }).click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 40 部");
  expect(
    (await recentCalls(page)).map((args) => [args.source, args.page]),
  ).toEqual([
    ["Pica", 1],
    ["JM", 1],
    ["JM", 1],
  ]);
  expect(await detailCalls(page)).toEqual([]);
});

test("recent browsing badges survive source and section switches, exclude history tails and clear next launch", async ({
  page,
}) => {
  const id = (value: number) => String(value).padStart(24, "0");
  await install(page, {
    recentPages: [[100, 1, 2]],
    retainedCount: 20,
    browsingBaselines: {
      Pica: { knownIds: [id(1)], headIds: [id(1)], reachedEnd: false },
    },
  });
  await expect(
    recentCard(page, "Pica", 100).getByTestId("browsing-new-badge"),
  ).toHaveText("新增");
  await expect(
    recentCard(page, "Pica", 2).getByTestId("browsing-new-badge"),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("recent-grid").getByTestId("browsing-new-badge"),
  ).toHaveCount(1);
  await page.getByLabel("最近更新来源").selectOption("JM");
  await page.getByLabel("最近更新来源").selectOption("Pica");
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-recent").click();
  await expect(
    recentCard(page, "Pica", 100).getByTestId("browsing-new-badge"),
  ).toHaveText("新增");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.recentTest.calls.filter(
            (call) =>
              call.command === "browsing_markers_write" &&
              call.args.source === "Pica",
          ).length,
      ),
    )
    .toBeGreaterThan(0);
  await page.reload();
  await page.getByTestId("nav-recent").click();
  await expect(recentCard(page, "Pica", 100)).toBeVisible();
  await expect(page.getByTestId("recent-browsing-note")).not.toContainText(
    "正在读取浏览基线",
  );
  await expect(
    page.getByTestId("recent-grid").getByTestId("browsing-new-badge"),
  ).toHaveCount(0);
  expect(await detailCalls(page)).toEqual([]);
});

test("first recent activation seeds all saved history and an unjoined later head remains uncertain", async ({
  page,
}) => {
  await install(page, { recentPages: [[100, 101]], retainedCount: 40 });
  await expect(page.getByTestId("recent-browsing-note")).toContainText(
    "首次浏览已建立基线",
  );
  await expect(
    page.getByTestId("recent-grid").getByTestId("browsing-new-badge"),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.recentTest.calls.filter(
            (call) => call.command === "browsing_markers_write",
          ).length,
      ),
    )
    .toBeGreaterThan(0);
  const saved = await page.evaluate(
    () =>
      JSON.parse(
        localStorage.getItem(
          "synthetic-browsing-markers:" +
            JSON.stringify(["Pica", "fixture-account-Pica", "recent"]),
        )!,
      ).value.baseline,
  );
  expect(saved.knownIds).toHaveLength(42);
  await page.evaluate(() => {
    window.syntheticBrowsingMarkers.seed(
      "Pica",
      "fixture-account-Pica",
      "recent",
      {
        knownIds: ["90".padStart(24, "0")],
        headIds: ["90".padStart(24, "0")],
        reachedEnd: false,
      },
    );
  });
  await page.reload();
  await page.getByTestId("nav-recent").click();
  await expect(page.getByTestId("recent-browsing-note")).toContainText(
    "尚未完整接回上次浏览的头部",
  );
  await expect(
    page.getByTestId("recent-grid").getByTestId("browsing-new-badge"),
  ).toHaveCount(0);
});

test("browse markers follow stable account identity across renewed sessions and isolate another account", async ({
  page,
}) => {
  const head = "1".padStart(24, "0");
  await install(page, {
    recentPages: [[100, 1]],
    browsingBaselines: {
      Pica: { knownIds: [head], headIds: [head], reachedEnd: false },
    },
  });
  await expect(
    recentCard(page, "Pica", 100).getByTestId("browsing-new-badge"),
  ).toBeVisible();
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.recentTest.accounts[1].sessionId = "renewed-Pica";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.getByTestId("nav-recent").click();
  await expect(
    recentCard(page, "Pica", 100).getByTestId("browsing-new-badge"),
  ).toBeVisible();
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.recentTest.accounts[1].sessionId = "other-Pica";
    window.recentTest.accounts[1].accountId = "other-account-Pica";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.getByTestId("nav-recent").click();
  await expect(page.getByTestId("recent-browsing-note")).toContainText(
    "首次浏览已建立基线",
  );
  await expect(
    recentCard(page, "Pica", 100).getByTestId("browsing-new-badge"),
  ).toHaveCount(0);
});

const recentCalls = (page: Page) =>
  page.evaluate(() =>
    window.recentTest.calls
      .filter((c) => c.command === "source_query" && c.args.kind === "recent")
      .map((c) => c.args),
  );
const detailCalls = (page: Page) =>
  page.evaluate(() =>
    window.recentTest.calls.filter(
      (call) => call.command === "source_query" && call.args.kind === "detail",
    ),
  );

test("unknown recent cards load covers and remain usable without any tag detail requests", async ({
  page,
}) => {
  await install(page, {
    unverified: true,
    covers: true,
    holdDetails: true,
    detailFailIds: [4],
  });
  await expect(
    recentCard(page, "Pica", 3).locator(".cover-interaction"),
  ).toBeVisible();
  await expect
    .poll(() => page.getByTestId("recent-grid").locator("img").count())
    .toBeGreaterThan(0);
  await expect(
    recentCard(page, "Pica", 4).getByRole("button", { name: "下载到漫画库" }),
  ).toBeEnabled();
  await expect(
    page.getByText("核验通过后显示作品", { exact: true }),
  ).toHaveCount(0);
  expect(await detailCalls(page)).toEqual([]);
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-recent").click();
  await expect(recentCard(page, "Pica", 3)).toBeVisible();
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(recentCard(page, "JM", 3)).toBeVisible();
  await expect(recentCard(page, "Pica", 3)).toHaveCount(0);
  expect(await detailCalls(page)).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/recent-passive-labels.png" });
});

test("available explicit labels filter immediately while unknown labels cause no requests", async ({
  page,
}) => {
  await install(page, {
    unverified: true,
    blIds: [1],
    aiIds: [2],
    femaleIds: [3],
  });
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await expect(recentCard(page, "Pica", 1)).toHaveCount(0);
  await expect(recentCard(page, "Pica", 2)).toHaveCount(0);
  await expect(recentCard(page, "Pica", 3)).toBeVisible();
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(recentCard(page, "JM", 4)).toBeVisible();
  await expect(recentCard(page, "JM", 3)).toHaveCount(0);
  expect(await detailCalls(page)).toEqual([]);
});

test("tags obtained by an explicit detail visit update the recent list without a verification crawl", async ({
  page,
}) => {
  await install(page, { unverified: true, detailTags: { "1": ["AI作畫"] } });
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  expect(await detailCalls(page)).toEqual([]);
  await recentCard(page, "Pica", 1)
    .getByRole("button", { name: /^Pica 合成最近更新 1 ·/ })
    .click();
  await expect(
    page.getByText("该作品已按内容偏好隐藏。", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "返回列表", exact: true }).click();
  await expect(recentCard(page, "Pica", 1)).toHaveCount(0);
  await expect(recentCard(page, "Pica", 2)).toBeVisible();
  expect(await detailCalls(page)).toHaveLength(1);
});
const recentCard = (page: Page, source: Source, id: number) =>
  page.getByTestId(
    `recent-work-${source}:${source === "JM" ? String(id) : String(id).padStart(24, "0")}`,
  );

async function captureRecentAnchor(page: Page) {
  const anchor = await page.getByTestId("recent-grid").evaluate((grid) => {
    const main = grid.closest("main")!.getBoundingClientRect();
    const cards = Array.from(grid.querySelectorAll("article[data-testid]"));
    const visible =
      cards.find((card) => {
        const rect = card.getBoundingClientRect();
        return rect.top >= main.top && rect.bottom <= main.bottom;
      }) ??
      cards.find((card) => {
        const rect = card.getBoundingClientRect();
        return rect.top < main.bottom && rect.bottom > main.top;
      });
    return visible
      ? {
          key: visible.getAttribute("data-testid")!,
          y: visible.getBoundingClientRect().top - main.top,
        }
      : null;
  });
  expect(
    anchor,
    "The current viewport must contain an existing recent card",
  ).not.toBeNull();
  return anchor!;
}

async function expectRecentAnchor(
  page: Page,
  anchor: { key: string; y: number },
) {
  const card = page.getByTestId(anchor.key);
  await expect(card).toBeInViewport();
  await expect
    .poll(
      async () => {
        const y = await card.evaluate(
          (element) =>
            element.getBoundingClientRect().top -
            element.closest("main")!.getBoundingClientRect().top,
        );
        return Math.abs(y - anchor.y);
      },
      {
        message: `Appending a page must preserve the visible position of ${anchor.key}`,
      },
    )
    .toBeLessThanOrEqual(4);
}

async function expectStationaryRecentGrid(page: Page) {
  const samples = await page
    .getByTestId("recent-grid")
    .evaluate(async (grid) => {
      const main = grid.closest("main")!;
      const samples: {
        key: string | null;
        y: number;
        scroll: number;
        height: number;
      }[] = [];
      for (let frame = 0; frame < 90; frame++) {
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => resolve()),
        );
        if (frame < 30) continue;
        const viewport = main.getBoundingClientRect();
        const card = Array.from(
          grid.querySelectorAll("article[data-testid]"),
        ).find((item) => {
          const rect = item.getBoundingClientRect();
          return rect.top >= viewport.top && rect.top < viewport.bottom;
        });
        samples.push({
          key: card?.getAttribute("data-testid") ?? null,
          y: card ? card.getBoundingClientRect().top - viewport.top : -1,
          scroll: main.scrollTop,
          height: grid.getBoundingClientRect().height,
        });
      }
      return samples;
    });
  expect(samples.every((sample) => sample.key !== null)).toBe(true);
  expect(new Set(samples.map((sample) => sample.key)).size).toBe(1);
  for (const field of ["y", "scroll", "height"] as const) {
    const values = samples.map((sample) => sample[field]);
    expect(
      Math.max(...values) - Math.min(...values),
      field,
    ).toBeLessThanOrEqual(1);
  }
  const geometry = await page.getByTestId("recent-grid").evaluate((grid) => {
    const rows = Array.from(grid.querySelectorAll(".source-virtual-row"));
    return {
      count: grid.querySelectorAll("article").length,
      overlaps: rows
        .slice(1)
        .some(
          (row, index) =>
            row.getBoundingClientRect().top <
            rows[index].getBoundingClientRect().bottom - 1,
        ),
    };
  });
  expect(geometry.overlaps).toBe(false);
  expect(geometry.count).toBeLessThan(100);
}

async function varyRecentRowMetadata(page: Page) {
  // Real catalog rows differ in fallback-font, metadata and action heights.
  // Deterministic extra line space models that variation on every CI platform.
  const selectors = Array.from({ length: 1000 }, (_, index) => index + 1)
    .filter((id) => Math.floor((id - 1) / 7) % 2 === 1)
    .map(
      (id) =>
        `[data-testid="recent-work-Pica:${String(id).padStart(24, "0")}"] .source-card-state`,
    );
  await page.addStyleTag({
    content: `${selectors.join(",")} { padding-bottom: 21px; }`,
  });
}

for (const source of ["JM", "Pica"] as const) {
  test(`${source} overlapping live/history pages keep the visible saved work in place until explicit refresh`, async ({
    page,
  }) => {
    await install(page, {
      retainedCount: 100,
      holdPage: 2,
      recentPages: [
        Array.from({ length: 20 }, (_, index) => index + 1),
        [20, 85, ...Array.from({ length: 18 }, (_, index) => index + 21)],
      ],
    });
    if (source === "JM")
      await page.getByLabel("最近更新来源").selectOption(source);
    const grid = page.getByTestId("recent-grid");
    await expect(grid).toHaveAttribute("data-total-items", "100");
    await page.getByRole("button", { name: "读取下一页", exact: true }).click();
    await expect
      .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
      .toBe(true);
    await grid.evaluate((element) => {
      const main = element.closest("main")!;
      const stride =
        element.getBoundingClientRect().height / Math.ceil(100 / 7);
      const offset =
        main.scrollTop +
        element.getBoundingClientRect().top -
        main.getBoundingClientRect().top;
      main.scrollTop = offset + 12 * stride;
    });
    await expect(recentCard(page, source, 85)).toBeInViewport();
    const anchor = await captureRecentAnchor(page);
    const before = await recentCard(page, source, 85).boundingBox();
    await page.evaluate(() => window.recentTest.release!());
    await expect(page.getByTestId("recent-counts")).toContainText(
      "已读取 39 部",
    );
    await expect(grid).toHaveAttribute("data-total-items", "100");
    await expectRecentAnchor(page, anchor);
    await expect(recentCard(page, source, 85)).toBeInViewport();
    await expect
      .poll(async () =>
        Math.abs(
          (await recentCard(page, source, 85).boundingBox())!.y - before!.y,
        ),
      )
      .toBeLessThanOrEqual(4);
    await expectStationaryRecentGrid(page);
    await page.getByTestId("nav-settings").click();
    await page.getByTestId("nav-recent").click();
    await expectRecentAnchor(page, anchor);
    expect(await detailCalls(page)).toEqual([]);
    expect(
      (await recentCalls(page))
        .filter((args) => args.source === source)
        .map((args) => args.page),
    ).toEqual([1, 2]);
    await page
      .getByRole("button", { name: "刷新最近更新", exact: true })
      .click();
    await expect(page.getByTestId("recent-counts")).toContainText(
      "已读取 20 部",
    );
    // Explicit refresh can now move the saved work into its known live position.
    await expect(grid.locator("article").nth(20)).toHaveAttribute(
      "data-testid",
      `recent-work-${source}:${source === "JM" ? "85" : "85".padStart(24, "0")}`,
    );
    await expect(grid).toHaveAttribute("data-total-items", "100");
    await mkdir("visual-evidence", { recursive: true });
    await page.screenshot({
      path: `visual-evidence/recent-merge-${source}.png`,
    });
  });
}

test("deep recent history with unequal row heights stays still after scrolling and never overlaps", async ({
  page,
}) => {
  await install(page, { retainedCount: 1000 });
  await expect(page.getByTestId("recent-grid")).toHaveAttribute(
    "data-total-items",
    "1000",
  );
  await varyRecentRowMetadata(page);
  const positions = await page.getByTestId("recent-grid").evaluate((grid) => {
    const main = grid.closest("main")!;
    const rows = grid.querySelectorAll(".source-virtual-row");
    const gap = parseFloat(getComputedStyle(grid).rowGap);
    const short = rows[0].getBoundingClientRect().height + gap;
    const tall = rows[1].getBoundingClientRect().height + gap;
    const offset =
      main.scrollTop +
      grid.getBoundingClientRect().top -
      main.getBoundingClientRect().top;
    // Stop on boundaries where using a different first row as the global
    // height would alternate the short/tall samples. No wheel input follows.
    return Array.from({ length: 60 }, (_, index) => index * 2 + 11)
      .filter(
        (row) => (Math.floor(((row + 2) * short + 2) / tall) - 2) % 2 === 0,
      )
      .slice(0, 3)
      .map((row) => offset + (row + 2) * short + 2);
  });
  expect(positions).toHaveLength(3);
  for (const top of [...positions, positions[0]]) {
    await page.getByRole("main").evaluate((main, value) => {
      main.scrollTop = value;
    }, top);
    await expectStationaryRecentGrid(page);
  }
  expect(await recentCalls(page)).toHaveLength(1);
  expect(await detailCalls(page)).toEqual([]);
});

test("a taller recent row preserves the visible anchor and the grid can settle after resize and return", async ({
  page,
}) => {
  await install(page, { retainedCount: 1000 });
  await expect(page.getByTestId("recent-grid")).toHaveAttribute(
    "data-total-items",
    "1000",
  );
  await varyRecentRowMetadata(page);
  await expectStationaryRecentGrid(page);
  await page.getByTestId("recent-grid").evaluate((grid) => {
    const main = grid.closest("main")!;
    const rows = grid.querySelectorAll<HTMLElement>(".source-virtual-row");
    const stride =
      parseFloat(rows[1].style.top) - parseFloat(rows[0].style.top);
    const offset =
      main.scrollTop +
      grid.getBoundingClientRect().top -
      main.getBoundingClientRect().top;
    // Deliberately leave a clipped row above the fully visible anchor. A fixed
    // scrollTop can happen to align exactly with a row on another CI/font run.
    main.scrollTop = offset + 29.8 * stride;
  });
  await expectStationaryRecentGrid(page);
  const anchor = await captureRecentAnchor(page);
  expect(anchor.key).toBe("recent-work-Pica:000000000000000000000211");
  await page.getByTestId("recent-grid").evaluate((grid) => {
    const row = grid.querySelector<HTMLElement>(".source-virtual-row")!;
    row.style.minHeight = row.getBoundingClientRect().height + 48 + "px";
  });
  await expectRecentAnchor(page, anchor);
  await expectStationaryRecentGrid(page);
  await page.setViewportSize({ width: 1180, height: 920 });
  await expect(page.getByTestId(anchor.key)).toBeInViewport();
  await expectStationaryRecentGrid(page);
  const resized = await captureRecentAnchor(page);
  await page.getByTestId("nav-library").click();
  await page.getByTestId("nav-recent").click();
  await expectRecentAnchor(page, resized);
  await expectStationaryRecentGrid(page);
  expect(await recentCalls(page)).toHaveLength(1);
});

test("both recent feeds preserve source order, language and unknown dates and directly enqueue without leaving the feed", async ({
  page,
}) => {
  await install(page);
  await expect(page.getByLabel("最近更新来源")).toHaveValue("Pica");
  await expect(page.getByTestId("recent-counts")).toContainText("20");
  await expect(
    recentCard(page, "Pica", 1).getByTestId("source-language-badge"),
  ).toHaveText("已汉化");
  await expect(
    recentCard(page, "Pica", 2).getByTestId("source-language-badge"),
  ).toHaveText("生肉");
  await expect(
    recentCard(page, "Pica", 3).getByTestId("source-language-badge"),
  ).toHaveText("未知");
  await expect(recentCard(page, "Pica", 3)).toContainText("更新时间未知");
  expect(
    (await recentCalls(page)).map((args) => [args.source, args.page]),
  ).toEqual([["Pica", 1]]);
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(page.getByTestId("recent-counts")).toContainText("已入库 1");
  await expect(page.getByTestId("recent-progress")).not.toContainText(
    "分页已读完",
  );
  await page
    .getByLabel("最近更新入库筛选")
    .getByRole("button", { name: "未入库 19", exact: true })
    .click();
  await expect(recentCard(page, "JM", 1)).toHaveCount(0);
  await expect(recentCard(page, "JM", 2)).toBeVisible();
  await recentCard(page, "JM", 2)
    .getByRole("button", { name: /打开《/ })
    .click({ button: "right" });
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("menuitem", { name: "作品详细", exact: true })
    .click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(recentCard(page, "JM", 2)).toBeVisible();
  await expect(recentCard(page, "JM", 1)).toHaveCount(0);
  await expect(page.getByTestId("recent-counts")).toContainText("20");
  await page.getByTestId("recent-counts").scrollIntoViewIfNeeded();
  await expect(recentCard(page, "JM", 2)).toBeInViewport();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/recent-updates-jm.png" });
  expect(
    await page.evaluate(() =>
      window.recentTest.calls.filter((c) => /download_prepare/.test(c.command)),
    ),
  ).toEqual([]);
  await recentCard(page, "JM", 2)
    .getByRole("button", { name: "下载到漫画库", exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => window.recentTest.queue.tasks.length))
    .toBe(1);
  await expect(page.getByTestId("download-confirmation")).toHaveCount(0);
  await expect(page.getByTestId("recent-panel")).toBeVisible();
  await expect(page.getByTestId("native-downloads")).toBeHidden();
  for (const args of await recentCalls(page)) {
    expect(args.query).toBe("");
    expect(args.folderId).toBeNull();
    expect(args).not.toHaveProperty("reverse");
  }
});

test("an entire page hidden by BL, AI categories and confirmed author-update membership still continues, while unrelated keyword membership stays visible", async ({
  page,
}) => {
  await install(page, {
    blIds: Array.from({ length: 5 }, (_, i) => i * 4 + 1),
    aiIds: Array.from({ length: 5 }, (_, i) => i * 4 + 3),
    catalogIds: Array.from({ length: 10 }, (_, i) => i * 2 + 2),
    catalogOtherIds: [21],
    holdPage: 2,
  });
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await expect(page.getByTestId("recent-grid")).toHaveAttribute(
    "data-total-items",
    "0",
  );
  await expect(page.getByTestId("recent-progress")).not.toContainText(
    "分页已读完",
  );
  // No wheel or button is needed when the whole source page was filtered out.
  // Hold the next response to verify both the empty intermediate view and the
  // automatic request before its unrelated, eligible works become visible.
  await expect
    .poll(async () => (await recentCalls(page)).map((value) => value.page))
    .toEqual([1, 2]);
  await expect
    .poll(() => page.evaluate(() => typeof window.recentTest.release))
    .toBe("function");
  await page.evaluate(() => window.recentTest.release?.());
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await expect(page.getByTestId("recent-grid")).toHaveAttribute(
    "data-total-items",
    "19",
  );
  await expect(recentCard(page, "Pica", 21)).toBeVisible();
  expect((await recentCalls(page)).map((value) => value.page)).toEqual([1, 2]);
  expect(
    await page.evaluate(() =>
      window.recentTest.calls.filter(
        ({ command, args }) =>
          command === "source_query" && args.kind !== "recent",
      ),
    ),
  ).toEqual([]);
});

test("feed anchors and loaded pages survive section and source switches, while a fresh app view starts at the top", async ({
  page,
}) => {
  await install(page);
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.getByRole("main").evaluate((main) => {
    main.scrollTop = 1500;
  });
  const anchor = await captureRecentAnchor(page);
  const before = await recentCalls(page);
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-recent").click();
  await expectRecentAnchor(page, anchor);
  expect(await recentCalls(page)).toEqual(before);
  await page
    .getByLabel("最近更新来源")
    .evaluate((select: HTMLSelectElement) => {
      select.value = "JM";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await page.getByLabel("最近更新来源").selectOption("Pica");
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await expectRecentAnchor(page, anchor);
  expect(
    (await recentCalls(page))
      .filter((value) => value.source === "Pica")
      .map((value) => value.page),
  ).toEqual([1, 2]);
  await page.reload();
  await page.getByTestId("nav-recent").click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await expect
    .poll(() => page.getByRole("main").evaluate((main) => main.scrollTop))
    .toBe(0);
});

test("recent source switches restore each feed's query and inventory filter without refetching its catalog", async ({
  page,
}) => {
  await install(page);
  const query = page.getByLabel("筛选已读取最近更新");
  const filters = page.getByRole("group", { name: "最近更新入库筛选" });
  await query.fill("更新 2");
  await filters.getByRole("button", { name: /^未入库 / }).click();
  await expect(page.getByTestId("recent-counts")).toContainText(
    "当前显示 2 部",
  );

  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(query).toHaveValue("");
  await expect(filters.getByRole("button", { name: /^全部 / })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await query.fill("更新 1");
  await filters.getByRole("button", { name: /^已入库 / }).click();
  await expect(page.getByTestId("recent-counts")).toContainText(
    "当前显示 1 部",
  );

  await page.getByLabel("最近更新来源").selectOption("Pica");
  await expect(query).toHaveValue("更新 2");
  await expect(
    filters.getByRole("button", { name: /^未入库 / }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("recent-counts")).toContainText(
    "当前显示 2 部",
  );
  await expect(recentCard(page, "Pica", 2)).toBeVisible();
  await expect(recentCard(page, "Pica", 20)).toBeVisible();

  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(query).toHaveValue("更新 1");
  await expect(
    filters.getByRole("button", { name: /^已入库 / }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(recentCard(page, "JM", 1)).toBeVisible();
  expect(
    (await recentCalls(page)).map(({ source, page }) => [source, page]),
  ).toEqual([
    ["Pica", 1],
    ["JM", 1],
  ]);
});

test("a downward browse reads only the next page, keeps good cards on failure, and deduplicates the retried live page", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await page.evaluate(() => {
    window.recentTest.failPage = 2;
  });
  const main = page.getByRole("main");
  await main.hover();
  await page.mouse.wheel(
    0,
    await main.evaluate((element) => element.scrollHeight),
  );
  await expect(
    page
      .getByTestId("recent-panel")
      .getByRole("button", { name: "重试读取", exact: true }),
  ).toBeVisible();
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await expect(recentCard(page, "Pica", 20)).toBeVisible();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  await main.hover();
  await page.mouse.wheel(0, 800);
  await page.clock.runFor(500);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await page.evaluate(() => {
    window.recentTest.failPage = null;
  });
  await page
    .getByTestId("recent-panel")
    .getByRole("button", { name: "重试读取", exact: true })
    .click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(3000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2, 2]);
  await expect(recentCard(page, "Pica", 20)).toHaveCount(1);
  const query = page.getByLabel("筛选已读取最近更新");
  await query.focus();
  await page.clock.runFor(150);
  const unfilteredAnchor = await captureRecentAnchor(page);
  await query.fill("not-in-this-synthetic-catalog");
  await expect(page.getByTestId("recent-grid").locator("article")).toHaveCount(
    0,
  );
  await page.getByTestId("recent-progress").hover();
  await page.mouse.wheel(0, 10000);
  await page.clock.runFor(2000);
  await expect
    .poll(async () => (await recentCalls(page)).map((args) => args.page))
    .toEqual([1, 2, 2, 3]);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 40 部");
  await query.fill("");
  await page.clock.runFor(150);
  await expectRecentAnchor(page, unfilteredAnchor);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/recent-updates-pica-partial.png",
  });
});

test("a refresh keeps its previous list on error and replaces the current browsing window instead of combining old and new order", async ({
  page,
}) => {
  await install(page);
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.evaluate(() => {
    window.recentTest.version = 1;
    window.recentTest.failPage = 1;
  });
  await page.getByRole("button", { name: "刷新最近更新", exact: true }).click();
  await expect(
    page.getByTestId("recent-panel").getByRole("alert"),
  ).toBeVisible();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await expect(recentCard(page, "Pica", 101)).toHaveCount(0);
  await page.evaluate(() => {
    window.recentTest.failPage = null;
  });
  await page
    .getByTestId("recent-panel")
    .getByRole("button", { name: "重试读取", exact: true })
    .click();
  await expect(recentCard(page, "Pica", 101)).toBeVisible();
  await expect(recentCard(page, "Pica", 1)).toHaveCount(0);
  await expect(recentCard(page, "Pica", 39)).toHaveCount(0);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([
    1, 2, 1, 1,
  ]);
});

test("late pages cannot enter another source or replacement session, and switching rankings does not scan the recent feed in the background", async ({
  page,
}) => {
  await install(page);
  await expect(recentCard(page, "Pica", 1)).toBeVisible();
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
  });
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  await page.getByLabel("最近更新来源").selectOption("JM");
  await expect(recentCard(page, "JM", 1)).toBeVisible();
  await page.evaluate(() => window.recentTest.release!());
  await expect(recentCard(page, "Pica", 21)).toHaveCount(0);
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
    delete window.recentTest.release;
  });
  await page.getByRole("button", { name: "读取下一页", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.evaluate(() => {
    window.recentTest.accounts[0].sessionId = "replacement-JM";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await page.evaluate(() => window.recentTest.release!());
  await openUnifiedSearch(page, "作品关键词");
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "最近更新", exact: true })
    .click();
  await expect(recentCard(page, "JM", 1)).toContainText("replacement-JM");
  await expect(recentCard(page, "JM", 21)).toHaveCount(0);
  const beforeRanks = await recentCalls(page);
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "JM 每周必看", exact: true })
    .click();
  await expect(page.getByTestId("rank-work-JM:901")).toBeVisible();
  expect(await recentCalls(page)).toEqual(beforeRanks);
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "最近更新", exact: true })
    .click();
  await expect(recentCard(page, "JM", 1)).toContainText("replacement-JM");
  expect(await recentCalls(page)).toEqual(beforeRanks);
});

test("downward input at an already reached edge loads one page, ignores other directions and stays single flight without chaining", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  const main = page.getByRole("main");
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(100);
  await main.hover();
  const initialEdge = await main.evaluate((element) => element.scrollTop);
  await main.evaluate((element) => {
    window.recentTest.directionInputs = [];
    element.addEventListener(
      "wheel",
      (event) => {
        const wheel = event as WheelEvent;
        window.recentTest.directionInputs!.push({
          x: wheel.deltaX,
          y: wheel.deltaY,
          shift: wheel.shiftKey,
        });
      },
      { passive: true },
    );
  });
  await page.mouse.wheel(300, 0);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, 200);
  await expect
    .poll(() => page.evaluate(() => window.recentTest.directionInputs))
    .toEqual([
      { x: 300, y: 0, shift: false },
      { x: 0, y: 200, shift: true },
    ]);
  await page.keyboard.up("Shift");
  await page.mouse.wheel(0, -80);
  // Native wheel delivery and compositor scrolling are not advanced by the
  // fake JavaScript clock. Let the upward gesture finish before resetting the
  // edge; otherwise its delayed -80 movement contaminates the next assertion.
  await expect
    .poll(() => main.evaluate((element) => element.scrollTop))
    .toBe(initialEdge - 80);
  await page.clock.runFor(100);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(100);
  const edge = await main.evaluate((element) => element.scrollTop);
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
  });
  await page.mouse.wheel(0, 100);
  await page.clock.runFor(100);
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  expect(
    Math.abs((await main.evaluate((element) => element.scrollTop)) - edge),
  ).toBeLessThanOrEqual(2);
  await main.evaluate((element) => {
    window.recentTest.observedWheelDelta = 0;
    element.addEventListener(
      "wheel",
      (event) => {
        window.recentTest.observedWheelDelta! += (event as WheelEvent).deltaY;
      },
      { passive: true },
    );
  });
  for (let i = 0; i < 3; i++) await page.mouse.wheel(0, 200);
  // mouse.wheel does not wait for browser delivery; advancing the JavaScript
  // clock also does not drain Chromium's native input queue. Keep page 2 held
  // until all three wheel deltas were actually delivered during the request.
  await expect
    .poll(() => page.evaluate(() => window.recentTest.observedWheelDelta))
    .toBe(600);
  await page.clock.runFor(300);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  const oldVisibleCard = await captureRecentAnchor(page);
  await page.evaluate(() => window.recentTest.release!());
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(3000);
  await expectRecentAnchor(page, oldVisibleCard);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(100);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await page.mouse.wheel(0, 100);
  await page.clock.runFor(100);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 40 部");
  await expect(page.getByTestId("recent-progress")).toContainText("分页已读完");
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.mouse.wheel(0, 500);
  await page.clock.runFor(2000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2, 3]);
});

test("a continuous drag may take longer than the old intent timeout and still loads only its next near-edge page", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  const main = page.getByRole("main");
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
  });
  const bounds = await main.boundingBox();
  expect(bounds).not.toBeNull();
  const pointerX = bounds!.x + bounds!.width - 2;
  const pointerStartY = bounds!.y + 100;
  const pointerEndY = bounds!.y + bounds!.height - 100;
  // Browser pointer events model a held scrollbar/thumb without relying on
  // operating-system scrollbar width or theme-dependent native coordinates.
  await main.dispatchEvent("pointerdown", {
    pointerId: 7,
    pointerType: "mouse",
    button: 0,
    buttons: 1,
    clientX: pointerX,
    clientY: pointerStartY,
    bubbles: true,
  });
  await main.evaluate((element) => {
    element.scrollTop = 80;
  });
  await page.clock.runFor(2200);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  // The held pointer's later scrollbar displacement must still count as input,
  // without another pointerdown or fresh wheel event renewing the old timeout.
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(100);
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  await main.dispatchEvent("pointerup", {
    pointerId: 7,
    pointerType: "mouse",
    button: 0,
    buttons: 0,
    clientX: pointerX,
    clientY: pointerEndY,
    bubbles: true,
  });
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await page.evaluate(() => window.recentTest.release!());
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(3000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
});

test("a delayed continuation preserves the user's newer reading position rather than returning to the request position", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  const main = page.getByRole("main");
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(150);
  await page.evaluate(() => {
    window.recentTest.holdPage = 2;
  });
  await main.hover();
  await page.mouse.wheel(0, 100);
  await expect
    .poll(() => page.evaluate(() => Boolean(window.recentTest.release)))
    .toBe(true);
  const requestAnchor = await captureRecentAnchor(page);
  await page.mouse.wheel(0, -500);
  // Wait for actual native-wheel displacement, not just a simulated clock tick.
  await expect
    .poll(() =>
      page
        .getByTestId(requestAnchor.key)
        .evaluate(
          (element) =>
            element.getBoundingClientRect().top -
            element.closest("main")!.getBoundingClientRect().top,
        ),
    )
    .toBeGreaterThan(requestAnchor.y + 200);
  await page.clock.runFor(150);
  const newerAnchor = await captureRecentAnchor(page);
  expect(newerAnchor.key).not.toBe(requestAnchor.key);
  await page.evaluate(() => window.recentTest.release!());
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(3000);
  await expectRecentAnchor(page, newerAnchor);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
});

test("programmatic changes and hidden feeds never supply browse intent, while an explicitly scrolled filtered feed can continue", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  const main = page.getByRole("main");
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.clock.runFor(2000);
  await page.setViewportSize({ width: 1672, height: 950 });
  await page.clock.runFor(100);
  await page.setViewportSize({ width: 1672, height: 1020 });
  await page.clock.runFor(100);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  await recentCard(page, "Pica", 20)
    .getByRole("button", { name: /打开《/ })
    .click({ button: "right" });
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("menuitem", { name: "作品详细", exact: true })
    .click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(recentCard(page, "Pica", 20)).toBeVisible();
  await page.clock.runFor(2000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  await page
    .getByLabel("最近更新入库筛选")
    .getByRole("button", { name: "已入库 0", exact: true })
    .click();
  // A deliberate downward input can advance even when the current ownership filter hides every card.
  await page.getByTestId("recent-progress").hover();
  await page.mouse.wheel(0, 10000);
  await page.clock.runFor(500);
  await expect
    .poll(async () => (await recentCalls(page)).map((args) => args.page))
    .toEqual([1, 2]);
  await page.clock.runFor(1000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await page
    .getByLabel("最近更新入库筛选")
    .getByRole("button", { name: "全部 39", exact: true })
    .click();
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "JM 每周必看", exact: true })
    .click();
  await expect(page.getByTestId("rank-work-JM:901")).toBeVisible();
  await main.hover();
  await page.mouse.wheel(0, 10000);
  await page.clock.runFor(1000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  await page
    .locator(".source-tabs")
    .getByRole("button", { name: "最近更新", exact: true })
    .click();
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 39 部");
  await page.clock.runFor(1000);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
});

test("context-menu scrolling and focused-input keys cannot continue the background recent feed", async ({
  page,
}) => {
  await page.clock.install();
  await install(page);
  await expect(page.getByTestId("recent-counts")).toContainText("已读取 20 部");
  const main = page.locator("main");
  const search = page.getByLabel("筛选已读取最近更新");
  await search.focus();
  await main.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await search.press("PageDown");
  await search.press("End");
  await search.dispatchEvent("wheel", { deltaY: 500, bubbles: true });
  await page.clock.runFor(1500);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  await recentCard(page, "Pica", 20)
    .getByRole("button", { name: /打开《/ })
    .click({ button: "right" });
  const dialog = page.getByTestId("reader-cover-actions");
  await expect(dialog).toBeVisible();
  // A menu is anchored outside the scrolling feed; its input must not grant
  // another pagination credit to the background list.
  await dialog.hover();
  await page.mouse.wheel(0, 600);
  await dialog.getByRole("menuitem").first().focus();
  await page.keyboard.press("PageDown");
  await page.keyboard.press("End");
  await page.clock.runFor(1500);
  await expect(dialog).toBeVisible();
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await page.clock.runFor(1500);
  expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
});

nativeScrollbarTest(
  "dragging the actual Chromium scrollbar after a long hold continues one page without synthetic scroll events",
  async ({ page }, testInfo) => {
    await page.clock.install();
    await install(page);
    await expect(page.getByTestId("recent-counts")).toContainText(
      "已读取 20 部",
    );
    const main = page.getByRole("main");
    await main.evaluate((element) => {
      element.scrollTop = 0;
    });
    await page.clock.runFor(100);
    const scrollbar = await main.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const trackHeight = element.clientHeight;
      const thumbHeight = Math.max(
        24,
        (trackHeight * trackHeight) / element.scrollHeight,
      );
      return {
        width: element.offsetWidth - element.clientWidth,
        x:
          rect.right -
          Math.max(3, (element.offsetWidth - element.clientWidth) / 2),
        startY: rect.top + element.clientTop + thumbHeight / 2,
        endY: rect.top + element.clientTop + trackHeight - thumbHeight / 2 - 2,
      };
    });
    expect(scrollbar.width).toBeGreaterThan(0);
    expect(scrollbar.endY).toBeGreaterThan(scrollbar.startY);
    await page.evaluate(() => {
      window.recentTest.holdPage = 2;
    });
    const gestureTrace = await main.evaluateHandle((element) => {
      const events: Record<string, string | number | boolean>[] = [];
      for (const type of [
        "pointerdown",
        "mousedown",
        "pointermove",
        "scroll",
      ]) {
        window.addEventListener(
          type,
          (event) => {
            if (events.length >= 40) return;
            const target = event.target;
            events.push({
              type: event.type,
              target: target instanceof Element ? target.tagName : "window",
              onMain: target === element,
              trusted: event.isTrusted,
              x: event instanceof MouseEvent ? event.clientX : -1,
              y: event instanceof MouseEvent ? event.clientY : -1,
              buttons: event instanceof MouseEvent ? event.buttons : 0,
              scrollTop: element.scrollTop,
            });
          },
          { capture: true, passive: true },
        );
      }
      return events;
    });
    await page.mouse.move(scrollbar.x, scrollbar.startY);
    await page.mouse.down();
    try {
      await page.clock.runFor(2200);
      expect((await recentCalls(page)).map((args) => args.page)).toEqual([1]);
      await page.mouse.move(scrollbar.x, scrollbar.endY, { steps: 16 });
      await page.clock.runFor(300);
      await expect
        .poll(() => main.evaluate((element) => element.scrollTop))
        .toBeGreaterThan(0);
      await expect
        .poll(() => page.evaluate(() => Boolean(window.recentTest.release)), {
          message:
            "A native scrollbar drag near the end should request its next page",
        })
        .toBe(true);
      const oldVisibleCard = await captureRecentAnchor(page);
      // The real scrollbar is still held stationary while the new page arrives.
      // Its changed thumb size must not move the user's previously visible card.
      await page.evaluate(() => window.recentTest.release!());
      await expect(page.getByTestId("recent-counts")).toContainText(
        "已读取 39 部",
      );
      await page.clock.runFor(3000);
      await expectRecentAnchor(page, oldVisibleCard);
      expect((await recentCalls(page)).map((args) => args.page)).toEqual([
        1, 2,
      ]);
    } catch (error) {
      await testInfo.attach("native-scrollbar-input.json", {
        body: JSON.stringify(
          { scrollbar, events: await gestureTrace.jsonValue() },
          null,
          2,
        ),
        contentType: "application/json",
      });
      throw error;
    } finally {
      await page.mouse.up();
      await gestureTrace.dispose();
    }
    expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
    await expect(page.getByTestId("recent-counts")).toContainText(
      "已读取 39 部",
    );
    await page.clock.runFor(3000);
    expect((await recentCalls(page)).map((args) => args.page)).toEqual([1, 2]);
  },
);
