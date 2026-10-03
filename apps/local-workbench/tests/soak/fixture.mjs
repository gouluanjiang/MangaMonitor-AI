import { installWorkflow } from "../workflow-fixture.ts";

// Only the native/source boundary is simulated. Production React, schedulers,
// caches, reducers, history runtime and reader rendering remain unmodified.
export async function installSoakFixture(page) {
  await installWorkflow(page);
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const clone = (value) => structuredClone(value);
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const hooks = window.workflowTest;
    const canvas = document.createElement("canvas");
    canvas.width = 720;
    canvas.height = 1000;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#245e47";
    ctx.fillRect(0, 0, 720, 1000);
    ctx.fillStyle = "white";
    ctx.font = "40px sans-serif";
    ctx.fillText("SYNTHETIC SOAK IMAGE", 30, 100);
    const image = canvas.toDataURL("image/jpeg", 0.7);
    const id = (source, n) =>
      source === "JM" ? String(n) : n.toString(16).padStart(24, "0");
    const work = (source, n, author = "合成长测作者") => ({
      source,
      workId: id(source, n),
      title: `合成作品 ${author} ${n}`,
      authors: [author],
      tags: [],
      description: "Generated fixture; no real source content",
      favorite: null,
      chapterCount: 2,
      pageCount: 120,
      coverAvailable: true,
      sourceUpdatedAt: "2026-10-01T00:00:00Z",
    });
    const sessions = new Map();
    const positions = new Map();
    const failedRequests = new Set();
    const coverFailures = new Map();
    const s = (window.soak = {
      active: 0,
      maxActive: 0,
      activeCovers: 0,
      maxCovers: 0,
      activeQueries: 0,
      maxQueries: 0,
      requests: 0,
      failures: 0,
      injected: 0,
      retryAttempts: 0,
      counts: {},
      recentCalls: [],
      faults: [],
      readerSequence: 0,
      readerSessions: 0,
      size: 24,
      epoch: 0,
      specialFail: false,
      special: {
        scopes: hooks.accounts.map(({ source, sessionId }) => ({
          source,
          sessionId,
        })),
        authors: [
          {
            author: "合成关注作者",
            enabled: true,
            baselinesComplete: 2,
            errorCodes: [],
          },
        ],
        updates: [101, 102].map((n) => ({
          work: work("JM", n, "合成关注作者"),
          authors: ["合成关注作者"],
          discoveredAt: 100,
          readAt: null,
        })),
        run: {
          id: 1,
          phase: "complete",
          startedAt: 1,
          finishedAt: 100,
          newCount: 2,
          errorCode: null,
        },
      },
      resize(size) {
        const template = hooks.library.items[0];
        hooks.library.items = Array.from({ length: size }, (_, i) => ({
          ...template,
          id: (100 + i).toString(16).padStart(64, "0"),
          title: i === 0 ? "已保存作品" : `合成库 ${i}`,
          fileName: `synthetic-${i}.zip`,
          relativePath: `synthetic-${i}.zip`,
          sourceRef: { source: "JM", workId: String(101 + i) },
          coverAvailable: true,
        }));
        hooks.library.visited = size;
        hooks.library.revision++;
        hooks.inventory.libraryRevision = hooks.library.revision;
        s.size = size;
      },
    });
    s.resize(24);
    hooks.queue = {
      revision: 1,
      tasks: ["JM", "Pica"].map((source, i) => ({
        id: (700 + i).toString(16).padStart(64, "0"),
        revision: 1,
        source,
        workId: id(source, 900 + i),
        title: "Synthetic same-title download",
        phase: "error",
        filesDone: 0,
        filesTotal: 3,
        bytesDone: 0,
        errorCode: "SOURCE_TIMEOUT",
        allowedActions: ["retry", "abandon"],
        libraryEntryId: null,
        localFiles: null,
        updatedAt: 1800000000000,
        destinationDisplay: "C:\\Synthetic\\same-title.zip",
      })),
    };
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      const requestKey = JSON.stringify([
        command,
        args.source,
        args.kind,
        args.query,
        args.page,
        args.workId,
        args.readerId,
        args.pageIndex,
      ]);
      if (failedRequests.delete(requestKey)) s.retryAttempts++;
      s.active++;
      s.requests++;
      s.maxActive = Math.max(s.maxActive, s.active);
      s.counts[command] = (s.counts[command] ?? 0) + 1;
      const cover = command.endsWith("_cover");
      const query = command === "source_query";
      if (cover) s.maxCovers = Math.max(s.maxCovers, ++s.activeCovers);
      if (query) s.maxQueries = Math.max(s.maxQueries, ++s.activeQueries);
      s.recentCalls.push({ command, args: clone(args), at: performance.now() });
      if (s.recentCalls.length > 100) s.recentCalls.shift();
      // The short-test fixture's audit array must not manufacture a memory leak.
      if (hooks.calls.length > 64)
        hooks.calls.splice(0, hooks.calls.length - 64);
      try {
        const fault = s.faults.find(
          (f) =>
            f.remaining > 0 &&
            f.command === command &&
            (!f.source || args.source === f.source) &&
            (!f.workId || args.workId === f.workId) &&
            (!f.kind || args.kind === f.kind) &&
            (!f.page || args.page === f.page),
        );
        if (fault) {
          fault.remaining--;
          s.injected++;
          await sleep(fault.delay ?? 80);
          if (fault.bad) return { source: args.source, items: "invalid" };
          if (fault.code) throw { code: fault.code, retryAfterMs: 80 };
        }
        if (command === "source_cover" || command === "library_cover") {
          await sleep(35);
          if (command === "source_cover") {
            const number =
              args.source === "JM"
                ? Number(args.workId)
                : parseInt(args.workId, 16);
            const key = args.source + ":" + args.workId;
            const attempts = coverFailures.get(key) ?? 0;
            if (number % 17 === 7 && attempts < 2) {
              coverFailures.set(key, attempts + 1);
              if (coverFailures.size > 256)
                coverFailures.delete(coverFailures.keys().next().value);
              s.injected++;
              throw { code: "SOURCE_COVER_SERVER_ERROR", retryAfterMs: 80 };
            }
          }
          return { ...args, dataUrl: image };
        }
        if (command === "source_author_known_works") {
          const n = Number(String(args.author).match(/\d+/)?.[0] ?? 1);
          return {
            ...args,
            items: [work(args.source, 10000 + n * 100, args.author)],
            checkedAt: 1000,
            discoveryRevision: 1,
            historyComplete: true,
          };
        }
        if (command === "source_recent_history") {
          await sleep(args.source === "JM" ? 180 : 40);
          return {
            ...args,
            revision: 1,
            items: Array.from({ length: 120 }, (_, i) =>
              work(args.source, 1500 + i),
            ),
            coverage: {
              headIds: [id(args.source, 1000)],
              checkedAt: 1800000000000,
              pagesRead: 1,
              reachedEnd: false,
              joinedPrevious: true,
              initialWindow: false,
              errorCode: null,
            },
          };
        }
        if (query) {
          await sleep(args.source === "Pica" ? 180 : 55);
          let items;
          const p = Number(args.page ?? 1);
          let pages = 6;
          if (args.kind === "detail") {
            const n =
              args.source === "JM"
                ? Number(args.query)
                : parseInt(String(args.query), 16);
            items = [work(args.source, n)];
            pages = 1;
          } else if (args.kind === "author") {
            const n = Number(String(args.query).match(/\d+/)?.[0] ?? 1);
            pages = 3;
            items = Array.from({ length: 30 }, (_, i) =>
              work(args.source, 10000 + n * 100 + (p - 1) * 30 + i, args.query),
            );
          } else {
            // Adjacent overlap and a repeated final page exercise deduplication
            // and stop conditions. History is deliberately delivered out of order.
            const offset = (Math.min(p, 5) - 1) * 19;
            items = Array.from({ length: 20 }, (_, i) =>
              work(args.source, 1000 + offset + i + s.epoch),
            );
          }
          return {
            source: args.source,
            sessionId: args.sessionId,
            items,
            page: p,
            pages,
            total: args.kind === "author" ? 90 : pages * 20,
            hasMore: p < pages,
            folders: [],
            timing: {
              queueMs: 0,
              sourceOperationMs: args.source === "Pica" ? 180 : 55,
              localCommitMs: 0,
            },
          };
        }
        if (command.startsWith("reader_")) {
          if (command === "reader_open") {
            const readerId = `soak-reader-${++s.readerSequence}`;
            const key = JSON.stringify(args.request);
            sessions.set(readerId, key);
            s.readerSessions = sessions.size;
            // Native reader_open records only a successfully opened reference.
            // The browser fixture models that boundary side effect explicitly.
            const request = args.request;
            await original("history_record", {
              identity:
                request.kind === "library"
                  ? {
                      kind: "library",
                      rootId: request.rootId,
                      entryId: request.entryId,
                    }
                  : {
                      kind: "source",
                      source: request.source,
                      workId: request.workId,
                    },
              title: "合成长测阅读器",
            });
            return {
              readerId,
              title: "合成长测阅读器",
              origin:
                args.request.kind === "library"
                  ? "library"
                  : args.request.source,
              sourceRef: { source: "JM", workId: "101" },
              chapters: [
                { id: "one", title: "第一章", pageCount: null },
                { id: "two", title: "第二章", pageCount: null },
              ],
              position: positions.get(key) ?? null,
            };
          }
          if (
            command === "reader_cancel_open" ||
            command === "reader_fullscreen"
          )
            return null;
          if (command === "reader_close") {
            sessions.delete(args.readerId);
            s.readerSessions = sessions.size;
            return null;
          }
          if (!sessions.has(args.readerId)) throw { code: "READER_CLOSED" };
          if (command === "reader_chapter")
            return { ...args, pageCount: args.chapterId === "one" ? 120 : 7 };
          if (command === "reader_page") {
            await sleep(Number(args.pageIndex) % 3 === 0 ? 100 : 25);
            return { ...args, dataUrl: image, width: 720, height: 1000 };
          }
          if (command === "reader_save_position") {
            positions.set(sessions.get(args.readerId), clone(args.position));
            return null;
          }
          throw new Error(`Unsupported soak reader command: ${command}`);
        }
        if (command.startsWith("special_")) {
          if (s.specialFail) throw { code: "STORE_UNAVAILABLE" };
          if (command === "special_progress") return clone(s.special.run);
          if (command === "special_mark_read")
            s.special.updates.forEach((row) => {
              if (
                !args.identity ||
                (row.work.source === args.identity.source &&
                  row.work.workId === args.identity.workId)
              )
                row.readAt = Date.now();
            });
          if (command === "special_set")
            s.special.authors[0].enabled = Boolean(args.enabled);
          if (command === "special_check_start") s.special.run.id++;
          return clone(s.special);
        }
        if (command === "discovery_cancel") {
          if (hooks.discovery.run) hooks.discovery.run.phase = "cancelled";
          return clone(hooks.discovery);
        }
        if (command === "library_recycle" || command === "library_reveal")
          // Native dialog cancellation is simulated; actual Shell behavior is
          // covered only by the Windows CI adapter and native tests.
          return null;
        if (command === "jm_download_control") {
          const task = hooks.queue.tasks.find((t) => t.id === args.taskId);
          if (!task || task.revision !== args.expectedRevision)
            throw { code: "DOWNLOAD_TASK_STALE" };
          if (task.source !== args.scope.source)
            throw { code: "DOWNLOAD_SOURCE_MISMATCH" };
          task.revision++;
          task.phase =
            args.action === "pause"
              ? "paused"
              : args.action === "abandon"
                ? "abandoned"
                : "downloading";
          task.allowedActions =
            task.phase === "paused"
              ? ["resume", "abandon"]
              : task.phase === "abandoned"
                ? ["cleanup"]
                : ["pause"];
          task.errorCode = null;
          hooks.queue.revision++;
          return clone(hooks.queue);
        }
        return await original(command, args);
      } catch (error) {
        s.failures++;
        failedRequests.add(requestKey);
        if (failedRequests.size > 256)
          failedRequests.delete(failedRequests.values().next().value);
        throw error;
      } finally {
        s.active--;
        if (cover) s.activeCovers--;
        if (query) s.activeQueries--;
        s.faults = s.faults.filter((f) => f.remaining > 0);
      }
    };
  });
}
