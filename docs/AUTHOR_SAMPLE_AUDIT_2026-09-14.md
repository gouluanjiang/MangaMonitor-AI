# Random author audit: current findings

The user explicitly requested a substantial random sample of authors in the existing library, after the full-selection batch. This authorizes a limited read-only sample, not library reconciliation, a full media scan, follows, inventory mutations or downloads. Private author names, manga filenames, source IDs and per-query responses remain only in the local `MangaMonitor-author-audit-20260914` evidence directory and must not enter Git or PR text.

Sample: 50 distinct query names derived from seeded random filename-author groups, with one small metadata inspection per selected archive. Of those samples, 13 have explicit JM source IDs. This is not a claim about the number of distinct real people or coverage of all source versions.

## Completed evidence

- All 50 JM queries completed their pagination: 62 public text requests, 2,231 returned records across queries, nine authors with multiple pages, largest result 218. Counts and IDs were consistent; all 13 explicit library sample IDs were found.
- A bounded offline diagnostic replayed these captured DTOs through the actual frontend page validation, `readCompleteSearch`, and ad-hoc author search controller from `0905e16`. Every captured JM ID was retained for all 50 queries. No native session or Pica response was simulated as live success; this distinct real-metadata diagnostic did not repeat the formal CI suites.
- Ten additional public text-detail requests confirmed that some non-empty search author strings concatenate coauthors, contain circle parentheses, or truncate names. At least five examined records contain the requested name in the detail author array even though the current update scanner would discard the search row first.

## Confirmed update-scanner issue and repair

Ad-hoc author search retains the source keyword query's full result. Native `discovery_scan` instead demands literal author equality; it reads detail only when the author list is empty, then skips non-empty mismatches. This can silently remove valid coauthored or differently formatted author records from updates. The sample has 34 queries with non-exact strings containing the query name (566 records); these are potential affected records, not 566 independently established omissions. Another 372 empty author fields would cause detail reads under the current scanner.

The user subsequently requested repair and supplied a signed-in Pica browser session. The chosen repair aligns native updates with the source keyword results already retained by ad-hoc search. A source list author label no longer excludes works or triggers author-only detail fanout. Shared source IDs retain every query membership, historical omissions stay visible, and metadata evidence does not grant ownership. Existing pagination/error/cancellation and account guards remain. The page explains this scope and distinguishes a complete zero-result query.

Regression coverage includes both sources, two pages, circle/coauthor/case/blank/other-keyword author labels, zero detail fanout, persisted multi-query records and retained omissions. UI coverage checks that non-literal keyword results remain visible and a complete empty query is not reported as all-owned. These changes await the required CI; no local formal suite is duplicated.

## Required post-fix audit

The user requires the original 50 authors to be searched again after repair, plus a new seeded random group of 50 from the same library author pool, disjoint from the original query names and formatting equivalents. Both JM and Pica are included. Store separate results and record incomplete ranges honestly; pre-fix diagnostics cannot count as post-fix passes. This audit is pending at this commit.

Pica browser access is now available. Initial pre-fix read-only queries also expose circle-style labels that literal equality would exclude. These are diagnostic baseline observations, not the post-fix audit. Do not export browser credentials or private website/library data. Local detailed evidence stays in the existing audit directory and a separate post-fix audit directory under Documents/Codex.

Current position: full-selection batch engineering-complete; update-filter repair prepared, formal checks and the requested 100-author post-fix audit next; then stage 4 UI/settings finishing and stage 5 integrated V1 acceptance. The last validated Dev remains `0905e16` until a new build passes; version 0.3.4, draft/unmerged PR, no installer and production disabled.
