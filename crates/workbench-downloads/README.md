# Desktop manual JM downloads

This crate implements one explicitly confirmed JM task at a time. It projects
only its private task and current approval into the existing A6 staging chain;
it never creates matcher evidence, updates production inventory, or changes the
phone library. Plans stay in memory. Confirmed tasks use the fixed private
`downloads.json` document and its existing revision/CAS storage boundary.

`prepare`, `confirm`, `read`, and `control` serve the desktop IPC boundary.
`run` must be explicitly started with a current native session lease and should
run on a dedicated blocking worker with an async runtime. The lease and current
library identity/generation are checked at authorization points. Reading a
reopened queue projects unfinished tasks as paused without starting network or
media writes. A live worker remains visible and must finish unwinding before
resume can start another worker. The file lock spans the worker and registration
handoff, while the document lock is held only for short storage transactions.

Checkpoint intents bind the descriptor-set hash and each processed page hash
before writing. A complete pending page can be reused after a crash without a
new media request. A partial pending page needs an explicitly resumed request
whose processed bytes match that recorded hash; only an identical prefix is
continued. Unknown or changed bytes are preserved and refused. Progress commits
after a just-completed authorized write preserve a concurrent pause revision.

Final output contains compatible JM metadata, four-digit page filenames, chapter
metadata, and a JPEG thumbnail derived from the first verified page. Metadata
fields unavailable from the workbench detail response have neutral compatibility
values explicitly listed in `mangaMonitor.compatibilityPlaceholders`. Images
retain the audited processed WEBP/GIF format. This batch limits actual pages to
9,999 because the existing library scanner includes the root cover in its 10,000
image limit; materialization supports up to 200 chapters. The cover uses the core
20,000-pixel dimension and 256 MiB decode allocation limits.

The permanent `_mangamonitor-layout.json` intent is synced before media output.
The final `_mangamonitor.json` lists every output file and exact hash. Ordinary
library scanning must reject a managed layout without its matching complete
manifest. A verified final tree yields `AwaitingIndexReceipt`; it is not reported
as downloaded until the native library service registers the exact ID/page count
and `mark_indexed` durably records that entry. Registration failure reuses the
complete output on an explicit retry. All finish paths release the worker even
if the finish checkpoint fails.

After durable registration, the desktop layer attempts to remove only this
completed task's exact hashed files under its private staging command directory,
then removes empty directories. It never removes final work files or failed or
paused staging. Unknown/changed temporary content is retained; cleanup failure
does not undo the completed download. The historical A6 CLI deletion flags and
materialization policy are unchanged.

Tests use generated GIFs, temporary directories, injected A6 fetchers, and the
real local registration API. They make no real source requests. Formal execution
belongs to CI; real user-selected acceptance is a separate explicit action.
