import { expect, type BrowserContext, type Page } from "@playwright/test";
import type { ReaderPosition, ReaderRequest } from "../src/reader/types.ts";
import { installWorkflow } from "./workflow-fixture.ts";

type Call = { label: string; command: string; args: Record<string, unknown> };
type Session = { label: string; request: ReaderRequest; closed: boolean };
type EventTarget =
  | { kind: "Any" | "App" }
  | {
      kind: "AnyLabel" | "Window" | "Webview" | "WebviewWindow";
      label: string;
    };
type EventDelivery = { event: string; target: EventTarget };
type Bridge = {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  transformCallback?: (
    callback: (event: unknown) => void,
    once?: boolean,
  ) => number;
  unregisterCallback?: (id: number) => void;
  metadata?: unknown;
};

declare global {
  interface Window {
    readerWindowInvoke(
      command: string,
      args: Record<string, unknown>,
    ): Promise<unknown>;
    readerWindowHarness: {
      attach(bridge: unknown): void;
      emit(targetLabel: string, event: string, payload: unknown): void;
      events(): string[];
      eventDeliveries(): EventDelivery[];
      holdQueueRead: boolean;
      waitingQueueRead: boolean;
      releaseQueueRead(): void;
    };
  }
}

/** Only the IPC/event boundary is simulated. Each reader runs its real React
 * bootstrap, layout, cache and interaction code in a separate browser Page.
 * This does not prove native OS window lifetime, focus or always-on-top behavior.
 */
export async function installReaderWindows(
  context: BrowserContext,
  main: Page,
) {
  const calls: Call[] = [];
  const errors: string[] = [];
  const violations: string[] = [];
  const pages = new Map<string, Page>([["main", main]]);
  const labels = new Map<Page, string>([[main, "main"]]);
  const requests = new Map<string, ReaderRequest>();
  const sessions = new Map<string, Session>();
  const saved = new Map<string, ReaderPosition>();
  const pinned = new Map<string, boolean>();
  const closed = new Set<string>();
  const cancelled = new Set<string>();
  const failPin = new Set<string>();
  const holdSave = new Set<string>();
  const pendingSave = new Map<string, () => void>();
  let nextWindow = 0;
  let nextReader = 0;
  let mainHidden = false;
  let appExited = false;
  const images = new Map<string, string>();
  const key = (request: ReaderRequest) =>
    request.kind === "library"
      ? `library:${request.rootId}:${request.entryId}`
      : `${request.source}:${request.workId}`;
  const watch = (page: Page) =>
    page.on("pageerror", (error) => errors.push(error.message));
  watch(main);
  await context.route("**/*", (route) =>
    new URL(route.request().url()).hostname === "127.0.0.1"
      ? route.continue()
      : route.abort(),
  );

  async function installBridge(page: Page, label: string) {
    await page.addInitScript(
      ({ label }) => {
        const callbacks = new Map<number, (event: unknown) => void>();
        const listeners = new Map<
          number,
          {
            event: string;
            handler: number;
            target: EventTarget;
          }
        >();
        const deliveries: EventDelivery[] = [];
        const wrapped = new WeakSet<object>();
        let sequence = 0;
        let releaseQueueRead: (() => void) | undefined;
        const state: Window["readerWindowHarness"] =
          (window.readerWindowHarness = {
            holdQueueRead: false,
            waitingQueueRead: false,
            releaseQueueRead() {
              releaseQueueRead?.();
              releaseQueueRead = undefined;
            },
            attach(value: unknown) {
              const bridge = value as Bridge;
              if (wrapped.has(bridge)) return;
              wrapped.add(bridge);
              const original = bridge.invoke.bind(bridge);
              bridge.metadata = {
                currentWindow: { label },
                currentWebview: { label },
              };
              bridge.transformCallback = (callback, once = false) => {
                const id = ++sequence;
                callbacks.set(id, (event) => {
                  if (once) callbacks.delete(id);
                  callback(event);
                });
                return id;
              };
              bridge.unregisterCallback = (id) => {
                callbacks.delete(id);
              };
              (
                window as unknown as {
                  __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
                }
              ).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener(_event: string, eventId: number) {
                  listeners.delete(eventId);
                },
              };
              bridge.invoke = async (command, args = {}) => {
                if (command === "plugin:event|listen") {
                  const id = ++sequence;
                  listeners.set(id, {
                    event: String(args.event),
                    handler: Number(args.handler),
                    target: (args.target ?? { kind: "Any" }) as EventTarget,
                  });
                  return id;
                }
                if (command === "plugin:event|unlisten") {
                  listeners.delete(Number(args.eventId));
                  return null;
                }
                if (command.startsWith("reader_"))
                  return window.readerWindowInvoke(command, args);
                if (command === "jm_download_read" && state.holdQueueRead) {
                  state.holdQueueRead = false;
                  state.waitingQueueRead = true;
                  await new Promise<void>((resolve) => {
                    releaseQueueRead = resolve;
                  });
                  state.waitingQueueRead = false;
                }
                return original(command, args);
              };
            },
            emit(targetLabel: string, event: string, payload: unknown) {
              for (const [id, listener] of [...listeners]) {
                // Tauri emit_to(label) matches Any listeners in every webview,
                // as well as listeners explicitly bound to this native label.
                const matches =
                  listener.target.kind === "Any" ||
                  ("label" in listener.target &&
                    listener.target.label === targetLabel);
                if (listener.event === event && matches) {
                  deliveries.push({
                    event,
                    target: structuredClone(listener.target),
                  });
                  callbacks.get(listener.handler)?.({ event, id, payload });
                }
              }
            },
            events: () =>
              [...listeners.values()].map((listener) => listener.event),
            eventDeliveries: () => structuredClone(deliveries),
          });
        const testWindow = window as unknown as {
          __TAURI_INTERNALS__?: Bridge;
        };
        if (label !== "main") {
          testWindow.__TAURI_INTERNALS__ = {
            invoke: (command, args = {}) =>
              window.readerWindowInvoke(command, args),
          };
        }
        // Both init-script orders are supported: the workflow fixture also calls
        // attach once its ordinary App bridge is available.
        if (testWindow.__TAURI_INTERNALS__)
          state.attach(testWindow.__TAURI_INTERNALS__);
      },
      { label },
    );
  }

  const emit = async (
    label: string,
    event: string,
    payload: unknown = null,
  ) => {
    const page = pages.get(label)!;
    await expect
      .poll(() => page.evaluate(() => window.readerWindowHarness.events()))
      .toContain(event);
    // Do not route directly to one browser Page: that hid global Any listeners
    // which receive native targeted events and used to close all reader windows.
    await Promise.all(
      [...pages]
        .filter(
          ([pageLabel, candidate]) =>
            !candidate.isClosed() && !closed.has(pageLabel),
        )
        .map(([, candidate]) =>
          candidate.evaluate(
            ({ label, event, payload }) =>
              window.readerWindowHarness.emit(label, event, payload),
            { label, event, payload },
          ),
        ),
    );
  };
  function sessionFor(label: string, readerId: unknown) {
    const session = sessions.get(String(readerId));
    if (!session || session.closed || session.label !== label) {
      violations.push(`reader ownership: ${label}/${String(readerId)}`);
      throw new Error("READER_CLOSED");
    }
    return session;
  }
  await context.exposeBinding(
    "readerWindowInvoke",
    async ({ page }, command: string, args: Record<string, unknown> = {}) => {
      const label = labels.get(page)!;
      calls.push({ label, command, args: structuredClone(args) });
      switch (command) {
        case "reader_window_open": {
          expect(label).toBe("main");
          const request = args.request as ReaderRequest;
          for (const [existing, previous] of requests) {
            if (!closed.has(existing) && key(previous) === key(request))
              return { label: existing };
          }
          const childLabel = `reader-window-${++nextWindow}`;
          requests.set(childLabel, structuredClone(request));
          pinned.set(childLabel, false);
          const child = await context.newPage();
          watch(child);
          pages.set(childLabel, child);
          labels.set(child, childLabel);
          await child.setViewportSize({ width: 420, height: 800 });
          await installBridge(child, childLabel);
          await child.goto("/#reader-window");
          return { label: childLabel };
        }
        case "reader_window_context":
          expect(label).not.toBe("main");
          return { request: structuredClone(requests.get(label)) };
        case "reader_open": {
          const request = args.request as ReaderRequest;
          if (label !== "main") expect(request).toEqual(requests.get(label));
          if (cancelled.has(`${label}:${String(args.requestId)}`))
            throw new Error("READER_CLOSED");
          const readerId = `reader-${++nextReader}`;
          sessions.set(readerId, {
            label,
            request: structuredClone(request),
            closed: false,
          });
          const local = request.kind === "library";
          return {
            readerId,
            title: local ? "合成本地图书 A" : "合成在线图书 B",
            origin: local ? "library" : request.source,
            sourceRef: local
              ? { source: "JM", workId: "101" }
              : { source: request.source, workId: request.workId },
            chapters: [
              { id: "one", title: "第一章", pageCount: null },
              { id: "two", title: "第二章", pageCount: null },
            ],
            position: saved.get(key(request)) ?? null,
          };
        }
        case "reader_cancel_open":
          cancelled.add(`${label}:${String(args.requestId)}`);
          return null;
        case "reader_chapter":
          sessionFor(label, args.readerId);
          return {
            readerId: args.readerId,
            chapterId: args.chapterId,
            pageCount: args.chapterId === "one" ? 6 : 3,
          };
        case "reader_page": {
          const session = sessionFor(label, args.readerId);
          return {
            readerId: args.readerId,
            chapterId: args.chapterId,
            pageIndex: args.pageIndex,
            dataUrl: images.get(
              session.request.kind === "library" ? "local" : "online",
            ),
            width: 720,
            height: 1000,
          };
        }
        case "reader_save_position": {
          const session = sessionFor(label, args.readerId);
          if (holdSave.has(label))
            await new Promise<void>((resolve) => {
              pendingSave.set(label, resolve);
            });
          saved.set(
            key(session.request),
            structuredClone(args.position as ReaderPosition),
          );
          return null;
        }
        case "reader_close": {
          const session = sessions.get(String(args.readerId));
          if (session && session.label !== label)
            violations.push(`close ownership: ${label}`);
          if (session) session.closed = true;
          return null;
        }
        case "reader_fullscreen":
          return null;
        case "reader_window_pin":
          if (failPin.has(label)) throw new Error("READER_WINDOW_PIN_FAILED");
          pinned.set(label, args.pinned === true);
          return null;
        case "reader_window_show_main":
          mainHidden = false;
          return null;
        case "reader_main_ready":
          expect(label).toBe("main");
          return null;
        case "reader_main_close":
          expect(label).toBe("main");
          mainHidden = true;
          appExited = [...requests.keys()].every((child) => closed.has(child));
          return null;
        case "reader_window_close":
          closed.add(label);
          appExited =
            mainHidden &&
            [...requests.keys()].every((child) => closed.has(child));
          return null;
        case "reader_window_download": {
          const session = sessionFor(label, args.readerId);
          expect(session.request.kind).toBe("source");
          if (session.request.kind !== "source")
            throw new Error("READER_DOWNLOAD_UNAVAILABLE");
          mainHidden = false;
          await emit("main", "reader-window-download", {
            source: session.request.source,
            workId: session.request.workId,
          });
          return null;
        }
        default:
          violations.push(`unexpected command: ${command}`);
          throw new Error("Unexpected reader-window command " + command);
      }
    },
  );
  await installBridge(main, "main");
  await installWorkflow(main, { enhanceBridge: true });
  for (const [name, color] of [
    ["local", "#543978"],
    ["online", "#265d55"],
  ]) {
    images.set(
      name,
      await main.evaluate(
        ({ name, color }) => {
          const canvas = document.createElement("canvas");
          canvas.width = 720;
          canvas.height = 1000;
          const ctx = canvas.getContext("2d")!;
          ctx.fillStyle = color;
          ctx.fillRect(0, 0, 720, 1000);
          ctx.fillStyle = "white";
          ctx.font = "48px sans-serif";
          ctx.fillText(`Synthetic ${name}`, 25, 100);
          return canvas.toDataURL("image/png");
        },
        { name, color },
      ),
    );
  }
  return {
    calls,
    errors,
    violations,
    pages,
    requests,
    sessions,
    saved,
    pinned,
    closed,
    failPin,
    holdSave,
    pendingSave,
    emit,
    key,
    releaseSave(label: string) {
      holdSave.delete(label);
      const release = pendingSave.get(label);
      pendingSave.delete(label);
      release?.();
    },
    get mainHidden() {
      return mainHidden;
    },
    get appExited() {
      return appExited;
    },
    async child(number: number, pageText = "1 / 6") {
      const label = `reader-window-${number}`;
      await expect.poll(() => pages.has(label)).toBe(true);
      const child = pages.get(label)!;
      await expect(child.getByTestId("reader-viewport")).toBeVisible();
      await expect(child.getByLabel("当前页码")).toHaveText(pageText);
      const currentPage = Number(pageText.split(" / ")[0]);
      await expect(
        child.getByRole("img", { name: `第 ${currentPage} 页`, exact: true }),
      ).toBeVisible();
      return child;
    },
  };
}

export type ReaderWindowHarness = Awaited<
  ReturnType<typeof installReaderWindows>
>;
