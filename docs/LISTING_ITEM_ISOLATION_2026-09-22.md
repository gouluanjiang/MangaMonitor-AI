# Isolate malformed source-list records

## Scope

The user approved continuing past malformed individual records so normal comics from the same and later pages remain usable. Apply this to JM and Pica list responses and their search, author-search, saved author-update, favorites and ranking consumers. Preserve source author evidence, existing ownership rules and current visual layout. This change does not introduce title-based matching, author inference, automatic background checking, media execution or extra detail requests.

## Contract

Source pages return normal `items` and a separate `issues` list. Each issue contains its source page, one-based raw row index, an optional validated work ID and a fixed reason code. Do not persist raw malformed titles, URLs, payloads or credentials. A bad entry has no new `SourceWork`, cover authority or download/favorite action. Detail and download preparation remain strict. The previous JM blank-title placeholder case now uses the same explicit issue path; already saved useful metadata and old records remain preserved.

The number of source rows read is `items.length + issues.length`. Use this for pagination progress, source totals and limits. A page containing only issues is still a nonempty source page. Continue to subsequent pages while validated pagination permits it. Bad JSON/envelopes, contradictory pagination, conflicting/repeated identities and unreadable responses remain whole-page errors; their scope must remain incomplete. Do not turn network/auth errors into missing books.

Reading every page and recovering every work's metadata are separate outcomes. Author searches and saved author updates with issues must not claim that all author works were verified or all are owned. Saved author ranges retain the issue count, up to 20 position samples and whether pagination finished; completed pagination with issues is `partial` with `SOURCE_ITEMS_PARTIAL`. Such a range does not establish an incremental baseline and remains eligible for an explicit unfinished-only retry. A later successful full check can resolve the issue state.

Old documents without issue fields load without migration or speculative source requests. Collection caches retain exact issues and normal-item page boundaries; raw counts include issues. An issue-only page may repeat the normal-item boundary. A cache with issues must have boundaries sufficient to validate each issue's location. Legacy caches without those boundaries rebuild through ordinary user-initiated source reading, never a silent background scan. A cache's complete flag describes traversal, and UI metadata-completeness messages also inspect issues.

## Presentation

Show a compact summary and an expandable list of source/page/position/validated-ID or unknown-ID/reason. Normal results remain selectable under the established author and ownership filters; issue entries have no mutation or batch-download controls. Preserve prior readable results on a later source failure and show both the failure and recorded issue information. Saved results and keyword-only results retain their existing distinct meanings.

## Verification and delivery

Implementation and independent static review are complete; formal regression tests and builds are pending CI. Added coverage includes malformed rows between normal rows in both sources, later-page continuation, issue-only pages, exact source accounting, strict envelope/detail limits, cache/restart persistence, retry resolution, unchanged historical records and absence of false complete/all-owned claims. The review also closed a control-only title path which could otherwise become blank during storage projection and reject its entire page. UI evidence will be compared with accepted layout references.

Before-state receipts and captures remain private under `Documents/Codex/MangaMonitor-item-isolation-20260922`. The existing catalog and previous final report edits were backed up before implementation. Real-source verification must stay bounded; no all-followed replay, manga download or private data publication is needed. PR #19 remains draft, version 0.3.4 and production disabled until their separate decisions change.

Initial head `674dfeea754142a971a56b348fa2ad03afc41f93` passed frontend `35690386551` (169 logic / 149 Chromium), baseline `35690386545` (70 source tests), and desktop `35690386618` (86 account / 38 native IPC, Clippy, build and actual Windows WebView startup/restart). The artifact and four new synthetic screenshots were digest/CRC verified. Visual review found a contradictory date-sorting scope sentence on the author page; its narrow correction and existing UI regression assertion are included before final delivery. Initial binaries were not launched or assigned to shortcuts. Final-revision CI and native acceptance remain pending.

Canonical report edits were byte-verified against their private backups and retained in stash `6b983a57e7f19015358bf651c1af38a34c161839` before canonical was fast-forwarded. Preserve this and earlier stashes/deliveries. No formal suite was duplicated locally.
