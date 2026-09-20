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
