# MangaMonitor AI Idea v1

## Goal

Build an optional ChatGPT assistant layer on top of the existing MangaMonitor backend without making AI a required runtime dependency.

The system must remain useful when ChatGPT is unavailable, the user is unsubscribed, or AI usage is temporarily unavailable. GitHub Actions and deterministic Rust components continue collecting and preserving source observations. Unreviewed items accumulate safely and can be processed later when ChatGPT is available again.

Core principle:

> AI absent -> accumulate safely.
>
> AI present -> review intelligently.
>
> AI wrong -> deterministic safety gates prevent irreversible damage.

## Existing assets to reuse

The imported No-AI baseline already provides the bulk of the infrastructure required by the AI-assisted design:

- `jm-adapter` and `pica-adapter` for source metadata collection.
- `cloud-monitor` for scanning, batching, checkpoint/resume and state transitions.
- `state-model` for `LocalItemId`, `WorkId`, `SourceKey`, versions, coverage and source records.
- `rules-core` and the accepted M1/M2/M3 matcher path as a conservative deterministic pre-filter.
- `catalog.json` plus fingerprints and timestamps as durable source memory.
- `review.json` and review exports as the natural backlog for ambiguous observations.
- `decisions.json` as durable human/assistant-confirmed identity memory.
- `pending.json` and `task_revision` as durable work/download intent.
- `inventory_index.json` as the local collection truth interface.
- `scan_state.json` and `latest.json` for scan progress and meaningful change reporting.
- low-concurrency source access, historical-ID incremental stopping, six-month recovery full scans, soft batching, checkpoint/resume and commit-aware state updates.

None of these should be reimplemented merely because an assistant layer is added.

## Product experience

The long-term user experience should support natural-language tasks such as:

- "Add author X to the monitored author list."
- "Stop monitoring author Y."
- "What new manga have appeared for all monitored authors since my last review?"
- "Which of these are already in my local library?"
- "Which are genuinely new works?"
- "Which existing works have a better Chinese/uncensored/color candidate?"
- "Download items 1, 3 and 7."
- "What is currently waiting for review or download?"
- "How many works are in my collection now?"

ChatGPT should act as the semantic controller and user interface; Rust/GitHub/local components remain the execution and safety substrate.

## AI availability must never gate collection

The collection pipeline must not call AI in order to make progress.

A normal unattended scan is:

1. GitHub Actions wakes the monitor.
2. The monitor reads the active author registry.
3. JM/Pica metadata is scanned using existing bounded, fail-closed behavior.
4. Existing source IDs/fingerprints are compared deterministically.
5. Unchanged observations are skipped.
6. New or meaningfully changed observations are persisted to the catalog.
7. Deterministically safe conclusions may be applied by the conservative matcher.
8. Anything requiring semantic review remains `REVIEW_REQUIRED` / pending review.
9. State is committed even when there is no AI reviewer available.

If the user does not use ChatGPT for months, the only intended consequence is a larger review backlog.

## Backlog semantics

Do not create duplicate monthly AI tasks for the same source record.

The durable identity is `source + source_work_id`. Preserve at least:

- `first_seen`
- `last_seen`
- `last_checked`
- current metadata/fingerprint
- review status/reason
- candidate work IDs
- author evidence
- matcher evidence/provenance

When metadata changes while an item is still unreviewed, update the source record/revision rather than appending duplicate logical items.

A user returning after a long absence should be able to request all items pending since the last completed review checkpoint and process them in batches.

## Responsibility split

### ChatGPT owns semantic/user-facing decisions

ChatGPT may eventually:

- summarize scan results;
- classify review candidates as existing/new/upgrade/not useful/uncertain;
- compare a source observation against a bounded set of relevant local candidates;
- manage the active monitored-author registry through explicit user requests;
- propose durable SAME / NOT_SAME / IGNORE decisions;
- recommend downloads and upgrades;
- interpret local scan/download results;
- present the current collection and backlog in natural language.

### Deterministic code owns facts and invariants

Rust/GitHub/local components remain authoritative for:

- HTTP/source interaction;
- pagination completeness;
- source/auth/network failure classification;
- IDs, timestamps and fingerprints;
- checkpoint/resume;
- atomic state writes and commit race protection;
- task revisions;
- file existence and download completion;
- inventory scanning;
- hashes/metadata integrity;
- coverage hard facts;
- physical replacement/deletion safety gates.

ChatGPT must not fabricate source facts or directly rewrite catalog observations as if they came from JM/Pica.

## Author management

The assistant layer should allow explicit natural-language additions/removals to the active monitored-author registry.

Removing an author means "stop future monitoring" only. It must not erase:

- historical catalog observations;
- inventory ownership;
- prior decisions;
- completed task history.

Re-adding an author should be able to reuse retained history.

Ambiguous author spelling/alias changes should be surfaced for confirmation instead of silently merging identities.

## Review flow

Preferred layered flow:

```text
source observation
  -> deterministic change detection
  -> conservative deterministic matcher
      -> safe deterministic result, OR
      -> REVIEW_REQUIRED
          -> ChatGPT semantic review
              -> durable decision / recommendation
                  -> deterministic reanalysis
```

The assistant should consume a bounded, structured review view rather than the entire catalog on every request.

For one new observation, the backend should preferentially provide only relevant inventory candidates (for example same confirmed author scope), along with raw source evidence and matcher provenance.

## User approval before download

AI recommendation and download authorization are separate states.

Conceptually:

```text
DISCOVERED
  -> PENDING_REVIEW
  -> REVIEWED
  -> CHATGPT_RECOMMENDED
  -> USER_APPROVED
  -> DOWNLOAD_PENDING
  -> DOWNLOADING
  -> VERIFIED
  -> COMPLETED
```

The exact schema may reuse existing `pending.json` and `task_revision`; do not introduce redundant state unless needed.

ChatGPT must not turn a recommendation into a local download without the user's approval unless the user later explicitly chooses an automatic policy.

## Local execution

ChatGPT does not need raw access to the Windows filesystem.

A future local MangaMonitor agent/updater should expose narrow deterministic operations such as:

- scan local library;
- fetch approved tasks;
- download a JM/Pica target;
- verify completion;
- rescan inventory;
- report task results;
- perform a replacement only when hard safety gates pass.

Early versions may still require the user to launch the local executable manually. Later unattended polling is optional.

## Replacement and deletion

AI may recommend replacement/deletion but may never bypass the deterministic safety layer.

Physical deletion must remain impossible unless hard conditions are independently satisfied, including as applicable:

- replacement download is fully verified;
- task revision is current;
- required content/coverage is preserved;
- no decisive UNKNOWN blocks authorization;
- required extra/bonus content is preserved;
- replacement file actually exists and is readable;
- the deterministic deletion gate authorizes the operation.

Identity SAME alone is never deletion authorization.

## Assistant interface phases

### A0 — isolated baseline

- independent repository created;
- full source history copied;
- source baseline recorded;
- No-AI repository remains untouched.

### A1 — assistant data contract

Define stable read-only views for:

- scan summary;
- new/changed candidates since review checkpoint;
- review backlog;
- one candidate plus bounded inventory matches;
- pending tasks;
- inventory summary.

No AI writes yet.

### A2 — read-only assistant

ChatGPT can answer collection/update questions using repository state but cannot mutate project state.

### A3 — controlled author management

Allow explicit user-requested add/deactivate operations with validation and audit history.

### A4 — controlled review decisions

Allow ChatGPT/user-confirmed SAME / NOT_SAME / IGNORE decisions through the decision layer, followed by deterministic reanalysis.

### A5 — download approval queue

Separate assistant recommendations from explicit user-approved download tasks.

### A6 — local executor integration

Connect approved tasks to the Windows downloader/updater while preserving completion semantics and task revisions.

### A7 — upgrade/replacement assistance

ChatGPT may assist semantic version/coverage interpretation; deterministic safety constraints remain authoritative.

### A8 — deletion integration

Only after dry-run and adversarial safety validation. ChatGPT can recommend; deterministic code alone can authorize physical deletion.

## Non-goals for v1

- No autonomous AI call on every source record.
- No requirement for OpenAI API billing.
- No Codex dependency in production.
- No ChatGPT dependency for scheduled scanning.
- No direct AI access to raw credentials.
- No raw manga image/archive storage in GitHub.
- No direct AI override of network, transaction, file-integrity or deletion safeguards.
- No reverse synchronization into the original No-AI repository.

## Cost/usage intent

Routine unattended monitoring should consume zero Codex usage and zero mandatory AI API usage.

GitHub/Rust should remove unchanged historical records before any semantic review. ChatGPT should normally receive only new/changed candidates and bounded relevant inventory evidence.

If ChatGPT access is unavailable, the review backlog waits without data loss.

## v1 success condition

MangaMonitor-AI v1 is successful when the user can return after an arbitrary AI-offline period and ask, in natural language, for all unreviewed changes since the previous review; ChatGPT can classify and present those candidates against the current collection; the user can approve selected downloads; and the deterministic backend/local executor carries out and verifies approved work without making AI a prerequisite for collection or a bypass around irreversible safety controls.
