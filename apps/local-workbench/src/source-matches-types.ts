import type { LibraryReference } from "./library-types.ts";

export interface SourceMatchWork extends LibraryReference {
  title: string;
}
export interface SourceMatchPair {
  id: string;
  jm: SourceMatchWork;
  pica: SourceMatchWork;
  confirmedAt: number;
  evidence: "manual";
}
export interface SourceMatchesSnapshot {
  revision: number;
  pairs: SourceMatchPair[];
}
export interface SourceMatchesAdapter {
  read(): Promise<SourceMatchesSnapshot>;
  confirm(
    revision: number,
    jm: SourceMatchWork,
    pica: SourceMatchWork,
  ): Promise<SourceMatchesSnapshot>;
  unlink(revision: number, pairId: string): Promise<SourceMatchesSnapshot>;
}
export const emptySourceMatches = (): SourceMatchesSnapshot => ({
  revision: 0,
  pairs: [],
});
