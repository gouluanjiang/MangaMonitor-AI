# Assistant A4 — controlled review decisions

## Goal

A4 prepares the bounded decision-writing layer for assistant-reviewed identity cases.

It supports user-confirmed decisions against one **currently active review item** and stages a proposed `decisions.json` together with an auditable deterministic reanalysis preview.

A4 still does not publish state by itself and does not perform source requests, downloads, replacement, coverage mutation, or deletion.

## Supported decisions

A4 supports exactly three review decisions:

- `same` — the reviewed source record is the same work as one presented candidate work;
- `not-same` — the reviewed source record is not the same work as one presented candidate work;
- `ignore` — ignore this source record for future identity/download processing while preserving its historical catalog record.

`same` and `not-same` require a `work_id` that is already present in the active review item's candidate list and in the inventory. This prevents the assistant from inventing arbitrary mappings outside the bounded evidence it was shown.

`ignore` is source-record scoped. A4 does not silently convert it into `ignored_works` because ignoring an entire work has broader semantics than dismissing one source observation.

## Durable decision representation

A4 reuses the existing production decision schema:

- `positive_mappings` for `same`;
- `negative_mappings` for `not-same`;
- `ignored_source_records` for `ignore`.

Existing decisions are preserved. The tool never deletes or rewrites unrelated decisions.

## Conflict policy

A4 fails closed rather than silently repairing contradictions.

Examples:

- `same` is refused if the same source/work pair already has `not-same`;
- `not-same` is refused if the same source/work pair already has `same`;
- `same` is refused if the source already has a different positive human mapping;
- an unknown, resolved, stale, or internally inconsistent review is refused;
- a `work_id` not in the current review candidate set is refused;
- a missing candidate work in inventory is refused.

Existing authoritative same-site source mappings are not rewritten by A4. If a new human `not-same` contradicts such source authority, the deterministic matcher may continue to return a conflict/review condition rather than pretending the contradiction is resolved.

## Stale-review protection

A4 acts through `review_id`, not a free-form source key.

Before staging a decision, the backend validates that:

- the review exists;
- it is still `REVIEW_REQUIRED`;
- its `source_key` still exists in the catalog;
- its stored matcher version agrees with the current matcher rule version when present;
- its provenance detail fingerprint, when present, still matches the catalog detail fingerprint.

This prevents a decision prepared for older evidence from being silently applied to a materially changed source observation.

## Deterministic reanalysis preview

After applying the proposed decision to an in-memory copy of state, A4 invokes the accepted deterministic Matcher M2 decision path for the reviewed source record.

The staged output therefore includes `reanalyze-preview.json` showing what the production matcher would conclude from the proposed decision under the current state generation.

The preview is evidence, not an independent source of truth. When the decision is later published, normal production context hashing causes deterministic offline reanalysis again.

## Staging output

`assistant-decision-edit` reads one complete state directory and writes only into a **new output directory**:

- `decisions.json` — complete proposed decision document;
- `decision-change.json` — audit of the requested decision and the exact durable mutation/no-op;
- `reanalyze-preview.json` — deterministic matcher result after the proposed decision.

The input state directory remains byte-identical. Existing output paths are refused.

## Idempotence

- repeating the same `same` decision is a no-op;
- repeating the same `not-same` decision is a no-op;
- repeating `ignore` is a no-op;
- no-op results remain explicitly audited.

No-op does not duplicate mappings or ignored source keys.

## Safety boundary

A4 does not:

- call JM or Pica;
- call ChatGPT/OpenAI from Rust;
- modify source observations;
- generate a `work_id` for a new work;
- approve downloads;
- mutate pending tasks;
- mutate inventory;
- perform replacement or deletion;
- write the original No-AI repository;
- enable production.

## Acceptance criteria

A4 is accepted when tests prove:

- `same` adds exactly one positive mapping for a current bounded candidate;
- `not-same` adds exactly one negative mapping for a current bounded candidate;
- `ignore` adds exactly one ignored source record;
- repeated identical decisions are idempotent;
- contradictory SAME/NOT_SAME pairs fail closed;
- a different pre-existing positive mapping fails closed;
- unknown/resolved/stale reviews fail closed;
- non-candidate or missing work IDs fail closed;
- proposed decisions preserve unrelated decision entries;
- deterministic reanalysis preview reflects the proposed decision;
- input state bytes remain identical;
- existing output directories are refused;
- execution succeeds with network proxies pointed at invalid loopback endpoints;
- all prior tests and Clippy continue to pass;
- `production_enabled=false` remains unchanged.

A4 does not yet provide the publication step that atomically commits the staged decision back into live monitor state; that remains a separate commit-aware write boundary.
