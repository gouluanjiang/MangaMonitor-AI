# Author attribution acceptance repair

## Problem and behavior

A real acceptance sample belonged to another author. Generic keyword search results had all been assigned to the queried author; retaining every page did not establish authorship. This repair applies one shared author-evidence projection to ad-hoc author search and saved/manual followed-author updates.

- The default author list and its ownership counts include only source author fields that explicitly contain the queried name as a complete name component. Unicode composition/width/case/spacing, circle-with-author labels and explicitly separated coauthors are supported.
- Substrings, kana/voicing folding, titles, tags, descriptions, saved query memberships and the legacy `authorVerified` bit cannot establish authorship. Sharing only a circle name does not establish a particular member.
- Other keyword results remain available through a separate inspection view with their actual author fields. Missing author metadata is labelled as missing, never replaced by the search term. This view has no author-list download or multi-select action; a user can inspect source details normally.
- Complete pagination and raw saved results remain unchanged. No source requests are added, no per-work detail fanout occurs, and older snapshots receive the same projection immediately without destructive migration.
- Author all-owned messages require a complete query, an unfiltered nonempty author scope with every work owned, and no remaining unconfirmed keyword results. Source, author and text filters do not silently broaden this assertion. Download ownership remains same-source receipts plus actual files.

## Verification

Local read-only replay used the previously captured 100 JM author queries (4,755 records): 3,140 have explicit author evidence; 1,615 remain accessible as other keyword results. Every former literal-author match is retained. This is an offline regression diagnostic, not 100 new live searches or a verified bibliography.

The native acceptance snapshot projects to 15 JM author results and 6 Pica author results, with 6 additional Pica keyword results. The user-identified wrong-author sample is excluded from the author list. The private store's bytes were unchanged. Private titles, IDs and full responses are kept outside Git.

New synthetic regressions cover Unicode and explicit circle/coauthor labels, substring and kana collisions, missing/truncated metadata, old saved flags, per-author membership of shared records, complete ad-hoc pagination, default counts, incomplete authorship/all-owned reporting and batch-selection exclusion. All required formal checks/builds passed in CI; no local duplicate suite or build was run.

Head `44da160475ce9b2774f67e4c136ef879559e3acc` passed frontend `34984486124` (149 logic / 111 Chromium), baseline `34984486095`, and Windows `34984486220` (34 native IPC tests, module/credential checks, Clippy, EXE build and actual WebView startup/restart). Dev 0.3.4 artifact `10402543580`, test merge `f87e8bc05d84df8a807d1c43bdc31d466a099b0c`, passed GitHub digest, ZIP CRC, PE and embedded-head verification. EXE SHA-256 `a7719360aa1ba7cc1633f167af472ae6ca210daf897294bf068f48b12a1c2649`.

Corrected native acceptance passed: existing saved results immediately show 21 author records plus 6 other keyword results, without starting a query; the unsuitable sample is only in the other-results view and retains its actual author and owned state. A manual fresh check completed both sources and retained the same projection. Actual select-all selected 21, then was cleared without preparation or download. User-assisted text entry enabled an independent real two-source search: 140 returned records were separated into 69 explicit author records and 71 other results, with one existing same-source receipt correctly owned. Owned filtering and queue/search round trips preserved results, filters and check time. This is not evidence that all 71 other results are unrelated authors. The native helper still cannot write the input through UIA; the user supplied only the search term, after which the agent performed the search and verification. No additional media download or file deletion occurred; the download document hash remained unchanged.

## Boundaries and continuation

This establishes explicit source-metadata correspondence, not globally unique author identity or an exhaustive bibliography. Different aliases, missing/truncated author fields and source metadata errors remain inspectable without being guessed into the author list. The source search endpoints and their category coverage are unchanged.

No additional real download or deletion is part of this repair. The earlier downloaded ZIP is retained. No matching/association/dislike feature is restored. Keep Dev 0.3.4, no installer, draft PR #19 unmerged, and production disabled. This repair's engineering and bounded native acceptance are complete. Return to final V1 acceptance review; do not call an arbitrary source bibliography verified.
