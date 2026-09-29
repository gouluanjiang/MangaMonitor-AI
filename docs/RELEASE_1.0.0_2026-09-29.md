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

Release implementation and verification are in progress. No stable artifact, merge, public Release or all-author acceptance is claimed yet. Final evidence is appended after the corresponding operation succeeds.
