# Author query eligibility and inventory contention repair

## Problem and behavior

The followed-author batch used raw keyword queries for every saved name. A missing-author placeholder and a one-letter credit can therefore read hundreds of keyword-result pages before UI attribution separates unrelated works. A long result set alone does not prove that a credited author is invalid. The private import audit confirmed a placeholder omission and a real short credit whose explicit full circle/author signature had been reduced to the initial.

Runtime query eligibility now rejects an exact normalized placeholder set and marks single ASCII letter/digit author queries too broad. Fullwidth ASCII, surrounding whitespace and ASCII case fold consistently. Valid one-character CJK names, longer pen names and complete circle/author signatures remain eligible. The generic work-keyword and direct work-ID modes are unchanged; valid authors still read every returned page, with no arbitrary new page cutoff.

Each blocked discovery scope is partial with an actionable local reason and zero requests for that attempt; other authors continue. No new checkpoint is granted. Older blocked ranges project as partial with no usable baseline, and exclusively blocked record associations are hidden from author results without deleting stored history. Records shared with valid authors remain available. Stored document validators remain compatible. Adding placeholder follows is rejected, while removal and real short-name follows remain supported. Both standalone author-search UI paths apply the same rule before source requests. Complete-catalog counts exclude invalidated scopes despite retained historical timestamps.

## Inventory status

During the reported run the UI temporarily displayed all inventory states as unknown, and later recovered. Code review found that a transient private-store `BUSY` could leave inventory unavailable until another explicit refresh; there is no captured error proving that this particular screenshot was caused by `BUSY`.

Native inventory reads now retry only exact `BUSY` up to five attempts with 20 ms gaps, on the existing blocking worker. Persistent contention and real read/root errors still report unknown. The author pages explain the failed inventory check and point to their existing refresh button. Discovery polling does not repeatedly rescan inventory, and no long reconnect mechanism is introduced.

## Verification and private correction

Synthetic regressions cover normalized query eligibility, mixed/fully blocked batches, zero source requests for blocked scopes, old-document projection and preserved records, follow removal, unchanged generic keyword searches, and a valid author with 125 pages. Native regressions use a real local lock, cover bounded contention and recovery, and verify corruption is returned immediately without modifying files. UI regressions cover actionable skipped-range reasons, truthful completeness, rejected standalone/source author entry, unchanged explicit keyword mode, and inventory recovery.

Formal tests/builds run in GitHub Actions only; final CI and bounded native results are pending at this implementation checkpoint. The current batch has been stopped through its normal UI and saved results backed up. Private evidence and the precise proposed following correction stay outside Git in `Documents/Codex/MangaMonitor-author-query-repair-20260919`. Do not delete a real author merely because their name is short, rerun the entire followed list for acceptance, erase discovered history, or modify manga/download data. The full signature must receive a bounded real-source check before claiming the corrected query's coverage.
