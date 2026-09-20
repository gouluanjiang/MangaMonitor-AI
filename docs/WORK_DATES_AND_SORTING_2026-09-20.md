# Work dates and sorting

## Accepted scope

The user approved this implementation after choosing the date semantics and accepting unknown historical dates. Search results and author updates display the website's work-update date. A local library work displays its downloaded version's source-update date and its original library-admission date. A later website update must not change the version date of an existing local package.

Search and author-update lists default to most recently updated first, with the reverse option. The library defaults to most recently admitted first, retaining the reverse option and adding both directions for version-update date. Unknown values sort after known dates in both directions. Sort preferences are independent per page, compose with ownership/text filters and never reorder pagination checkpoints or the source request traversal. Incomplete scopes remain explicitly incomplete; ordering then covers only the loaded results. Favorites and ranking source order retain their existing semantics.

## Date provenance and compatibility

`sourceUpdatedAt` is optional source metadata. `versionUpdatedAt` is a separate optional snapshot in the download metadata and library record. Both support a precise RFC3339 instant normalized to UTC or a source-provided calendar date without invented time precision. `addedAt` retains the existing first-successful-admission meaning; it does not claim to recover a historical download date.

Do not use observation time, directory scan time, task update time, file modification time, creation/publication dates or the current website date to fill a missing historical version-update date. Empty values, malformed dates and epoch placeholders remain unknown. New optional fields are omitted when absent so older cached records and queued tasks load without adding null fields to their serialized approval metadata. A rollback to an older strict-schema executable requires the preserved compatible profile backup.

The source field evidence is the existing pinned upstream source: JM `search_resp.rs` exposes numeric `update_at`; Pica `search_resp_data.rs` and `get_comic_resp_data.rs` expose `updated_at`. JM detail's `addtime` is not evidence of a work-update date. Pica's original downloader serializes the captured `updated_at` as `updatedAt` in its comic metadata. The workbench's historical generated `updatedAt=1970-01-01T00:00:00Z` and empty JM `addtime` are compatibility placeholders and must not be promoted to dates.

No per-work detail fan-out or automatic full-author replay is introduced to populate dates. Normal source responses fill available fields; older saved source results may remain unknown until their normal refresh. Historical local version dates may be recovered only from trustworthy metadata within the local package. No bulk ZIP rewrite, repackaging, identity matching or current-source lookup is authorized by this feature.

When a detail response omits its update date, the account service can retain a value already read for that exact work in the current authenticated source/session. It does not import an old saved discovery date as proof of the current downloaded version. Consequently a direct JM download, or one selected after restart/cache eviction, can have an unknown version date when the detail endpoint omits `update_at`. Pica detail normally carries its own update date. This is reported as a source-metadata limitation, not silently filled with `addtime` or a historical current-source guess.

## Download review

This change affects the metadata carried through the existing prepared task, generated ZIP and successful library registration. Existing single/bulk source identities, approval hashes/revisions, image enumeration, staging, resume, manifest checks, add-only promotion and completion gates remain in force. Missing dates never fail an otherwise valid work or authorize a download. Existing metadata with no date must retain its prior serialization and generated-package compatibility. There is no changed upstream protocol pin, media request, transform, host, retry or execution authority, so the prior executor reviews remain applicable; regressions target the affected metadata and registration path.

## Required evidence

Formal suites and builds run only in CI. Regressions cover explicit update fields and rejected creation/placeholder values; saved discovery/cache round trips; a download date snapshot through ZIP metadata and library indexing; stable admission/version dates on rescans; legacy task compatibility; ascending/descending unknown-last ordering; filtering, partial coverage and remembered independent choices. Visual review uses the accepted library and discovery layouts with added subordinate date text.

Existing private profile documents and the two prior final evidence reports were backed up outside Git before implementation. Real titles, identifiers, credentials and library contents stay private. No live media download or library replacement is needed to validate this feature. Engineering checks, native verification and user acceptance are reported separately.

Implementation is in progress. Version remains 0.3.4; no installer, release, merge or production enablement is included.
