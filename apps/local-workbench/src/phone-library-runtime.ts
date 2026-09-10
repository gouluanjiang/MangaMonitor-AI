import { invokeDesktop, isDesktopRuntime } from "./runtime.ts";
import { LibraryError } from "./library-runtime.ts";
import { parseLibraryReference } from "./library-model.ts";
import type {
  PhoneLibraryAdapter,
  PhoneLibrarySnapshot,
} from "./phone-library-types.ts";
import type { LibraryReference } from "./library-types.ts";
const fail = (): never => {
  throw new LibraryError("PHONE_LIBRARY_INVALID");
};
function name(value: unknown, manual = false): string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    [...value].length > 1024 ||
    /[\x00-\x1f\x7f]/.test(value) ||
    (!manual &&
      (/[\\/]/.test(value) ||
        /^[a-z]:/i.test(value.trim()) ||
        [".", ".."].includes(value.trim())))
  )
    return fail();
  return value;
}
const integer = (value: unknown): number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : fail();
export function validatePhoneLibrarySnapshot(
  value: unknown,
): PhoneLibrarySnapshot {
  if (typeof value !== "object" || value === null) return fail();
  const raw = value as Record<string, unknown>;
  if (
    !Array.isArray(raw.importedNames) ||
    raw.importedNames.length > 20000 ||
    !Array.isArray(raw.manualEntries) ||
    raw.manualEntries.length > 20000
  )
    return fail();
  const importedNames = raw.importedNames.map((value) => name(value));
  const manualEntries = raw.manualEntries.map((value) => {
    if (typeof value !== "object" || value === null) return fail();
    const entry = value as Record<string, unknown>;
    if (typeof entry.id !== "string" || !/^[a-f0-9]{64}$/.test(entry.id))
      return fail();
    let reference: LibraryReference | null = null;
    if (entry.reference !== null) {
      const ref = entry.reference as Record<string, unknown>;
      if (
        !ref ||
        (ref.source !== "JM" && ref.source !== "Pica") ||
        typeof ref.workId !== "string"
      )
        return fail();
      reference = parseLibraryReference(ref.source, ref.workId);
      if (!reference || reference.workId !== ref.workId) return fail();
    }
    return {
      id: entry.id,
      name: name(entry.name, true),
      reference,
      markedAt: integer(entry.markedAt),
    };
  });
  if (
    new Set(manualEntries.map((entry) => entry.id)).size !==
    manualEntries.length
  )
    return fail();
  return {
    revision: integer(raw.revision),
    importedNames,
    manualEntries,
    importedAt: raw.importedAt === null ? null : integer(raw.importedAt),
    importFileName:
      raw.importFileName === null ? null : name(raw.importFileName),
  };
}
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export function createPhoneLibraryAdapter(
  options: { invoke?: Invoke; native?: boolean } = {},
): PhoneLibraryAdapter {
  const invoke = options.invoke ?? invokeDesktop;
  async function call(
    command: string,
    args: Record<string, unknown> = {},
  ): Promise<PhoneLibrarySnapshot | null> {
    if (!(options.native ?? isDesktopRuntime()))
      throw new LibraryError("DESKTOP_REQUIRED");
    let result: unknown;
    try {
      result = await invoke(command, args);
    } catch (cause) {
      const code = (cause as { code?: unknown })?.code;
      throw new LibraryError(
        typeof code === "string" && /^[A-Z_]{1,80}$/.test(code)
          ? code
          : "PHONE_LIBRARY_UNAVAILABLE",
      );
    }
    return result === null ? null : validatePhoneLibrarySnapshot(result);
  }
  const required = async (command: string, args?: Record<string, unknown>) => {
    const result = await call(command, args);
    return result ?? fail();
  };
  return {
    read: () => required("phone_library_read"),
    import: (revision) => call("phone_library_import", { revision }),
    mark: (revision, name, reference) =>
      required("phone_library_mark", { revision, name, reference }),
    unmark: (revision, entryId) =>
      required("phone_library_unmark", { revision, entryId }),
  };
}
