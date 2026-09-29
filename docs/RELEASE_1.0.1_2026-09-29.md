# MangaMonitor 1.0.1 release closure

## Scope

The user authorized merging the final JM pagination correction and publishing 1.0.1 to close V1 delivery. This patch includes the [reviewed JM search boundary compatibility](JM_SEARCH_BOUNDARY_2026-09-29.md) and the versioned release resources. It preserves application identifiers, data and credential namespaces, the existing installation name and the previously accepted product scope. No source matching, author inference, download authority, data migration or automatic monitoring behavior is introduced.

The existing v1.0.0 tag and assets remain intact. PR #20 is the delivery PR. A new v1.0.1 release must point to the actual merge commit and carry the reviewed final-version artifact; do not relabel the previous 1.0.0 executable.

## Existing acceptance evidence

The V1 all-followed-author batch and its positive-new, filtering and history-preservation checks have completed. Its original unresolved JM boundary range was subsequently fixed and verified through the native application in a bounded unfinished-only retry. The corrected range read every page and reconciled the unique source total. One separate malformed source record still lacks usable metadata; valid records remain available and that range is honestly partial. This is a source limitation, not a claim that all source metadata can be recovered.

The correction's CI, independent review, native zero-new filtering/refresh and before/after preservation checks passed. The all-author run is not repeated merely for a patch version change. The original long full-list run needed a manual terminal refresh after window visibility returned; the later bounded native retry reached idle automatically. Neither result is a proof of every future website or window-visibility condition.

## Final-version release checks

Before publication, verify all required final-head CI checks and the complete Windows artifact: independently obtained SHA-256, ZIP CRC, exact file manifest and hashes, embedded version/revision, x64 application, bundled notices/license inventory and installed-application lifecycle evidence. Use an expected-head merge guard, then check the actual merge tree against the reviewed source tree. Preserve source revision, CI test-merge revision and actual merge/tag revision as separate facts.

The release contains exactly four public files: `MangaMonitor.Dev_1.0.1_x64-setup.exe`, `MangaMonitor-1.0.1-windows-x64.zip`, `manifest.json` and `SHA256SUMS.txt`. The ZIP retains the verified CI archive bytes. The setup and manifest are exact verified payload copies. Upload to a draft release, verify names/sizes/digests and tag identity, then publish and verify again. No private evidence, account data, book list or manga is included.

For local delivery, observe the existing application idle and close its main/reader windows normally. Preserve the prior installed payload and shortcut state before upgrading the existing current-user installation. Check installed resources, version/revision, registration, shortcuts and protected data. Observe native startup, the existing library and download status and retained author results. Session restoration is verified without reading credentials; a transient source error is not treated as lost login or permission to change credentials.

## Status and delivery boundary

Preparation is in progress. The final-version source commit, CI, merge, release and local upgrade are not yet claimed in this checked-in preparation record. Final receipts will be appended after completion and retained locally until the next necessary push; do not create a release-build loop for evidence-only text changes.

The package remains unsigned and does not include the deferred in-app updater. Existing local and online readers, user-driven source queries and local downloads remain the delivered product; cloud monitoring stays disabled. Agent checks and previous user acceptance remain distinct from any new user report about the installed patch.
