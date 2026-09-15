import type { Page } from "@playwright/test";
import type { DiscoverySnapshot } from "../src/completion-types.ts";
import type { LibrarySnapshot } from "../src/library-types.ts";
import type {
  DownloadBatchPlan,
  DownloadInventorySnapshot,
  DownloadPlan,
  DownloadSnapshot,
} from "../src/download-types.ts";
import type { AccountSummary, SourceWork } from "../src/source-types.ts";
import { initialPreferences } from "../src/preferences.ts";

declare global {
  interface Window {
    workflowTest: {
      calls: { command: string; args: Record<string, unknown> }[];
      accounts: AccountSummary[];
      library: LibrarySnapshot;
      inventory: DownloadInventorySnapshot;
      queue: DownloadSnapshot;
      discovery: DiscoverySnapshot;
      failLibraryRead: boolean;
      searchFault: "none" | "fail-last" | "hold-first" | "hold-last";
      releasePage?: () => void;
      finishCheck(): void;
      finishDownloads(): void;
    };
  }
}

// A shared synthetic service model for cross-page integration. Completion writes
// distinct ZIP entries and same-source receipts, independently of rendering.
// No filesystem, credentials, source HTTP requests or real media are involved.
export async function installWorkflow(page: Page) {
  await page.addInitScript(
    ({ preferences }) => {
      const clone = <T>(value: T): T => structuredClone(value);
      const rootId = "a".repeat(64);
      const id = (n: number) => n.toString(16).padStart(64, "0");
      const picaId = (n: number) => n.toString().padStart(24, "0");
      const works: SourceWork[] = [
        ["JM", "101", "已保存作品"],
        ["JM", "102", "本次选择的 JM 作品"],
        ["JM", "103", "未选择的旧遗漏"],
        ["Pica", picaId(201), "本次选择的哔咔作品"],
        ["Pica", picaId(202), "已保存作品"],
        ["JM", "104", "新作者的 JM 作品"],
        ["Pica", picaId(203), "新作者的哔咔作品"],
      ].map(([source, workId, title]) => ({
        source: source as SourceWork["source"],
        workId,
        title,
        authors: ["合成关注作者", "合成新作者"],
        description: null,
        tags: [],
        favorite: true,
        chapterCount: 1,
        pageCount: 3,
        coverAvailable: false,
      }));
      const updates = works.slice(0, 5);
      const search = [works[1], works[5], works[3], works[6]];
      const accounts: AccountSummary[] = (["JM", "Pica"] as const).map(
        (source) => ({
          source,
          sessionId: "synthetic-" + source,
          accountId: "account-" + source,
          displayName: "合成账号",
          state: "connected",
          remembered: false,
          errorCode: null,
        }),
      );
      const entryFor = (work: SourceWork) => {
        const fileName = "[合成作者] " + work.title + ".zip";
        return {
          id: id(100 + works.indexOf(work)),
          relativePath: fileName,
          fileName,
          title: work.title,
          authors: work.authors,
          format: "zip" as const,
          description: null,
          tags: [],
          bytes: 300,
          modifiedAt: 1800000000000,
          addedAt: 1800000000000,
          pageCount: 3,
          coverAvailable: false,
          state: "indexed" as const,
          errorCode: null,
          sourceRef: { source: work.source, workId: work.workId },
          identityEvidence: "metadata" as const,
        };
      };
      const library: LibrarySnapshot = {
        rootId,
        rootPath: "C:\\Synthetic",
        revision: 1,
        generation: 1,
        phase: "complete",
        freshness: "live",
        items: [entryFor(works[0])],
        visited: 1,
        skipped: 0,
        updatedAt: 1800000000000,
        errorCode: null,
      };
      const inventory: DownloadInventorySnapshot = {
        rootId,
        revision: 1,
        libraryRevision: 1,
        items: [
          {
            source: "JM",
            workId: "101",
            libraryEntryId: entryFor(works[0]).id,
            localFiles: "present",
          },
        ],
      };
      const record = (work: SourceWork) => ({
        work,
        matchedAuthors: ["合成关注作者"],
        authorVerified: true,
        observedAt: 1800000000000,
        scanId: "synthetic-check",
      });
      const discovery: DiscoverySnapshot = {
        scopes: accounts.map((a) => ({
          source: a.source,
          sessionId: a.sessionId!,
        })),
        revision: 1,
        run: null,
        authors: accounts.map((a) => ({
          source: a.source,
          author: "合成关注作者",
          state: "idle",
          lastAttemptAt: null,
          lastCompleteAt: null,
          observedCount: 0,
          pagesRead: 0,
          errorCode: null,
        })),
        records: [record(works[2])],
      };
      const saved = JSON.parse(
        sessionStorage.getItem("synthetic.workflow") ?? "null",
      );
      const hooks = (window.workflowTest = {
        calls: [],
        accounts,
        library,
        inventory,
        discovery,
        queue: { revision: 0, tasks: [] },
        ...(saved ?? {}),
        failLibraryRead: false,
        searchFault: "none",
        finishCheck() {
          hooks.discovery.records = updates.map(record);
          hooks.discovery.authors = hooks.discovery.authors.map((range) => ({
            ...range,
            state: "complete",
            pagesRead: 1,
            observedCount: updates.filter((w) => w.source === range.source)
              .length,
            lastAttemptAt: 1800000001000,
            lastCompleteAt: 1800000001000,
          }));
          hooks.discovery.run = {
            ...hooks.discovery.run!,
            phase: "complete",
            completedScopes: 2,
          };
          hooks.discovery.revision++;
          save();
        },
        finishDownloads() {
          for (const task of hooks.queue.tasks) {
            if (task.phase === "downloaded") continue;
            const work = works.find(
              (w) => w.source === task.source && w.workId === task.workId,
            )!;
            const entry = entryFor(work);
            hooks.library.items.push(entry);
            hooks.inventory.items.push({
              source: work.source,
              workId: work.workId,
              libraryEntryId: entry.id,
              localFiles: "present",
            });
            Object.assign(task, {
              phase: "downloaded",
              revision: task.revision + 1,
              filesDone: 3,
              filesTotal: 3,
              bytesDone: 300,
              libraryEntryId: entry.id,
              localFiles: "present",
              allowedActions: [],
              updatedAt: 1800000002000,
            });
          }
          hooks.queue.revision++;
          hooks.library.revision++;
          hooks.library.visited = hooks.library.items.length;
          hooks.inventory.revision++;
          hooks.inventory.libraryRevision = hooks.library.revision;
          save();
        },
      } as Window["workflowTest"]);
      function save() {
        sessionStorage.setItem(
          "synthetic.workflow",
          JSON.stringify({
            library: hooks.library,
            inventory: hooks.inventory,
            queue: hooks.queue,
            discovery: hooks.discovery,
          }),
        );
      }
      let planSequence = 1000;
      const plans = new Map<string, DownloadPlan>();
      const batches = new Map<string, DownloadBatchPlan>();
      function prepare(source: string, input: string) {
        const work = works.find(
          (w) => w.source === source && w.workId === input,
        );
        if (!work) throw { code: "DOWNLOAD_INVALID_INPUT" };
        const plan: DownloadPlan = {
          planId: id(++planSequence),
          revision: hooks.queue.revision,
          source: work.source,
          workId: work.workId,
          title: work.title,
          authors: work.authors,
          rootId,
          generation: hooks.library.generation,
          destinationDisplay: "C:\\Synthetic\\" + entryFor(work).fileName,
        };
        plans.set(plan.planId, plan);
        return plan;
      }
      function confirm(selected: DownloadPlan[]) {
        for (const plan of selected) {
          if (plan.revision !== hooks.queue.revision)
            throw { code: "DOWNLOAD_PLAN_STALE" };
        }
        hooks.queue.tasks.push(
          ...selected.map((plan) => ({
            id: plan.planId,
            revision: 1,
            source: plan.source,
            workId: plan.workId,
            title: plan.title,
            phase: "downloading" as const,
            filesDone: 0,
            filesTotal: null,
            bytesDone: 0,
            errorCode: null,
            allowedActions: ["pause" as const],
            libraryEntryId: null,
            localFiles: null,
            updatedAt: 1800000001000,
            destinationDisplay: plan.destinationDisplay,
          })),
        );
        hooks.queue.revision++;
        save();
        return clone(hooks.queue);
      }
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        value: {
          invoke: async (
            command: string,
            args: Record<string, unknown> = {},
          ) => {
            hooks.calls.push({ command, args: clone(args) });
            switch (command) {
              case "read_preferences":
                return { revision: 0, value: preferences };
              case "source_accounts":
                return clone(hooks.accounts);
              case "source_following":
                return {
                  source: args.source,
                  sessionId: args.sessionId,
                  revision: 1,
                  authors: ["合成关注作者"],
                  works: [],
                };
              case "source_rank_options":
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
              case "source_catalog":
                return {
                  source: args.source,
                  sessionId: args.sessionId,
                  snapshot: args.action === "write" ? args.snapshot : null,
                  completeSnapshot:
                    args.action === "write" ? args.snapshot : null,
                };
              case "source_query": {
                const candidates =
                  args.kind === "detail"
                    ? works.filter(
                        (w) =>
                          w.source === args.source && w.workId === args.query,
                      )
                    : (args.kind === "search" ? search : updates).filter(
                        (w) => w.source === args.source,
                      );
                const page = Number(args.page ?? 1);
                if (args.kind === "search") {
                  if (
                    (hooks.searchFault === "hold-first" &&
                      args.source === "JM" &&
                      page === 1) ||
                    (hooks.searchFault === "hold-last" &&
                      args.source === "Pica" &&
                      page === 2)
                  )
                    await new Promise<void>((resolve) => {
                      hooks.releasePage = resolve;
                    });
                  if (
                    hooks.searchFault === "fail-last" &&
                    args.source === "Pica" &&
                    page === 2
                  )
                    throw { code: "SOURCE_UNAVAILABLE" };
                }
                const items =
                  args.kind === "search"
                    ? candidates.slice(page - 1, page)
                    : candidates;
                const pages = args.kind === "search" ? candidates.length : 1;
                return clone({
                  source: args.source,
                  sessionId: args.sessionId,
                  items,
                  page,
                  pages,
                  total: candidates.length,
                  hasMore: page < pages,
                  folders: [],
                });
              }
              case "library_read":
              case "library_scan":
                if (hooks.failLibraryRead)
                  throw { code: "LIBRARY_UNAVAILABLE" };
                return clone(hooks.library);
              case "inventory_read":
                return clone(hooks.inventory);
              case "jm_download_read":
                return clone(hooks.queue);
              case "discovery_read":
                return clone(hooks.discovery);
              case "discovery_start": {
                hooks.discovery.run = {
                  id: "synthetic-check",
                  phase: "checking",
                  currentAuthor: "合成关注作者",
                  currentSource: "JM",
                  currentPage: 1,
                  requestsUsed: 1,
                  completedScopes: 0,
                  totalScopes: 2,
                  errorCode: null,
                };
                hooks.discovery.authors.forEach((r) => {
                  r.state = "checking";
                });
                hooks.discovery.revision++;
                return {
                  runId: "synthetic-check",
                  snapshot: clone(hooks.discovery),
                };
              }
              case "jm_download_prepare":
                return clone(
                  prepare(
                    (args.scope as { source: string }).source,
                    String(args.input),
                  ),
                );
              case "jm_download_confirm": {
                const plan = plans.get(String(args.planId));
                if (!plan || plan.revision !== args.expectedRevision)
                  throw { code: "DOWNLOAD_PLAN_STALE" };
                return confirm([plan]);
              }
              case "jm_download_batch_prepare": {
                const source = (args.scope as { source: string }).source;
                const batch: DownloadBatchPlan = {
                  batchId: id(++planSequence),
                  plans: (args.inputs as string[]).map((input) =>
                    prepare(source, input),
                  ),
                  issues: [],
                };
                batches.set(batch.batchId!, batch);
                return clone(batch);
              }
              case "jm_download_batch_cancel":
                batches.clear();
                return null;
              case "jm_download_selection_confirm":
                return confirm(
                  (args.batchIds as string[]).flatMap(
                    (id) => batches.get(id)!.plans,
                  ),
                );
              default:
                throw new Error(
                  "Unexpected synthetic workflow command: " + command,
                );
            }
          },
        },
      });
    },
    { preferences: initialPreferences() },
  );
  await page.goto("/");
}
