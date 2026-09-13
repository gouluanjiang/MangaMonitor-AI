# Library usability follow-up

This batch addresses the user's current library and favorites workflow before continuing roadmap C. Keep Dev 0.3.4, no installer, PR #19 draft/unmerged and production disabled.

## User-visible changes

- Library: admission date ascending/descending; known dates survive rescanning and explicit ZIP path migration. Legacy dates stay unknown and sort last. File modification time remains a separate choice.
- Library filters distinguish readable PC copies, file review and missing source associations. A missing source association does not mean a ZIP is absent.
- Both favorite sources: owned, candidate, unmatched and unknown filters, counts for the searched/read scope, and an empty-state action. Changing a status filter clears hidden multi-selection. Filtering does not silently request further pages.
- A direct Read all favorites action replaces the need to use sorting as an indirect way to prepare a complete search scope. The established sequential reader, pause/retry and canonical ordering are reused.
- Completed favorite catalogs reconcile automatically once per catalog/library generation. Manual reconciliation of the already-read scope is also available. Only title/author/page-count agreement, an unambiguous candidate and a current file identity can add a source link. Creator/event prefixes and archive extensions can differ; edition/language/volume suffixes remain significant. No image or credential is read by reconciliation.
- Candidate details show the local filename, author and both page counts, with an explicit confirmation action. A second source adds an association instead of replacing the first. Existing manual decisions remain authoritative; clearing associations disables automatic relinking of that same unchanged file.
- ZIP indexing accepts up to 50,404 central-directory entries, retaining the 8 MiB central directory bound and all path/format checks. This accommodates a large existing collection without reading every image into memory.

## Affected authority review

The only native write is to the revisioned private library index. Stable root/generation/file identities and the store lock/CAS remain required. File changes discard derived links on reindex. Automatic evidence is labeled `titleAuthorPages`, not manual or source-provided metadata. A same-source conflict, unknown/different page count, ambiguous title, empty or unreadable file, and manual unlink cannot become an automatic positive match. The source family graph and duplicate-download preflight consume these current linked references consistently. Existing download completion still requires the original execution receipt and primary metadata ID; reconciliation cannot complete a download, write media, replace, delete, or start network requests. The download executor's upstream pins, API/image behavior and production gate remain unchanged; the existing JM/Pica reviews apply.

The local maintenance helper invokes the same reconciliation service against an explicitly supplied metadata manifest, with preview/apply modes, a required index revision and a new private result file. Real input/catalogs, filenames, paths and reports stay outside Git. It does not scan or download missing favorites. Cached partial source coverage must be reported as partial.

## Usability review and remaining work

The review covers finding a work, seeing whether the PC has it, preparing a complete favorites search, confirming a candidate and returning to the library. Synthetic verification uses mixed states, ambiguous identities, old timestamps, a large ZIP, filter/search combinations and fresh UI screenshots. Local acceptance separately uses existing metadata without running the CI suites again. These checks do not replace user acceptance or claim a complete V1 UI audit.

After this batch: C version relationships and author completeness; D JM weekly recommendations and Pica rankings; E favorites/work-follow discovery and complete-scope batch planning; F legacy cloud closure; G final UI/settings and V1 acceptance. Reader and in-app updater remain frozen until after V1. Phone inventory, classification booklists and PDF/CBZ export remain cancelled.
