# Assistant A4 Results

## Status

**PASS**

A4 adds the bounded review-decision staging layer for the optional assistant system.

## Accepted behavior

- `same` writes a proposed `positive_mappings` entry only for a work already presented in the active review candidate set and present in inventory.
- `not-same` writes a proposed `negative_mappings` entry under the same bounded-candidate requirement.
- `ignore` writes a proposed source-scoped `ignored_source_records` entry; it does not silently ignore an entire work.
- Decisions are addressed through an active `review_id`, not a free-form source key.
- Review title/author evidence is checked against the current catalog record.
- Stored matcher version and provenance detail fingerprint are checked when present; stale review evidence fails closed.
- Contradictory SAME/NOT_SAME pairs fail closed.
- A different existing positive human mapping for the same source fails closed.
- Unknown, resolved, stale, non-candidate, or missing-work decisions fail closed.
- Repeated identical decisions are deterministic no-ops and do not duplicate durable entries.
- Unrelated existing decisions are preserved.
- After the proposed decision is applied to an in-memory state copy, accepted Matcher M2 logic is invoked to emit `reanalyze-preview.json`.
- The staging CLI writes only `decisions.json`, `decision-change.json`, and `reanalyze-preview.json` into a new output directory.
- Input state files remain unchanged.
- No source request, AI runtime call, download, replacement, coverage mutation, deletion, or original No-AI repository write is performed by A4.
- `production_enabled=false` remains unchanged.

## Validation

Implementation merge commit: `2e896c7e4935e6ac9ce05be890cf566e8d493fc5`

A small test-only helper-shadowing error was found by the first Linux run and fixed in commit:

`471aa0850e8770f308f88003a8e22346a58f2c02`

Final GitHub Actions run: `34038417470`

Job: `101500630058` (`rust-regression`)

Final result: **success**.

Validated steps:

- complete workspace tests: PASS;
- assistant CLI build step: PASS;
- Clippy with warnings denied: PASS;
- assistant read-only/offline regression: PASS;
- production gate closed check: PASS.

The failed predecessor run `34038285425` failed only because a Rust test-local variable named `state` shadowed the test helper function `state()`. Production A4 logic had compiled up to the integration-test compilation boundary; the helper was renamed/restructured and the complete succeeding run closed the issue.

## A4 acceptance

A4 is accepted.

The next assistant-layer phase is A5: separate assistant recommendation from explicit user download approval, while preserving `task_revision`, current-target checks, and deterministic local execution safety.
