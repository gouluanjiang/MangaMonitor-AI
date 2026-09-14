# Random author audit: repair and post-fix results

## Current result

Head `729c415e0b4ac372adac4c350099fb7fde2da2e6` repairs the native author-update literal-filter defect. All required engineering workflows passed. After the repair, all original 50 query names were searched again and 50 additional seeded random names were checked on both JM and the user's signed-in Pica website. The two groups are disjoint after name normalization and known equivalent-name corrections made before searching. Private evidence asserts the original query list is unchanged and each source has all 100 completed query ranges.

| Group | Source | Queries | Pages | Returned records | Empty queries |
| --- | --- | ---: | ---: | ---: | ---: |
| Original recheck | JM | 50 | 62 | 2,231 | 0 |
| Additional random sample | JM | 50 | 62 | 2,524 | 0 |
| Original recheck | Pica | 50 | 141 | 2,244 | 3 |
| Additional random sample | Pica | 50 | 371 | 6,864 | 4 |

These are per-query counts, not globally unique works or a verified bibliography. Pica empty results count as one initial query page; repeated browser-recovery reads are not added. Sample names may represent authors or circles, not 100 independently verified people. No pre-fix result was counted as a post-fix pass.

Fresh JM metadata completed 124 pages / 4,755 records. Replay through the current frontend source and author search controllers preserved every returned ID. All original 50 fresh ID sets also match their prior baseline. This is live same-route public data plus local diagnostic replay, not 100 native real-account UI scans.

Pica completed 512 page counts / 9,108 records through signed-in browser UI, checking query input, visible pagination and loaded cards. One original query used two adjacent footer-verified segments after browser interruption; large additional queries used contiguous segments with card-count checks. The largest was 3,217 records / 161 pages. No credentials or hidden application state were extracted. The website cards do not expose usable source IDs, so this evidence does not establish website/native Pica ID equality. The repaired native scanner has dual-source synthetic regression coverage; native real-account end-to-end user acceptance remains separate and pending.

### Remaining source-precision limitation

Pica short keywords can return broad results bearing other author labels. In three inspected large results, only 60 of 3,217, 19 of 912, and 12 of 300 visible author fields contain the full query string. This is label evidence, not an independent judgment that every other record is unrelated. Empty queries also do not establish an author's absence from the source; alternate spellings may matter. The repair intentionally retains full source keyword results and explains this scope. It does not infer author identities or guarantee complete bibliography coverage. Do not report all author-search accuracy issues resolved.

### Engineering evidence and delivery

- UI run `34837406767`: 142 logic tests, 100 Chromium tests, formatting, types and build passed. Updated author-search/update screenshots were inspected.
- Baseline run `34837406724`: passed.
- Windows run `34837406755`: passed, including 55 account unit tests, 59 download unit tests, 32 native tests, Clippy and actual WebView startup/restart.
- Test merge `d76ba75a7d0a3f1dd187ae834be5405cf0c7af25`; Dev artifact `10344649167`, version 0.3.4, 20,348,416 executable bytes. Archive digest matches GitHub and ZIP CRC passed. EXE SHA-256 `b6b9b9cca3204c30014badeefaa59023336c0fa6604300f57f8551eb51fd3bd4`.

The Dev executable and Chinese final report were delivered locally without an installer or automatic launch. Formal suites/builds ran only in CI. Private sampling, queries and diagnostic replay were separate read-only work; no real follows, library/ownership changes or downloads were performed. Keep draft PR #19 unmerged and production disabled. Evidence-only documentation stays local for the next necessary code push.

Current position: repair engineering checks and requested 50 + 50 source audit complete; source-precision limits remain explicit. Next are necessary stage 4 UI/settings finishing and stage 5 integrated V1 acceptance. Existing-library assisted reconciliation stays after V1; cancelled matching features remain cancelled.

## Pre-fix diagnostic history

The user explicitly requested a substantial random sample of authors in the existing library, after the full-selection batch. This authorizes a limited read-only sample, not library reconciliation, a full media scan, follows, inventory mutations or downloads. Private author names, manga filenames, source IDs and per-query responses remain only in the local `MangaMonitor-author-audit-20260914` evidence directory and must not enter Git or PR text.

Sample: 50 distinct query names derived from seeded random filename-author groups, with one small metadata inspection per selected archive. Of those samples, 13 have explicit JM source IDs. This is not a claim about the number of distinct real people or coverage of all source versions.

### Original baseline evidence

- All 50 JM queries completed their pagination: 62 public text requests, 2,231 returned records across queries, nine authors with multiple pages, largest result 218. Counts and IDs were consistent; all 13 explicit library sample IDs were found.
- A bounded offline diagnostic replayed these captured DTOs through the actual frontend page validation, `readCompleteSearch`, and ad-hoc author search controller from `0905e16`. Every captured JM ID was retained for all 50 queries. No native session or Pica response was simulated as live success; this distinct real-metadata diagnostic did not repeat the formal CI suites.
- Ten additional public text-detail requests confirmed that some non-empty search author strings concatenate coauthors, contain circle parentheses, or truncate names. At least five examined records contain the requested name in the detail author array even though the current update scanner would discard the search row first.

## Confirmed update-scanner issue and repair

Ad-hoc author search retained the source keyword query's full result. Before this repair, native `discovery_scan` instead demanded literal author equality; it read detail only when the author list was empty, then skipped non-empty mismatches. This could silently remove valid coauthored or differently formatted author records from updates. The sample had 34 queries with non-exact strings containing the query name (566 records); these were potential affected records, not 566 independently established omissions. Another 372 empty author fields would cause detail reads under that scanner.

The user subsequently requested repair and supplied a signed-in Pica browser session. The chosen repair aligns native updates with the source keyword results already retained by ad-hoc search. A source list author label no longer excludes works or triggers author-only detail fanout. Shared source IDs retain every query membership, historical omissions stay visible, and metadata evidence does not grant ownership. Existing pagination/error/cancellation and account guards remain. The page explains this scope and distinguishes a complete zero-result query.

Regression coverage includes both sources, two pages, circle/coauthor/case/blank/other-keyword author labels, zero detail fanout, persisted multi-query records and retained omissions. UI coverage checks that non-literal keyword results remain visible and a complete empty query is not reported as all-owned. These changes passed the exact-head CI recorded above; no local formal suite was duplicated.

## Post-fix audit requirement and provenance

The user required the original 50 authors to be searched again after repair, plus a new seeded random group of 50 from the same library author pool, disjoint from the original query names and formatting equivalents. Both JM and Pica are included. Separate post-fix files, a 200-row local result table, sampling provenance and evidence hashes are retained outside Git. This audit is complete with the boundaries recorded above.

Pica browser access is now available. Initial pre-fix read-only queries also expose circle-style labels that literal equality would exclude. These are diagnostic baseline observations, not the post-fix audit. Do not export browser credentials or private website/library data. Local detailed evidence stays in the existing audit directory and a separate post-fix audit directory under Documents/Codex.

The current validated Dev is `729c415`, version 0.3.4. Older `0905e16` diagnostics remain historical evidence, not the repaired executable or the post-fix audit.
