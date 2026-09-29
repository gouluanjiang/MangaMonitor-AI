# Current development handoff

Updated 2026-09-29. This is the continuation entry point. Read [project status](PROJECT_STATUS.md) for current product scope and [the documentation index](README.md) for supporting contracts. Historical plans do not restore cancelled features or grant execution authority.

## Delivered version

The agreed local V1 is complete and [1.0.1 is formally released](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.1). User experience acceptance, project-review corrections, release preparation, agent-operated all-followed-author acceptance, the bounded JM pagination correction, publication and local upgrade are complete. User acceptance and agent verification remain separate evidence.

- Reviewed application source: `6446e1ebdaac7f3275ef8492cbe3c36aa2fedb04`.
- PR test-merge revision: `dbe5c140ffdb729e87e4bb4dfa795d76db32225f`.
- Actual [PR #20](https://github.com/gouluanjiang/MangaMonitor-AI/pull/20) merge and `v1.0.1` tag: `641d2edb6f9e6e362259ebe48da7e4cade346a19`.
- Final PR and automatically triggered main CI passed. The reviewed artifact, four public assets and installed resources were verified. Version `1.0.1 / 6446e1e`, normal startup, complete exit/restart, session restoration and preserved library/download/author records passed native checks.
- Original v1.0.0 release assets remain intact. Later documentation commits do not replace the application revision or the reviewed release payload.

See the [1.0.1 release record](RELEASE_1.0.1_2026-09-29.md) for CI links, hashes, publication and installation evidence. The [1.0.0 release record](RELEASE_1.0.0_2026-09-29.md) and [JM boundary contract](JM_SEARCH_BOUNDARY_2026-09-29.md) retain the earlier all-author and correction boundaries.

## Known boundary and future work

One source record still lacks usable metadata. Its scope remains explicitly partial while valid results remain available; this is not an unfinished product feature or a promise to recover missing website data. Source availability, unencountered failures and future website changes are not guaranteed by the completed acceptance.

No agreed V1 development item remains open. Continue with actual maintenance issues or newly approved requirements. The in-app updater is for later discussion; cloud monitoring stays disabled and `production_enabled=false` is unchanged. Do not restart cancelled matching, phone-list, classification-booklist or other historical proposals. The reader and integrated UI have already shipped.

## Approved experience batch in progress

The user approved the [browsing and download experience plan](BROWSING_DOWNLOAD_EXPERIENCE_2026-09-29.md). Implementation is on `codex/browsing-download-experience` from `01274ab1`; CI and user acceptance are pending. All manga browsing views preserve position for this run, including details and downloaded lists. This is a new maintenance batch, not part of the already accepted 1.0.1 payload.

For this batch, the user explicitly removed the second confirmation for both single and batch downloads. Clicking download authorizes those selected works; source/session/root/revision checks, no-overwrite behavior, and download history remain. This current instruction supersedes historical requirements to show another download confirmation. No release or replacement of installed 1.0.1 has occurred.

## Continuing development

- Use the canonical `MangaMonitor-AI` checkout and refresh current `main`, status and applicable instructions before editing. Retired dated worktrees and historical EXE paths are not continuation targets. The currently installed app remains the reviewed formal version.
- Read [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md). Formal suites/builds run in CI; do not duplicate them locally or trigger full source scans for documentation maintenance.
- Preserve local data identity, verified ownership, source uncertainty and file-operation authority; apply the current download-click authorization described above. Apply the [download thaw gate](DOWNLOAD_EXECUTOR_THAW_GATE.md) when changing download behavior or authority.
- Private account, book, library, installation and cleanup evidence stays outside Git. Local cleanup receipts are not product payloads or public documentation.
- Repository organization changes documentation and navigation only. Do not rebuild, republish or replace 1.0.1 assets for these text changes.

## Historical evidence

- [Complete handoff through 2026-09-29](archive/DEVELOPMENT_HANDOFF_2026-09-29.md), including the 2026-09-15–29 development and acceptance trail.
- [Earlier handoff through 2026-09-15](archive/DEVELOPMENT_HANDOFF_ARCHIVE_2026-09-15.md).
- [Historical document index](archive/README.md), including the original cloud, assistant and matcher plans.

Historical draft/unmerged, pending-acceptance, temporary-path and waiting-for-push statements describe their original milestones and are superseded by this current entry.
