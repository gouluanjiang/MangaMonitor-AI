# Pre-production Author Search Acceptance Gate

## Purpose

Formal production use must remain disabled until the implementation project is otherwise complete and a final live, read-only author-search acceptance test passes.

This gate is intentionally separate from implementation CI. Unit/regression success does not substitute for a live author-search acceptance run against the current JM/Pica services.

`production_enabled=false` must remain in force until this gate is recorded as PASS.

## Timing

Run this gate only after all required implementation stages have been merged and the final main-branch CI is green, but before the first formal production monitoring/download run.

The author-search acceptance run itself is read-only. It must not approve download tasks, fetch manga image bytes, write staging files, mutate inventory, complete tasks, promote/replace files, or delete anything.

## Test set

Use at least the following two cases.

### Case A — single author

Search one known author independently through every enabled source.

Default seeded candidate:

- `Hisasi`

A different known author may be substituted if the live service has changed materially, but the exact input and reason must be recorded.

### Case B — multiple authors

Search multiple authors in one acceptance session while preserving independent per-author/per-source results.

Default seeded set:

- `40010試作型`
- `Hisasi`
- `武田弘光`

This is not a union-only test. Each author/source pair must retain its own pagination, request outcome, records, and error state so one author's/source's failure cannot be hidden by another successful result.

## Required live behavior

For each author/source pair:

1. The exact author query must be sent through the pinned read-only metadata search path.
2. Pagination must remain bounded and fail closed. Reaching a configured budget before the server-reported end is not equivalent to an exhausted search.
3. A source/network/API failure must be recorded as an error, never converted to “zero works”.
4. Returned source IDs must be valid for that source and must not collide within the same author/source result.
5. Result ordering and page-one repeat evidence must be captured so obvious ID/order instability is visible.
6. At least one returned work where practical should be checked through the existing detail metadata path and bound back to the same source work ID.
7. Author metadata from detail/search should be inspected for the expected author where the source provides that field. Missing author fields must remain “unknown/missing”, not be fabricated as a match.
8. Sanitized evidence must not contain authentication tokens, passwords, cookies, authorization headers, raw response bodies, image URLs, or other credentials/sensitive transport data.
9. The run must leave monitor state and the manga archive byte-for-byte unchanged.

## Acceptance rules

The gate is PASS only when:

- the single-author case completes without an unclassified source error;
- the multi-author case completes with every author/source pair accounted for;
- pagination status is explicit for every pair;
- duplicate/invalid IDs are absent;
- detail ID binding checks pass where performed;
- no source error is mislabeled as an empty search;
- the read-only/no-state-mutation invariant is proven;
- sanitized evidence is retained for review;
- final main-branch CI remains green after any fixes required by the live test.

If a live service is unavailable, authentication is invalid, pagination cannot be exhausted within the approved safety budget, or results violate an invariant, the gate is FAIL/INCONCLUSIVE and formal production use remains disabled.

## Formal-use transition

After this gate passes, production should still be enabled by an explicit, reviewable configuration change/approval step. Passing the author-search gate must never implicitly flip `production_enabled` or grant download, promotion, replacement, or deletion authority.
