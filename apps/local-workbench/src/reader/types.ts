export type ReaderSourceRef = { source: "JM" | "Pica"; workId: string };
export type ReaderRequest =
  | { kind: "library"; rootId: string; generation: number; entryId: string }
  | ({ kind: "source"; sessionId: string } & ReaderSourceRef);
export type ReaderPosition = {
  chapterId: string;
  pageIndex: number;
  offset: number;
};
export type ReaderChapter = {
  id: string;
  title: string;
  pageCount: number | null;
};
export type ReaderBook = {
  readerId: string;
  title: string;
  origin: "library" | "JM" | "Pica";
  sourceRef: ReaderSourceRef | null;
  chapters: ReaderChapter[];
  position: ReaderPosition | null;
};
export type ReaderChapterInfo = {
  readerId: string;
  chapterId: string;
  pageCount: number;
};
export type ReaderImage = {
  readerId: string;
  chapterId: string;
  pageIndex: number;
  dataUrl: string;
  width: number;
  height: number;
};
export interface ReaderAdapter {
  open(request: ReaderRequest, requestId: string): Promise<ReaderBook>;
  cancelOpen(requestId: string): Promise<void>;
  chapter(readerId: string, chapterId: string): Promise<ReaderChapterInfo>;
  page(
    readerId: string,
    chapterId: string,
    pageIndex: number,
  ): Promise<ReaderImage>;
  savePosition(readerId: string, position: ReaderPosition): Promise<void>;
  close(readerId: string): Promise<void>;
  fullscreen(fullscreen: boolean): Promise<void>;
}
