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

New synthetic regressions cover Unicode and explicit circle/coauthor labels, substring and kana collisions, missing/truncated metadata, old saved flags, per-author membership of shared records, complete ad-hoc pagination, default counts, incomplete authorship/all-owned reporting and batch-selection exclusion. Required formal frontend/native checks and candidate validation are pending CI; no local duplicate suite or build is run.

## Boundaries and continuation

This establishes explicit source-metadata correspondence, not globally unique author identity or an exhaustive bibliography. Different aliases, missing/truncated author fields and source metadata errors remain inspectable without being guessed into the author list. The source search endpoints and their category coverage are unchanged.

No additional real download or deletion is part of this repair. The earlier downloaded ZIP is retained. No matching/association/dislike feature is restored. Keep Dev 0.3.4, no installer, draft PR #19 unmerged, and production disabled. Finish this repair's engineering and native acceptance before returning to the remaining V1 acceptance steps.
