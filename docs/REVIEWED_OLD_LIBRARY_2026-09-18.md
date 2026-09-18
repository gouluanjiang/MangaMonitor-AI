# Explicit registration of the reviewed old library

The user resumed post-V1 project 3, the old-library review, and asked to complete it. They also explicitly accepted the previously inspected set of 154 incomplete ZIPs as normal library works. That decision removes a file-completeness prerequisite for those exact files; it does not invent source identities or authorize deleting/replacing media.

The application previously counted only successful same-source download records and current files. Confirmed old works therefore remained unowned in favorites, author search and author updates. This change adds separately recorded old-library review evidence to that shared inventory projection. No successful download records are fabricated and no runtime title, author, translation or cross-source inference is restored.

## Import and persistence

- A local maintenance helper reads an explicitly reviewed JSON manifest containing the exact configured root, relative ZIP paths, byte sizes, SHA-256 hashes and individually reviewed source/work references. Real manifests and titles stay outside Git.
- Preview checks the scope, index and file identities and reports counts plus the manifest digest and current library revision. Its result explicitly says file hashes have not yet been verified.
- Applying requires that exact manifest digest and expected revision. The helper verifies every file hash and current identity, rejects conflicting/duplicate references and writes the reviewed evidence in one revision-checked private library transaction. It never writes manga bytes, changes legacy metadata links or updates the download queue.
- Registration accepts the reviewed ZIP as the user owns it; it does not assert that every chapter/page is complete. The raw private completeness findings remain available separately.
- Ordinary inventory reads verify the registered file identity and preserve source namespaces. Missing or replaced files stop counting as owned. Same-root rescans retain explicit registrations; changing roots does not inherit them. Existing verified path-migration evidence can locate an unchanged registered ZIP.

The helper is intentionally separate from ordinary application flows. It has no renderer IPC endpoint and refuses its real-store apply mode in CI. The application gains the owned status from the imported evidence, not a new interactive guessing/association workflow.

## Local usage

Run the exact new helper and application after CI passes. Back up the current private library document and preserve its revision. Close the previous application before changing its private metadata, then open the newly verified executable afterwards; older builds do not understand the added registration field.

```text
mangamonitor-library-review --app-data APP_DATA_ROOT --manifest REVIEWED_MANIFEST
mangamonitor-library-review --app-data APP_DATA_ROOT --manifest REVIEWED_MANIFEST --apply --expected-revision PREVIEW_REVISION --manifest-sha256 PREVIEW_DIGEST
```

Only the manifest's reviewed entries are imported. Ambiguous candidates are kept in a separate private review list and do not become owned just because they have a similar title. The result is not a claim that every old library work has a known source identifier.

## Verification status

Implementation and synthetic regressions are prepared. Checks and builds run in CI under the repository resource rules; live import and native acceptance remain pending until the exact executable and helper pass validation. New tests cover preview without mutation, separate download history, an accepted partial ZIP, atomic failure, scope/digest/revision checks, conflicting references, repeat import, rescans/root changes and removal/replacement. No production enablement, installer, release or new manga download is included.
