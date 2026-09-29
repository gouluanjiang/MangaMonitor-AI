# Desktop manual JM and Pica downloads

This crate implements explicitly confirmed JM/Pica queues, executing one work at a time. It projects
only its private task and current approval into the existing A6 staging chain;
it never creates matcher evidence, updates production inventory, or changes the
phone library. Plans stay in memory. Confirmed tasks use the fixed private
`downloads.json` document and its existing revision/CAS storage boundary.

`prepare_for_source`, `confirm`, `confirm_many`, `read`, and `control` serve the desktop IPC boundary.
Up to 50 reviewed plans can be confirmed with one durable queue write. Each immutable
plan retains its source/root/approval binding while unrelated image progress may
advance the document revision; confirmation rechecks current conflicts under the
service lock. Waiting tasks admitted in this process stay queued. A native FIFO
driver owns the entire download, materialize, and PC-registration lifecycle before
taking its next item, so adding work never creates a second media worker. Ordinary
failures or per-task pauses leave later admitted work able to proceed.
The original `prepare` and `run` remain JM-compatible entry points. Old ledger
records default to JM, omit that default source when serialized, and retain both
historical JM binding profiles without modification. `DownloadMetadata` aliases
the original `JmDownloadMetadata` display-field type; validation uses the selected
source's canonical ID. Pica records require `jpeg_output=false` and have a distinct
approval-binding namespace. Native control must resolve `task_source` using the
same expected control revision before selecting its account session.

`run_with_token` must be explicitly started with a current native session lease and should
run on a dedicated blocking worker with an async runtime. The lease and current
library identity/generation are checked at authorization points. Reading a
reopened queue projects unfinished tasks as paused without starting network or
media writes. `resume_many` explicitly admits up to 50 exact source/revision-bound
tasks under a current native session; `pause_all` stops waiting/media work while
letting an already-started final save finish. A paused task's old worker must unwind
before that same task resumes; other tasks may join the waiting queue. The file lock spans the worker and registration
handoff, while the document lock is held only for short storage transactions.
Pica requires a current, borrowed session token; JM refuses a Pica token. Tokens
are never persisted in the task, metadata, receipt, logs, or media-byte client.
The existing core checks complete chapter and image pagination before staging.

Checkpoint intents bind the descriptor-set hash and each processed page hash
before writing. A complete pending page can be reused after a crash without a
new media request. A partial pending page needs an explicitly resumed request
whose processed bytes match that recorded hash; only an identical prefix is
continued. Unknown or changed bytes are preserved and refused. Progress commits
after a just-completed authorized write preserve a concurrent pause revision.

Final output contains source-compatible metadata, chapter metadata, and a JPEG
thumbnail derived from the first verified page. JM keeps four-digit page names;
new JM tasks emit JPG for static restored images while legacy WEBP and GIF retain
their approved policy. Pica uses `[Pica{id}] title` work directories, chapter
directories with a numeric order and canonical chapter ID, and three-digit page
names (expanding naturally above 999). Its JPG/JPEG/PNG/WEBP/GIF pages retain the
verified source bytes and extension. The Pica root thumbnail is actually
`cover.jpg`, matching the local `thumb.path` metadata field.

Metadata
fields unavailable from the workbench detail response have neutral compatibility
values explicitly listed in `mangaMonitor.compatibilityPlaceholders`. Images
never take filenames or local read authority from source URLs. Pica metadata
contains all required fields of the pinned upstream `Comic` shape. Unknown
creator, timestamps, engagement counts, and source flags use marked compatibility
defaults; chapter titles use their known ID rather than an invented title.
Only verified page/chapter counts and local completion are reported as observed.
The page limit remains 9,999 because the existing library scanner includes the root cover in its 10,000
image limit; materialization supports up to 200 chapters. The cover uses the core
20,000-pixel dimension and 256 MiB decode allocation limits.

The permanent `_mangamonitor-layout.json` intent is synced before media output.
The final `_mangamonitor.json` lists every output file and exact hash. Ordinary
library scanning must reject a managed layout without its matching complete
manifest. A verified final tree yields `AwaitingIndexReceipt`; it is not reported
as downloaded until the native library service registers the exact ID/page count
and source, and `mark_indexed` durably records that entry. The receipt carries the
source and rejects a cross-source registration. Registration failure reuses the
complete output on an explicit retry. All finish paths release the worker even
if the finish checkpoint fails.

After durable registration, the desktop layer attempts to remove only this
completed task's exact hashed files under its private staging command directory,
then removes empty directories. It never removes final work files or failed or
paused staging. Unknown/changed temporary content is retained; cleanup failure
does not undo the completed download. The historical A6 CLI deletion flags and
materialization policy are unchanged.

Historical completion and current local file presence remain separate. Polling
can reuse the process-local presence cache; prepare and explicit confirmation
always recheck the source-specific target. Only positively missing index rows for
that source are removed with CAS before a newly approved task is recorded. Other
sources, manual associations, unavailable roots, and existing media remain intact.

Visible full history is bounded at 500 tasks, with the existing 32 MiB private
document ceiling retained. `remove_history` accepts only explicitly selected,
revision-current completed records and never writes media, PC index, or phone
state. It retains compact source/root/original-destination/entry evidence (bounded
at 20,000 identities) so clearing history cannot remove same-source duplicate or
manual association protection. Full image manifests and checkpoints leave the
visible ledger with the removed history entry. Empty evidence is omitted when
serializing older documents; existing JM approval bindings remain unchanged.

Tests use generated images, complete synthetic Pica pagination, temporary
directories, injected A6 fetchers, and the
real local registration API. They make no real source requests. Formal execution
belongs to CI; real user-selected acceptance is a separate explicit action.
