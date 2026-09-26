# Download queue presentation

The user authorized the next scheduled batch after the cover optimizations. This implements the agreed three queue tabs and suggestions 2/3/4. The top source/ID entry form stays visible; its proposed collapse was explicitly rejected. Language badges, A6 summaries and the later backlog are separate work. The prior cover delivery remains user-acceptance-pending, not silently certified by this continuation request.

## Behavior

- Default to 下载中. Show only 下载中, 下载失败／需要处理 and 已下载, each with a count for the current title/ID and source filters. Success moves the task to 已下载 without forcing a tab change. A separate labeled summary reports the entire current queue regardless of those filters.
- Active includes queued, downloading, verifying, saving and paused. Attention includes errors and completed records whose files are missing, changed or unavailable. Downloaded requires completion and present files. Full image progress alone is not completion. Invalid completed-file states still fail the existing protocol validation and retain the previous visible snapshot.
- Downloaded is also the successful history view, sorted newest completion first with stable ID ties. Successful rows show title, source/ID, completion time and file-location action. An accessible details disclosure contains path, image/byte totals and the existing individual history removal. The always-full success progress bar is omitted. Unknown/unrepresentable dates are displayed as unknown.
- The source download service writes `updatedAt` in Unix milliseconds when successful library registration becomes Downloaded. Presence checks and relocated-path projection do not rewrite it; it is therefore used for completed-record date/order without a new stored schema. Do not apply this interpretation to unfinished tasks.
- Keep the existing filtered history cleanup (at most 50 records per confirmation) under 已下载. Completed records with file problems retain individual cleanup under attention. Both use the existing confirmation and revision-bound command; neither deletes files or removes library identity.
- Account problems point to account settings, directory/index problems to library settings, and file-state problems offer read-only rechecking. Missing files alone retain the existing reprepare-and-confirm route. Unavailable/changed files do not gain an overwrite or download action. Pause/resume/retry still derive exclusively from the native allowed actions and the task's source session.
- The status banner prefers actual downloading/verifying/saving work before queued entries. Active, waiting and paused counts remain distinct.

## Recent batch progress

Only after a successful, identity-validated single or batch confirmation does the controller retain the exact admitted task IDs in memory. The UI labels this as the most recently added batch during this application run, not the whole history. Counts are unaffected by search/source filters. Historical completion IDs remain counted after the user removes those history records; file problems are separately counted as attention. A later valid confirmation replaces the displayed group; failure/cancellation does not. A reopened application shows current queue statistics without inventing a past batch. This is display state only, not a new persistent queue/executor contract.

## Review and verification

`DOWNLOAD_EXECUTOR_THAW_GATE.md` was reviewed. No backend protocol, task/file authority, download concurrency, media execution, completion/registration transition or persistent schema changes are made. Native controls keep their established boundaries. Demo-only queue presentation follows the same three-tab shape without introducing real execution.

Formal checks/builds run only in CI. Meaningful regressions cover exhaustive valid task grouping, millisecond completion dates and stable order, retained history identity/confirmation, filtered counts versus global counts, actual batch IDs and cleanup/restart/failed-confirmation behavior, automatic success without tab navigation, compact/expanded rows, source-account action boundaries, and rejected invalid snapshots preserving earlier data. Synthetic screenshots cover active, attention and downloaded compact/expanded states. Desktop checks continue to exercise the unchanged native permissions and actual isolated WebView startup/restart. The completed cover round's one-time profiling step is removed from routine CI; the diagnostic itself remains available when warranted.

CI results, revision and verified delivery are pending at this implementation checkpoint. No user app control, live source traffic or manga download is performed by this batch. Final user experience acceptance remains separate from synthetic and CI evidence. Version remains Dev 0.3.4, the PR stays draft/unmerged, and production remains disabled.
