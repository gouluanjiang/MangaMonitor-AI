# Assistant A6.5 — pinned upstream completion audit

This audit records why MangaMonitor-AI cannot treat the existing GUI/Tauri download commands as a trusted completion signal.

## Pinned upstreams

- JM: `lanyeeee/jmcomic-downloader@f0cdd724af6892002f2fb7be883b88832cebe7e9`
- Pica: `lanyeeee/picacomic-downloader@77c8b62ede42b3afc074506d092313816af8092d`

A6.5 rejects completion evidence claiming a different upstream commit.

## JM findings

Relevant upstream files:

- `src-tauri/src/commands.rs`
- `src-tauri/src/downloader/download_manager.rs`
- `src-tauri/src/downloader/download_task.rs`

The Tauri `create_download_tasks` command delegates to `DownloadManager::create_download_tasks` and returns after task creation. `DownloadTask::new` starts asynchronous processing rather than making the command wait for the chapter to finish. Therefore a successful command return is only spawn/create evidence, not completion evidence.

Inside the chapter worker there is a stronger internal completion point: image jobs are placed in a `JoinSet`, all are joined, downloaded image count is compared with total image count, the temporary directory is renamed only after the image count matches, and only later does the task transition to `Completed`.

A6.5 therefore requires every expected JM chapter to be scheduled, joined, terminal `COMPLETED`, and have exact nonzero image accounting. A create/spawn-only transcript is rejected.

## Pica findings

Relevant upstream files:

- `src-tauri/src/commands.rs`
- `src-tauri/src/download_manager.rs`
- `src-tauri/src/utils.rs`
- `src-tauri/src/pica_client.rs`

`DownloadManager::create_download_task` creates a task, spawns `task.process()`, inserts it in the manager, and returns. As with JM, command success is not chapter completion.

The per-chapter image URL path in `download_manager.rs` is stricter: page 1 establishes the total page count; later image-page requests are joined and an error from any later page is returned. Image download jobs are also joined and image count is checked before the task reaches `Completed`.

However, the whole-comic chapter aggregation helper in `utils.rs::get_comic` is unsafe as a completeness certificate. It fetches chapter page 1, spawns requests for pages 2..N, and when a later chapter-page request fails the spawned task logs the error and simply returns. `join_all()` then finishes and the helper constructs the comic from whatever chapter pages succeeded. Thus a later failed chapter page can become a partial chapter list without the caller receiving a failure.

A6.5 therefore requires two independent Pica pagination proofs:

1. complete whole-work chapter pagination; and
2. complete image pagination for every expected chapter.

For each pagination proof, `successful_pages` must be exactly `1..=total_pages` and `failed_pages` must be empty. A first-page-only or later-page-failure transcript is rejected.

## Deliberate A6.5 boundary

A6.5 is an offline proof contract/normalizer only. It does not copy the upstream downloader implementation, call either source, use Pica credentials, download an image, write staging files, mutate inventory, complete a pending task, promote/replace files, or delete anything.

A later phase may implement real source bridges. Those bridges must generate the A6.5 proof from observed execution and must not fabricate the completeness flags from a GUI command return value.
