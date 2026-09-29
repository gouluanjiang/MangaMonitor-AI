# 1.0.0 release and all-author acceptance

The user accepted the 1.0.0-rc.1 candidate on 2026-09-29 and explicitly requested formal release closure, followed by agent-operated all-followed-author acceptance of the check-change summary. The ordering is release first, live acceptance second. Candidate acceptance supersedes its historical pending statements; it is user-reported acceptance, not a newly observed upgrade trace.

## Release scope

- Promote version to 1.0.0 and remove the candidate suffix from the main window.
- Preserve NSIS productName MangaMonitor Dev, application identifier, internal binary, private store and credential namespace. No data or installation-name migration.
- Reuse the candidate's strict license collection, single installer build, installed WebView and lifecycle verification. Stable versions must take the packaging path automatically.
- Commit the candidate's final evidence notes with this necessary release change. Formal checks/builds remain CI-only.
- Verify final artifacts and publish only code, installer, usage/release notes, notices and checksums in the existing repository. Merge the reviewed PR at its verified head and retain a version tag. No real book data is included.
- Deliver the verified build locally and check startup identity before live acceptance. Existing manga and stored registrations are preserved. The in-app updater remains deferred; cloud production remains disabled.

## Full-followed-author acceptance

After release closure, establish an idle baseline of the existing private profile and accepted author list. Run the application's normal all-author check, covering its current JM/Pica scopes with existing incremental-catalog behavior. This is not an instruction to force a complete historical re-fetch, edit queries/follows, or download any manga.

Check the terminal run identity, full selected/attempted/completed scope counts, newly discovered source IDs, retained historical IDs/first-discovery markers, and the displayed summary/filter. Verify refresh and restart retention, current library-status breakdown, and preservation of unrelated library/download/following/policy data. Source failures remain incomplete ranges; do not erase or silently count them as complete. One bounded retry of failed ranges may be used when appropriate and must be reported separately from the original all-author batch.

Private author names, work IDs, profile snapshots and detailed query results remain outside Git. Public reports contain aggregate evidence only. No authentication material is read or logged; if a source requires login, the user enters it in the application.

## Current status

Formal release closure, artifact verification, local installation and native startup identity checks are complete. [MangaMonitor v1.0.0](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.0) was published on 2026-09-29 at 04:05:43 UTC; Release ID `398813423` is public (`draft=false`). The user delegated real all-followed-author A6 acceptance to the agents. The full-list test has been executed and A6 functionality passed; complete source coverage still has two JM exceptions. The bounded retry, its zero-new/restore UI checks and independent retry audit are complete.

## Source, merge and CI evidence

- Reviewed source head: `adaf7083e7cdb8648c412380db86e09dc3a91f6f`.
- PR CI checkout/test merge: `7781b89d469c99dd64d2df81294eb85a90c8763d`.
- Actual [PR #19](https://github.com/gouluanjiang/MangaMonitor-AI/pull/19) merge and `v1.0.0` tag target: `141508afbe1282a52a211f7cbb3bc93237fe9409`.

The PR was merged with the expected-head guard. The publication gate required the actual merge commit to match the recorded PR merge result, be reachable from the default branch, and have exactly the same Git tree as the reviewed source head. Tag and all asset checks passed again after publication.

| CI stage | Frontend | Baseline | Windows desktop |
| --- | --- | --- | --- |
| Reviewed PR head | [36517510471](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36517510471), success | [36517510485](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36517510485), both jobs successful | [36517510481](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36517510481), attempt 2 successful |
| Automatic push CI after merge | [36519498724](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36519498724), success | [36519498726](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36519498726), both jobs successful | [36519498738](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36519498738), success |

The first PR Windows attempt hit a 10-second completion-helper timeout in `eligible_author_catalogs_are_not_cut_off_at_a_small_page_limit`; the saved evidence does not show a page/count assertion failure. Only that failed job was rerun, once, on the same source revision; it then passed. The other PR workflows were not rerun. All three merge-triggered workflows passed on their first attempts without a manual trigger or rerun. Their artifacts were not downloaded or substituted for the already verified release package. Formal suites and builds ran in CI only.

Windows validation includes license/NSIS checks, native IPC and Clippy, the single installer build, installed WebView startup/restart and reader-window lifecycle, fresh installation, same-version reinstallation, uninstallation preserving synthetic data, and installation again. Native lifecycle evidence uses synthetic library entries and does not prove real media reading or a physical close-button click. Historical `0.3.4` upgrade and interactive installer pages remain outside that CI evidence.

## Verified release artifacts

The published package retains the original bytes of PR Windows release artifact `11011539438`; the later main-branch artifact is not the delivered package. The installer was copied byte-for-byte under its public filename. The complete ZIP contains the verified installer, installed executable, resources and manifest. Every public asset name, byte count and remote SHA-256 digest matched the approved local list before publication and in the post-publication check.

| Public asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `MangaMonitor.Dev_1.0.0_x64-setup.exe` | 5100773 | `59f72055c3346bbb018ec033b547746a0dbc6f5b5cc0529e09484ce8ce6dbc02` |
| `MangaMonitor-1.0.0-windows-x64.zip` | 13007344 | `aff5bed3e1fcde2fce2935025417ffa50b7256cf7217179d0af9948149a7d457` |
| `manifest.json` | 3019 | `8ac5d7ea792cc2b5beb71963c9cbdac562a5fe4eaf848630836459e0ddc1f587` |
| `SHA256SUMS.txt` | 284 | `aa5d1df5053cbc918d532e307d4691db13c419a818b95376bfa1060786d571a0` |

The installed application SHA-256 is `b514acb71cec4cc3afee3f614ae9100955395f77cbf2ed95008efcfc5fb9bf40`. The manifest records the permitted NSIS bundle-marker change from the original build and the installed hash. The installer and executable remain unsigned. The application/product identity remains `MangaMonitor Dev`; the application version is `1.0.0`.

## Local installation and startup

Verified local delivery: `Documents/Codex/MangaMonitor-1.0.0-adaf708`. The current-user installer completed on 2026-09-29 at 03:59:18 UTC with exit code `0`. The private installation receipt records `passed=true`, registration version `1.0.0`, matching installed payload/resource hashes, and verified desktop/Start Menu shortcut targets. Protected private documents matched their saved hashes before and after installation and at the final installation check.

The installation script did not launch the application. A subsequent real native startup/diagnostic check observed `1.0.0` and source revision `adaf708`, with both JM and Pica connected and no active download processing. Private library/queue details remain in private evidence. The agent-operated A6 evidence below is recorded separately from installation/startup verification.

## A6 full-batch result and bounded retry

The normal all-followed incremental check ran on the actual installed formal version after release. All selected scopes were attempted; two JM ranges remained partial while Pica scopes completed. The terminal phase correctly remained `partial`. Real author identities, work IDs, private library sizes and exact discovery counts remain in private evidence.

The batch supplied real positive newly discovered author-work cases from both sources. The native summary/new-only view matched confirmed author results; raw keyword-only additions stayed in their separate view. Read-only baseline/terminal comparison passed all mechanical gates, with no old source IDs removed, no old first-discovery markers changed and critical private documents unchanged. Auxiliary attribution counts were corroborated by the actual UI, rather than treated as sufficient by themselves.

Native checks covered source/text/ownership/author-filter combinations, a correct filtered empty state, a historical-only author explanation, clearing a selected item when changing new-only mode, and a separate keyword-results view without batch downloading or author-count contamination. Refresh and queue-page navigation retained the summary. A real full Alt+F4 exit and restart retained both source connections, the library state and the original batch's positive summary/new subset. The saved terminal, final-refresh and after-restart snapshots passed the independent persistence comparison. No download was executed.

The original scan was not cancelled or restarted during the earlier black-screenshot/stale-accessibility interval. After it was already terminal, the UI temporarily retained BUSY/progress state. Once window visibility returned, one manual refresh successfully loaded the terminal result. This validates manual recovery and subsequent persistence, not an uninterrupted automatic terminal-refresh path.

One separate unfinished-only retry attempted only the two partial JM scopes, completed neither and found no new records. One range reached pagination end with a source record still requiring review; the other reported changed pagination and unconfirmed completeness. Their root causes are unclassified, and the two ranges are not accepted as complete. The current latest summary is this two-scope retry with no new results; earlier discoveries are retained in the historical results. The retry's new-only view correctly showed empty results with a limited-read-scope explanation; disabling it restored the retained list. All source/author/text filters were cleared, the missing-items tab was restored, new-only was turned off and the application remained idle.

Independent retry comparison passed: only the previously incomplete scope summaries changed; all other scope summaries, record identities, first-discovery/author flags and critical documents were preserved. Existing JM record metadata summaries changed during re-observation, so an all-metadata-unchanged claim would be incorrect. The audit verifies the exact two-scope restriction through saved range summaries; compact snapshots do not identify each changed internal field or independently adjudicate each record's author attribution.

The final evidence edits are preserved locally and uncommitted in the working and canonical checkouts. The private batch `Documents/Codex/MangaMonitor-v1-release-20260929` contains publication/installation receipts, CI evidence, asset review, full-run/persistence/retry comparisons and final native UI evidence. Public evidence contains only aggregate conclusions, not account identities, author names, work IDs, private library sizes or exact discovery counts. Cloud production remains disabled (`production_enabled=false`); no cloud monitoring or in-app updater is enabled by this release.
