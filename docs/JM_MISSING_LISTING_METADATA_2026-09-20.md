# JM listing records with missing metadata

## Observed failure

The final unfinished JM author scope retained one page and reported `SOURCE_RESPONSE_INVALID`. A bounded anonymous read of that author's two public search pages reproduced one record with a valid source ID but an empty title, author and image field. A single exact-ID detail read also returned empty title/author metadata. The source returned a retained identifier with missing information; this is not evidence that the author name is invalid or that the work can be treated as absent. Private payloads, IDs, titles and profile snapshots remain outside Git.

The client required a nonblank title for every list item. That record caused the whole later page to fail, withholding otherwise readable results. The page totals and record counts were internally consistent. Update-date parsing, array capacity and duplicate pagination were not the cause.

## Bounded correction

Only JM listing records may represent a valid identifier with a bounded blank-string title using the explicit label `来源作品信息缺失（JM编号）`. Keep the identifier, position, count and source-provided metadata, including author credits if any; never infer authorship from the query. The placeholder has no usable cover descriptor. Search/favorite/ranking list parsing shares this rule. Null, unsupported title types, over-limit titles, invalid identifiers and other malformed required metadata remain errors. Existing nonblank parsing, including already supported numeric text, is unchanged. Pica parsing is unchanged.

Strict detail parsing is retained. Download preparation still requires readable work details; a listing placeholder does not authorize a download or favorite mutation. No per-work fallback request, extra source traversal, silent omission, author deletion, private-store rewrite or manga mutation is introduced.

An entry with no source or retained historical author credit stays in the separately viewable other-keyword results. It is excluded from author counts and author bulk selection. Existing saved useful metadata remains preserved by the unchanged merge rule when a later listing omits authors; the query itself never establishes that evidence. Returned source pagination can finish without pretending that the missing metadata was recovered or that the item was not returned. Existing incomplete-state handling remains in effect for actual errors.

## Verification

Formal tests and builds run in CI only. Focused source regressions preserve a blank metadata entry in a later page with exact counts and no detail fan-out, cover the applicable lists, retain source authors when present, and keep strict detail/other malformed-field behavior. Author tests check full pagination accounting, separate classification and exclusion from bulk actions/all-owned claims.

The current profile and prior uncommitted final reports were backed up before implementation. Live verification is scoped to the one unfinished author/source pair; do not restart the complete all-author scan or reread the already completed other source just to validate this repair. Code validation, bounded native acceptance, and user acceptance remain separate.

## Delivery evidence

Head `4ca34bfe8f15db3cff5621cfaf7a3a16fe737c9b`, test merge `239fca04ce5b400c02bc409b68a22abd05184f21`, passed UI run `35518925424` (164 logic / 145 Chromium), baseline run `35518925486` (64 source tests), and desktop run `35518925418` (79 account / 38 native IPC, Clippy, native build and actual Windows WebView startup/restart). The captured two-page response also passed isolated actual-frontend validation, full traversal, identity/order preservation and author partitioning, with the missing record kept in other-keyword results. This diagnostic is separate from the Rust CI tests and does not constitute native real-account acceptance.

The verified executable is in `Documents/Codex/MangaMonitor-Dev-20260920-4ca34bf`, SHA-256 `8c8f0b482ee32ad8696699bcf544c1cedac979e2e578248abca6c0c1e2e2def7`. Both existing shortcuts now target it, with backups and previous executables retained. Native startup and existing library display passed. After the user reconnected the missing session, the one unfinished JM scope completed its full returned pagination and the unfinished list cleared. The explicit missing-metadata record remains outside the author classification. The UI returned to idle unowned results, with original owned count unchanged. A transient BUSY progress read recovered without manual intervention.

Independent persisted-state audit confirms the selected range is complete, original keys/order/query associations and useful metadata are preserved, no unrelated range or record changed, the same author's completed Pica scope stayed untouched, and the eight unrelated document/cache files remain byte-identical. All discovery changes are attributable to the selected query and single scan. No all-author replay, media download or library mutation was needed. Private native and preservation evidence is saved locally; agent verification is complete and does not claim user acceptance or recovery of the source's missing metadata.
