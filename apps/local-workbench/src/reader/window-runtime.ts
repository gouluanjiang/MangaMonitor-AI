import { invokeDesktop } from "../runtime.ts";
import { ReaderError } from "./runtime.ts";
import type { ReaderRequest } from "./types.ts";

export interface ReaderWindowAdapter {
  context(): Promise<{ request: ReaderRequest }>;
  pin(pinned: boolean): Promise<void>;
  showMain(): Promise<void>;
  download(readerId: string): Promise<void>;
  close(): Promise<void>;
  listen(name: string, handler: () => void): Promise<() => void>;
}

export function parseReaderWindowContext(value: unknown): {
  request: ReaderRequest;
} {
  const request =
    value && typeof value === "object" && "request" in value
      ? value.request
      : null;
  if (!request || typeof request !== "object")
    throw new ReaderError("READER_WINDOW_CONTEXT_INVALID");
  const raw = request as Record<string, unknown>;
  const text = (value: unknown): value is string =>
    typeof value === "string" && value.length > 0 && value.length <= 16384;
  if (
    raw.kind === "library" &&
    text(raw.rootId) &&
    Number.isSafeInteger(raw.generation) &&
    (raw.generation as number) >= 0 &&
    text(raw.entryId)
  )
    return {
      request: {
        kind: "library",
        rootId: raw.rootId,
        generation: raw.generation as number,
        entryId: raw.entryId,
      },
    };
  if (
    raw.kind === "source" &&
    (raw.source === "JM" || raw.source === "Pica") &&
    text(raw.sessionId) &&
    text(raw.workId)
  )
    return {
      request: {
        kind: "source",
        source: raw.source,
        sessionId: raw.sessionId,
        workId: raw.workId,
      },
    };
  throw new ReaderError("READER_WINDOW_CONTEXT_INVALID");
}

export function supportsReaderWindowEvents(): boolean {
  const bridge =
    typeof window === "undefined"
      ? undefined
      : (
          window as unknown as {
            __TAURI_INTERNALS__?: { transformCallback?: unknown };
          }
        ).__TAURI_INTERNALS__;
  return typeof bridge?.transformCallback === "function";
}
export async function listenReaderEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  // Existing browser previews only expose invoke; they do not have a native
  // event bus. This does not bypass native caller-window authorization.
  if (!supportsReaderWindowEvents()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(name, (event) => handler(event.payload));
}

export function createReaderWindowAdapter(
  options: {
    invoke?: typeof invokeDesktop;
    listen?: ReaderWindowAdapter["listen"];
  } = {},
): ReaderWindowAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  return {
    async context() {
      return parseReaderWindowContext(await invoke("reader_window_context"));
    },
    async pin(pinned) {
      await invoke("reader_window_pin", { pinned });
    },
    async showMain() {
      await invoke("reader_window_show_main");
    },
    async download(readerId) {
      await invoke("reader_window_download", { readerId });
    },
    async close() {
      await invoke("reader_window_close");
    },
    listen:
      options.listen ?? ((name, handler) => listenReaderEvent(name, handler)),
  };
}

export const nativeReaderWindowAdapter = createReaderWindowAdapter();
