import type { LibraryReference } from "./library-types.ts";
export interface PhoneLibraryEntry {
  id: string;
  name: string;
  reference: LibraryReference | null;
  markedAt: number;
}
export interface PhoneLibrarySnapshot {
  revision: number;
  importedNames: string[];
  importedAt: number | null;
  importFileName: string | null;
  manualEntries: PhoneLibraryEntry[];
}
export interface PhoneLibraryAdapter {
  read(): Promise<PhoneLibrarySnapshot>;
  import(revision: number): Promise<PhoneLibrarySnapshot | null>;
  mark(
    revision: number,
    name: string,
    reference: LibraryReference | null,
  ): Promise<PhoneLibrarySnapshot>;
  unmark(revision: number, entryId: string): Promise<PhoneLibrarySnapshot>;
}
export const emptyPhoneLibrary = (): PhoneLibrarySnapshot => ({
  revision: 0,
  importedNames: [],
  importedAt: null,
  importFileName: null,
  manualEntries: [],
});
