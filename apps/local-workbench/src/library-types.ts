import type { Source } from "./source-types.ts";

export interface LibraryReference {
  source: Source;
  workId: string;
}
export interface LibraryItem {
  id: string;
  relativePath: string;
  fileName: string;
  format: "zip" | "cbz" | "rar" | "directory";
  title: string;
  authors: string[];
  description: string | null;
  tags: string[];
  bytes: number;
  modifiedAt: number | null;
  pageCount: number | null;
  coverAvailable: boolean;
  state: "indexed" | "unreadable" | "unsupported";
  errorCode: string | null;
  sourceRef: LibraryReference | null;
  identityEvidence: "metadata" | "filename" | "manual" | null;
}
export interface LibrarySnapshot {
  revision: number;
  rootId: string | null;
  rootPath: string | null;
  generation: number;
  phase: "idle" | "reading" | "paused" | "complete" | "error";
  freshness: "none" | "cached" | "live";
  items: LibraryItem[];
  visited: number;
  skipped: number;
  updatedAt: number | null;
  errorCode: string | null;
}
export type LibraryScanAction = "start" | "next" | "pause" | "resume";
export interface LibraryCover {
  rootId: string;
  generation: number;
  entryId: string;
  dataUrl: string | null;
}
export interface LibraryAdapter {
  read(): Promise<LibrarySnapshot>;
  choose(): Promise<LibrarySnapshot | null>;
  scan(
    rootId: string,
    generation: number,
    action: LibraryScanAction,
  ): Promise<LibrarySnapshot>;
  cover(
    rootId: string,
    generation: number,
    entryId: string,
  ): Promise<LibraryCover>;
  link(
    rootId: string,
    generation: number,
    entryId: string,
    reference: LibraryReference | null,
  ): Promise<LibrarySnapshot>;
}
export const emptyLibrary = (): LibrarySnapshot => ({
  revision: 0,
  rootId: null,
  rootPath: null,
  generation: 0,
  phase: "idle",
  freshness: "none",
  items: [],
  visited: 0,
  skipped: 0,
  updatedAt: null,
  errorCode: null,
});
