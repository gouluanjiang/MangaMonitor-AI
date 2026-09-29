import {
  initialPreferences,
  isWorkbenchPreferences,
  isBackgroundDataUrl,
  PREFERENCES_STORAGE_KEY,
} from "./preferences.ts";
import type { BackgroundSelection, PreferencesStorage } from "./preferences.ts";
import { initialBooklists, validateBooklists } from "./booklists.ts";
import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";

export const NATIVE_BACKGROUND_BYTES = 8 * 1024 * 1024;
export const BOOKLISTS_STORAGE_KEY = "mangamonitor.workbench.booklists.v1";
const MAX_DOCUMENT_CHARS = 12 * 1024 * 1024;

export interface DocumentSnapshot<T> {
  revision: string | number | null;
  value: T;
}

export class PersistenceError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(code);
    this.code = code;
    this.name = "PersistenceError";
  }
}

export function persistenceErrorMessage(error: unknown): string {
  const code =
    error instanceof PersistenceError ? error.code : "STORAGE_UNAVAILABLE";
  if (code === "COMMIT_UNCERTAIN")
    return "写入结果尚未确认。请重新读取核对保存内容，当前草稿已保留。";
  if (code === "CONFLICT" || code === "REVISION_CONFLICT")
    return "数据已在另一处更新。请重新读取后再保存，当前草稿已保留。";
  if (code === "BUSY" || code === "LOCK_BUSY")
    return "本机数据正在写入，请稍后重试。当前草稿已保留。";
  if (
    [
      "INVALID_DOCUMENT",
      "CORRUPT_DOCUMENT",
      "INVALID_SCHEMA",
      "UNSUPPORTED_VERSION",
      "DOCUMENT_CORRUPT",
      "UNSUPPORTED_SCHEMA",
    ].includes(code)
  )
    return "数据损坏或版本暂不支持，原数据已保留。修复后可重新读取。";
  return "本机存储暂不可用，当前草稿已保留。请检查存储并重新读取后重试。";
}

type Invoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;
interface PersistenceOptions {
  native?: boolean;
  invoke?: Invoke;
  storage?: PreferencesStorage;
  fixture?: string | null;
}

function nativeError(error: unknown): PersistenceError {
  if (error instanceof PersistenceError) return error;
  const code =
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
      ? error.code
      : "STORAGE_UNAVAILABLE";
  // Do not surface arbitrary exception text or native paths in the interface.
  return new PersistenceError(code);
}

export function createWorkbenchPersistence(options: PersistenceOptions = {}) {
  const native = options.native ?? isDesktopRuntime();
  const invoke = options.invoke ?? invokeDesktop;
  const suffix = !native && options.fixture ? "." + options.fixture : "";
  function storage() {
    return options.storage ?? window.localStorage;
  }

  function store<T>(
    name: "preferences" | "booklists",
    key: string,
    initial: () => T,
    validate: (value: unknown) => T,
  ) {
    // The browser preview uses a same-origin Web Lock when available. The native
    // adapter has a process/file lock and compares the on-disk revision itself.
    let pending: Promise<unknown> = Promise.resolve();
    async function serialize<R>(action: () => Promise<R>): Promise<R> {
      const run = async () => {
        if (typeof navigator !== "undefined" && navigator.locks)
          return navigator.locks.request(key + suffix, action);
        return action();
      };
      const result = pending.then(run, run);
      pending = result.catch(() => undefined);
      return result;
    }
    function snapshot(value: unknown): DocumentSnapshot<T> {
      if (
        typeof value !== "object" ||
        value === null ||
        Array.isArray(value) ||
        Object.keys(value).length !== 2 ||
        !("revision" in value) ||
        !("value" in value) ||
        typeof value.revision !== "number" ||
        !Number.isSafeInteger(value.revision) ||
        value.revision < 0
      )
        throw new PersistenceError("INVALID_DOCUMENT");
      return { revision: value.revision, value: validate(value.value) };
    }
    async function read(): Promise<DocumentSnapshot<T>> {
      try {
        if (native) return snapshot(await invoke("read_" + name));
        const raw = storage().getItem(key + suffix);
        if (raw === null) return { revision: null, value: initial() };
        if (raw.length > MAX_DOCUMENT_CHARS)
          throw new PersistenceError("INVALID_DOCUMENT");
        let parsed: unknown;
        try {
          parsed = JSON.parse(raw);
        } catch {
          throw new PersistenceError("INVALID_DOCUMENT");
        }
        return { revision: raw, value: validate(parsed) };
      } catch (error) {
        throw nativeError(error);
      }
    }
    async function write(
      previous: DocumentSnapshot<T>,
      value: T,
    ): Promise<DocumentSnapshot<T>> {
      try {
        const valid = validate(value);
        if (native) {
          if (typeof previous.revision !== "number")
            throw new PersistenceError("INVALID_DOCUMENT");
          return snapshot(
            await invoke("write_" + name, {
              expectedRevision: previous.revision,
              value: valid,
            }),
          );
        }
        return await serialize(async () => {
          if (storage().getItem(key + suffix) !== previous.revision)
            throw new PersistenceError("CONFLICT");
          const raw = JSON.stringify(valid);
          if (raw.length > MAX_DOCUMENT_CHARS)
            throw new PersistenceError("INVALID_DOCUMENT");
          storage().setItem(key + suffix, raw);
          return { revision: raw, value: valid };
        });
      } catch (error) {
        throw nativeError(error);
      }
    }
    return { read, write };
  }
  return {
    native,
    preferences: store(
      "preferences",
      PREFERENCES_STORAGE_KEY,
      initialPreferences,
      (value) => {
        if (
          !isWorkbenchPreferences(
            value,
            native ? NATIVE_BACKGROUND_BYTES : undefined,
          )
        )
          throw new PersistenceError("INVALID_DOCUMENT");
        return value;
      },
    ),
    booklists: store(
      "booklists",
      BOOKLISTS_STORAGE_KEY,
      initialBooklists,
      (value) => {
        try {
          return validateBooklists(value);
        } catch {
          throw new PersistenceError("INVALID_DOCUMENT");
        }
      },
    ),
    async chooseBackground(): Promise<BackgroundSelection | null> {
      if (!native) throw new PersistenceError("NATIVE_ONLY");
      try {
        const selected = await invoke("choose_background");
        if (selected === null) return null;
        if (
          typeof selected !== "object" ||
          selected === null ||
          !("backgroundImage" in selected) ||
          !("backgroundName" in selected) ||
          !isBackgroundDataUrl(
            selected.backgroundImage,
            NATIVE_BACKGROUND_BYTES,
          ) ||
          typeof selected.backgroundName !== "string" ||
          !selected.backgroundName.trim()
        )
          throw new PersistenceError("INVALID_BACKGROUND");
        return {
          backgroundImage: selected.backgroundImage,
          backgroundName: selected.backgroundName,
        };
      } catch (error) {
        throw nativeError(error);
      }
    },
  };
}
