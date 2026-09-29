# Assistant Data Contract v1

Status: design contract for the first read-only assistant layer. This file defines what ChatGPT may read before any mutation capability is introduced.

## Design goals

1. Reuse the eight existing durable state files instead of creating a parallel database.
2. Expose only the minimum structured evidence needed for semantic review.
3. Never require ChatGPT to reread the entire catalog or the entire inventory for routine review.
4. Preserve enough provenance that a user or deterministic reanalysis can audit every assistant conclusion.
5. Keep source facts and assistant judgments separate.
6. Allow review backlog to accumulate indefinitely while AI is unavailable.

## Existing durable sources

The assistant view is derived from, but does not replace:

- `authors.json`
- `catalog.json`
- `inventory_index.json`
- `pending.json`
- `review.json`
- `decisions.json`
- `scan_state.json`
- `latest.json`

In v1 the assistant layer is read-only. It MUST NOT mutate any of the above files.

## View 1: assistant scan summary

Purpose: answer questions such as "What happened in the latest scan?" without loading detailed records.

Suggested logical shape:

```json
{
  "schema_version": 1,
  "view": "scan_summary",
  "scan_id": "...",
  "scan_status": "COMPLETE|PARTIAL",
  "started_at": "...",
  "requested_mode": "incremental|full",
  "selected_author_count": 0,
  "review_required_count": 0,
  "pending_task_count": 0,
  "latest_event_count": 0,
  "source_failures": [],
  "progress_summary": {}
}
```

No source fact may be inferred from absence when a source/author boundary is partial.

## View 2: review backlog summary

Purpose: answer "How much is waiting for me?" and permit batching.

```json
{
  "schema_version": 1,
  "view": "review_backlog_summary",
  "total": 0,
  "reason_counts": {},
  "oldest_first_seen": null,
  "newest_last_seen": null,
  "batch_cursor": null
}
```

A review item remains part of the backlog until a durable decision/review-state transition resolves it. AI availability has no effect on collection or persistence.

## View 3: review batch

Purpose: provide a bounded batch of unresolved source observations for semantic review.

Each item should contain only source facts already persisted plus bounded candidate evidence:

```json
{
  "review_id": "...",
  "source_key": "jm:123",
  "source": "jm",
  "source_work_id": "123",
  "first_seen": "...",
  "last_seen": "...",
  "last_checked": "...",
  "raw_title": "...",
  "raw_authors": [],
  "metadata": {},
  "processing_result": "...",
  "review_reason": "...",
  "author_evidence": {},
  "matcher_version": "...",
  "identity_evidence": {},
  "provenance": {},
  "candidate_work_ids": [],
  "candidate_works": []
}
```

`candidate_works` is populated only from the listed `candidate_work_ids` or another explicitly bounded deterministic candidate query. The assistant must not receive all 2833 works for every item.

## View 4: bounded inventory candidate

For each candidate work, expose enough information to compare identity and version without exposing irrelevant local filesystem details:

```json
{
  "work_id": "WORK_00001",
  "owned": true,
  "authors_confirmed": [],
  "title_candidates": [],
  "versions": [],
  "source_mappings": {}
}
```

No assistant conclusion may silently alter these facts.

## View 5: pending task summary

Purpose: answer "What is waiting to be downloaded/upgraded?"

Expose at least:

```json
{
  "task_id": "...",
  "work_id": "...",
  "task_revision": 1,
  "status": "...",
  "action": "...",
  "first_seen": "...",
  "target": {},
  "old_local_item_ids": []
}
```

A completion result for an older revision must never clear a newer revision.

## View 6: collection summary

Purpose: answer "How many works do I currently own?" without sending the full inventory.

Suggested fields:

```json
{
  "schema_version": 1,
  "view": "collection_summary",
  "total_work_ids": 0,
  "owned_work_ids": 0,
  "local_item_count": 0,
  "authors_with_owned_works": 0
}
```

Counts are derived from `inventory_index.json`; ChatGPT does not invent or persist them as source truth.

## Review checkpoint semantics

The assistant layer should eventually maintain a separate, auditable review checkpoint rather than overloading source scan timestamps.

Conceptually it records the latest completed assistant/user review window, not the latest network scan.

Requirements:

- absence of a checkpoint means "all unresolved reviews are eligible";
- review checkpoint must never delete or hide unresolved items;
- returning after months without AI must expose every still-unresolved item regardless of how many source scans occurred;
- metadata changes to an unresolved source record update the same logical source identity rather than creating duplicate monthly backlog entries.

The initial read-only phase may derive batches directly from `review.json` and defer persistence of the checkpoint until controlled write capability is designed.

## Provenance separation

Every assistant-facing item must distinguish:

- source observation facts;
- deterministic matcher evidence;
- inventory facts;
- prior durable decisions;
- assistant recommendation (future write layer);
- user approval (future write layer).

Assistant recommendations must never be serialized into fields that claim to be raw JM/Pica observations.

## Privacy and credential boundary

Assistant views must exclude:

- Pica tokens/passwords/cookies;
- authorization headers;
- raw HTTP bodies not already sanitized;
- image URLs where they are not needed for the decision;
- local filesystem paths unless a later local-executor task explicitly requires a bounded path reference.

The existing sanitized review export is the preferred starting point.

## A1 acceptance criteria

A1 is complete when the repository contains deterministic code/tests that can produce these read-only logical views from an existing state directory while satisfying all of the following:

- no network requests;
- no state mutation;
- input state files remain byte-identical;
- output is deterministic for the same state;
- review batches are bounded;
- candidate inventory is bounded to relevant work IDs;
- malformed state fails closed through existing persistence validation;
- no change to production matcher behavior;
- `production_enabled` remains false.
