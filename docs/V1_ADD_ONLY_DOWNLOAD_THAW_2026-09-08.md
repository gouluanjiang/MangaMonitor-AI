# V1 add-only real-download thaw review — 2026-09-08

Status: **REPOSITORY THAW GATE ACCEPTED FOR LOCAL ADD-ONLY STAGING; REAL LOCAL STAGING ACCEPTANCE STILL REQUIRED**

This document records the mandatory thaw review performed after the user explicitly authorized: **“解冻 V1 add-only 本地下载路径，执行 thaw gate”**.

The authorization is intentionally narrow. It permits the existing guarded local executor to perform real JM/Pica retrieval only for a currently approved new-work `download` task and only into a fresh command-owned isolated staging tree. It does not authorize production, `upgrade`, replacement, overwrite, library materialization, inventory mutation, task completion, or deletion.

## Baseline reviewed

- MangaMonitor-AI main at review start: `ec3a8cb4a609e45f65f1d4c07094ece9de4fdf30`
- Cloud production readiness: accepted; `production_enabled=false`
- Required repository documents read in this session:
  - `AGENTS.md`
  - `docs/DOWNLOAD_EXECUTOR_THAW_GATE.md`
  - `docs/upstream-source-reference.md`
  - `ASSISTANT_A6_15_RESULTS.md`
  - `ASSISTANT_V1_6_RESULTS.md`
- Current local execution path reviewed:
  - `local_execution_orchestrator.rs`
  - `source_preflight_authorization.rs`
  - `source_completion.rs`
  - `live_media_descriptors.rs`
  - `live_media_fetch.rs`
  - `live_media_transport.rs`
  - `isolated_staging_execution.rs`
  - JM/Pica media-descriptor adapters
  - `mangamonitor-local-executor.rs`

## Upstream pin refresh

All three pinned references were reopened from GitHub during this thaw session. Their default-branch heads still equal the recorded pins, so no pin update is required.

| Upstream | Recorded pin | Default branch head at thaw review | Decision |
| --- | --- | --- | --- |
| `hect0x7/JMComic-Crawler-Python` | `9fddb0494caf0cdc812ac6cbfc1c62f4f845b058` | same | keep pin |
| `lanyeeee/jmcomic-downloader` | `f0cdd724af6892002f2fb7be883b88832cebe7e9` | same | keep pin |
| `lanyeeee/picacomic-downloader` | `77c8b62ede42b3afc074506d092313816af8092d` | same | keep pin |

### JM findings

The pinned Python downloader treats a run as complete only when no failures were recorded, every expected photo completed, and every expected image completed; partial downloads raise a failure rather than becoming success.

The pinned Rust downloader confirms the protocol details already implemented in MangaMonitor-AI:

- `/chapter` plus `/chapter_view_template` are both required for chapter media;
- `/chapter_view_template` uses the distinct `18comicAPPContent` token secret;
- GIF is not scrambled;
- WEBP block reconstruction uses the pinned scramble/chapter/filename formula;
- image tasks are joined and successful image count must equal total image count before chapter success;
- unsupported image extensions are not scheduled by that upstream worker.

The upstream downloader may remove an existing final chapter directory before renaming a completed temporary directory. **MangaMonitor-AI does not adopt this behavior.** V1 add-only staging must never overwrite or delete an existing destination.

The upstream ecosystem also contains retry/cache-busting behavior for image retrieval. MangaMonitor-AI deliberately keeps the A6.14 exact-media transport stricter: one descriptor-bound GET, no query mutation, no hidden retry, no redirect, and no source credentials in the media client. A failed or empty response remains a failed staging attempt and may be retried only by a later newly authorized execution attempt.

### Pica findings

The pinned Pica client confirms the signed API shape, authorization header, chapter endpoint `comics/{id}/eps?page=N`, and image endpoint `comics/{id}/order/{order}/pages?page=N`.

The pinned downloader:

- reads the first image page to obtain total page count;
- retrieves every remaining image page;
- fails if any spawned page request fails;
- sorts page results by page number before flattening media order;
- joins all image download tasks;
- requires successful image count to equal total image count before chapter success;
- writes into a temporary directory before finalization.

MangaMonitor-AI preserves these completeness semantics and tightens them: chapter and image pagination are explicit fail-closed proofs, duplicate media IDs fail, changing/missing page counts fail, media origins/paths are constrained, media GETs carry no Pica API credentials, and no staging result grants library or task-completion authority.

## Existing MangaMonitor-AI execution chain accepted by this thaw

The repository already contained the guarded A6.10–A6.15 real source-to-staging path before this thaw. The thaw does not replace it with an upstream downloader.

Accepted execution chain:

```text
current pending task + current user approval
        ↓
exact ExecutorCommand / task revision / target hash
        ↓
current-generation source preflight authorization
        ↓
complete pinned source metadata enumeration
        ↓
non-reusable image/staging authorization
        ↓
exact media descriptor re-enumeration
        ↓
credential-isolated exact media GET
        ↓
JM transform only when descriptor-bound
        ↓
fresh commands/<command_id> isolated staging
        ↓
complete source transcript + manifest
        ↓
filesystem verification
        ↓
verified execution receipt
```

Safety properties retained:

- the local CLI refuses `GITHUB_ACTIONS=true`;
- authorization is revalidated before source reads, media fetches, file writes, and final acceptance;
- command staging directory must be new;
- each artifact uses create-new semantics and cannot overwrite an existing file;
- symlink/reparse escape is rejected;
- partial staging is not silently deleted;
- media redirects and hidden retries are disabled;
- each media response is bounded to 128 MiB and must match expected image magic;
- successful source completion requires full enumeration, all scheduled work joined, zero failed images, exact expected/completed counts, and matching artifact set;
- Pica chapter/image pagination must be complete;
- inventory mutation, task completion, promotion, replacement, and physical deletion remain false in the staging result and receipt chain.

The legacy A6.2 `LocalExecutionPlan.execution_supported=false` field remains unchanged. It is a retained skeleton/diagnostic field and is not the downstream live authority; later A6 generation-bound authorization layers control source reads and staging writes.

## New thaw-scope regression boundary

Before expanding live execution behavior, the branch added regression tests for the newly authorized scope. The live source-preflight authorization now fails before the first source request unless all of these are true:

- current task action is exactly `download`;
- command action is exactly `download`;
- the task has no `old_local_item_ids`;
- task revision/target/source binding is current;
- current user approval still authorizes the exact generation.

Therefore `upgrade` and any task already bound to local items remain frozen even though lower-level historical planning code can still model them.

Protocol/completion regressions were also refreshed for:

- Pica incomplete image pagination must not normalize as complete;
- JM partial image completion must not normalize as complete;
- a chapter not joined must not normalize as complete.

## Thawed authority

Allowed now:

```text
approved new-work download task
        ↓
real JM/Pica source metadata + media reads on the local machine
        ↓
fresh command-owned isolated staging
        ↓
manifest / filesystem proof / verified execution receipt
```

Still forbidden/not yet authorized:

- `upgrade` execution;
- replacing or overwriting any existing library target;
- staging-to-library promotion/materialization;
- writing `inventory_index.json`;
- marking a task completed;
- replacement or deletion;
- running the real executor in GitHub Actions;
- `production_enabled=true`.

## Remaining thaw-gate acceptance step

Repository review, upstream refresh, regression coverage, and the narrow live authorization boundary can be completed and merged without touching the user's manga library.

The next non-automatable acceptance step is a **guarded real local staging-only execution** on the user's machine using one currently approved add-only task. Acceptance requires:

1. a fresh staging root with `commands/` present;
2. local credentials only where required (Pica token stays in the process environment);
3. the exact current command/state/gate generation;
4. successful staging/proof/receipt output;
5. manual confirmation that downloaded files exist only under `commands/<command_id>` and no library/inventory/task state was mutated.

Only after that acceptance should the project proceed to the separate add-only inventory materialization/apply gate described by the V1 roadmap.
