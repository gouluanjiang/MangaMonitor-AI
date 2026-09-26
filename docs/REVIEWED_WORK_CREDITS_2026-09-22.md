# Reviewed credits for specific source works

## Scope

The user approved applying original-book evidence to mistaken website author fields. This is explicit maintenance of reviewed source/work IDs, not automatic identity matching. Personal aliases, circle membership, store names and per-book role credits remain separate. Unproven identities stay unresolved and existing follows remain intact. Private names, IDs, book pages and account keys stay outside Git.

## Contract

An account/source may hold up to 500 `workCredits` rules alongside its existing author query profiles. Each rule contains a source-appropriate `workId`, `expectedAuthors` and `correctedAuthors`. Author lists contain 1–64 distinct bounded fields. Old documents remain readable without the optional field. Imports merge explicitly supplied work IDs and preserve unrelated profiles and rules.

A rule applies only to the exact source/work ID and an equal complete set of expected raw author fields, compared using NFKC, lowercase and collapsed whitespace. A changed website credit does not silently inherit an outdated correction. The corrected field does not become a global alias or alter other works by the wrongly named author. Query fingerprints exclude credit rules, so an offline correction does not force a remote rescan. Policy-revision guards still prevent stale scan commits.

An optional `expectedAuthorVariants` can list up to four additional, explicitly reviewed **complete** raw credit sets for the same work, because website listings and details sometimes use different spelling or join the collaborators into one field. These alternatives use the same bounded validation and complete-set guard; duplicate sets, partial credits and unreviewed later changes do not qualify. The list and detail projections share the rule. Legacy rules require no migration.

Raw source credits and saved query associations remain unchanged. Author-facing views use an immutable corrected projection, retain the original website credit in an explicit review note, and classify counts, filters and batch eligibility consistently. For a guarded correction, saved author views can also show the work under the currently followed corrected author even if its old query association contained only the wrong author. This does not certify or rewrite that author's query coverage. Source IDs, ownership, ZIPs and download history are unchanged.

## Validation

Synthetic cases cover legacy documents, account/source/work isolation, exact original-field guards, changed website credits, aliases and coauthors, import preservation, unchanged query baselines, saved-catalog reclassification and all author entry points. Formal tests/builds run in CI only. Read-only original-book evidence and bounded current source details are separate from these tests; no full-author scan or media download is needed.

Delivery requires a verified CI artifact, idle-app maintenance import with expected revisions and backups, preservation checks, and bounded native acceptance. Record completed evidence in the handoff. This is a Dev maintenance update, not an installer, release or production enablement.
