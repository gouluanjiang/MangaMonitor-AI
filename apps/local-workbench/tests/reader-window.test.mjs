import test from "node:test";
import assert from "node:assert/strict";
import {
  ReaderLifetime,
  ReaderProgressWriter,
  readerRequestedPages,
} from "../src/reader/model.ts";
import {
  createReaderWindowAdapter,
  listenReaderEvent,
  parseReaderWindowContext,
} from "../src/reader/window-runtime.ts";
import { readerErrorMessage } from "../src/reader/runtime.ts";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};

test("reader event subscriptions use the current native window label and directed events cannot close another window", async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const callbacks = new Map();
  const subscriptions = new Map();
  const calls = [];
  const delivered = [];
  let sequence = 0;
  const bridge = {
    metadata: {
      currentWindow: { label: "reader-window-A" },
      currentWebview: { label: "reader-window-A" },
    },
    transformCallback(callback) {
      const id = ++sequence;
      callbacks.set(id, callback);
      return id;
    },
    async invoke(command, args) {
      calls.push({ command, args });
      if (command === "plugin:event|listen") {
        const id = ++sequence;
        subscriptions.set(id, args);
        return id;
      }
      if (command === "plugin:event|unlisten") {
        subscriptions.delete(args.eventId);
        return null;
      }
      throw new Error(`Unexpected window command: ${command}`);
    },
  };
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: bridge,
      __TAURI_EVENT_PLUGIN_INTERNALS__: {
        unregisterListener(_event, eventId) {
          subscriptions.delete(eventId);
        },
      },
    },
  });
  // Match Tauri's Any-or-label dispatch rather than delivering only to the
  // browser page named by a test. An unscoped listener must fail this test.
  const emitTo = (label, event, payload) => {
    for (const [id, subscription] of subscriptions) {
      if (
        subscription.event === event &&
        (subscription.target.kind === "Any" ||
          subscription.target.label === label)
      )
        callbacks.get(subscription.handler)({ event, id, payload });
    }
  };
  try {
    const stopA = await listenReaderEvent("reader-window-close-requested", () =>
      delivered.push("A"),
    );
    bridge.metadata.currentWindow.label = "reader-window-B";
    const stopB = await listenReaderEvent("reader-window-close-requested", () =>
      delivered.push("B"),
    );
    bridge.metadata.currentWindow.label = "main";
    const stopMain = await listenReaderEvent(
      "reader-window-download",
      (reference) => delivered.push(reference),
    );
    assert.deepEqual(
      calls.map(({ args }) => args.target),
      [
        { kind: "Window", label: "reader-window-A" },
        { kind: "Window", label: "reader-window-B" },
        { kind: "Window", label: "main" },
      ],
    );
    emitTo("reader-window-A", "reader-window-close-requested", null);
    assert.deepEqual(delivered, ["A"]);
    emitTo("reader-window-B", "reader-window-close-requested", null);
    assert.deepEqual(delivered, ["A", "B"]);
    const reference = { source: "JM", workId: "102" };
    emitTo("main", "reader-window-download", reference);
    assert.deepEqual(delivered, ["A", "B", reference]);
    await stopA();
    emitTo("reader-window-A", "reader-window-close-requested", null);
    assert.deepEqual(delivered, ["A", "B", reference]);
    await stopB();
    await stopMain();
    assert.equal(subscriptions.size, 0);
  } finally {
    if (previousWindow)
      Object.defineProperty(globalThis, "window", previousWindow);
    else delete globalThis.window;
  }
});

test("window context comes only from the native envelope and keeps distinct source/library request identities", async () => {
  assert.match(readerErrorMessage({ code: "READER_WINDOW_LIMIT" }), /阅读小窗/);
  const request = {
    kind: "library",
    rootId: "root-A",
    generation: 4,
    entryId: "entry-A",
  };
  assert.deepEqual(parseReaderWindowContext({ request }), { request });
  assert.deepEqual(
    parseReaderWindowContext({
      request: {
        kind: "source",
        source: "Pica",
        sessionId: "session-B",
        workId: "work-B",
      },
    }).request.workId,
    "work-B",
  );
  for (const value of [
    null,
    {},
    { request: { ...request, generation: -1 } },
    {
      request: { kind: "source", source: "other", sessionId: "s", workId: "w" },
    },
  ])
    assert.throws(() => parseReaderWindowContext(value));
  const calls = [];
  const adapter = createReaderWindowAdapter({
    invoke: async (command, args) => {
      calls.push({ command, args });
      return command === "reader_window_context" ? { request } : null;
    },
  });
  assert.deepEqual(await adapter.context(), { request });
  await adapter.pin(true);
  await adapter.showMain();
  await adapter.download("reader-A");
  await adapter.close();
  assert.deepEqual(calls, [
    { command: "reader_window_context", args: undefined },
    { command: "reader_window_pin", args: { pinned: true } },
    { command: "reader_window_show_main", args: undefined },
    { command: "reader_window_download", args: { readerId: "reader-A" } },
    { command: "reader_window_close", args: undefined },
  ]);
});

test("closing a reading window awaits its latest progress before its native close and never closes another reader", async () => {
  const held = deferred();
  const events = [];
  const writer = new ReaderProgressWriter(async (value) => {
    await held.promise;
    events.push(["save-A", value.pageIndex]);
  });
  writer.set({ chapterId: "A", pageIndex: 8, offset: 0.4 });
  const a = new ReaderLifetime(async () => {
    events.push("cancel-A");
  });
  const b = new ReaderLifetime(async () => {
    events.push("cancel-B");
  });
  a.attach(async () => {
    await writer.flush();
    events.push("close-A");
  });
  b.attach(async () => {
    events.push("close-B");
  });
  const closing = a.close();
  assert.equal(a.close(), closing);
  assert.deepEqual(events, []);
  held.resolve();
  await closing;
  assert.deepEqual(events, [["save-A", 8], "close-A"]);
  await b.close();
  assert.deepEqual(events.at(-1), "close-B");
});

test("closing before open completes cancels once, closes the obsolete late result and does not cancel a newer lifetime", async () => {
  const events = [];
  const old = new ReaderLifetime(async () => {
    events.push("cancel-old");
  });
  const fresh = new ReaderLifetime(async () => {
    events.push("cancel-fresh");
  });
  await Promise.all([old.close(), old.close()]);
  assert.equal(
    old.attach(async () => {
      events.push("close-old-result");
    }),
    false,
  );
  assert.equal(
    fresh.attach(async () => {
      events.push("close-fresh");
    }),
    true,
  );
  assert.deepEqual(events, ["cancel-old", "close-old-result"]);
  await fresh.close();
  assert.deepEqual(events, ["cancel-old", "close-old-result", "close-fresh"]);
});

test("unfocused reading retains visible pages but omits speculative neighbors until focused", () => {
  assert.deepEqual(readerRequestedPages(5, 20, [5, 6], false), [5, 6]);
  assert.deepEqual(readerRequestedPages(5, 20, [5, 6], true), [5, 6, 4, 7, 3]);
  assert.deepEqual(readerRequestedPages(0, 0, [], true), []);
  assert.ok(
    readerRequestedPages(
      0,
      50000,
      Array.from({ length: 50 }, (_, i) => i),
      true,
    ).length <= 12,
  );
});
