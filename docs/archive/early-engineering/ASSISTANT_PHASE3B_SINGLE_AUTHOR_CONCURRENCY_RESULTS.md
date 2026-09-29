# Phase3B single-author JM+Pica concurrency results

## Status

Single-author dual-source acquisition is implementation-complete and live-validated on branch `assistant-phase3b-single-author-dual-source-concurrency`.

The change is deliberately limited to one confirmed author. It does **not** enable multi-author concurrency and it does **not** make concurrent acquisition the production Phase3B default yet.

The previously completed Phase3B scale/thaw-guard PR was merged to `main` first at merge commit `9f0c2c02cdc91b9ce1c5b3102479179bca75138f`.

## Architecture

The current production Phase3B runner remains the deterministic state authority.

A new read-only acquisition probe, `phase3b-dual-source-acquire`, performs exactly one author's JM and Pica source acquisition concurrently:

`one confirmed author -> JM source task || Pica source task -> deterministic JM-then-Pica observation tape -> existing Phase3B replay writer`

Important boundaries:

- only the network/source acquisition overlaps;
- each source's pagination remains sequential, so source-specific completion and incremental early-stop semantics are preserved;
- detail requests remain sequential within each source in this stage;
- the acquisition probe never mutates durable `monitor-state`;
- source results are serialized in deterministic JM-then-Pica order before replay;
- the existing Phase3B replay path remains responsible for catalog/review/pending/cursor/event state transitions and checkpoint semantics.

## Concurrent request budget

Single-author concurrent acquisition uses one shared physical-request budget.

Pica reserves one physical attempt per logical request. JM reserves enough budget for its complete already-pinned domain failover set, then returns unused reservation after the actual `RequestTrace` count is known. The final acquired request count is required to exactly equal the combined JM + Pica trace count.

This prevents concurrent source tasks from independently oversubscribing the global request budget and does not hide physical retries from accounting.

## Unit and baseline CI

The acquisition probe adds unit coverage for:

- shared budget never exceeding its limit and correctly returning unused reservation;
- rejecting an author selection that is not exactly one author.

Baseline CI run `34142439302` passed on the implementation branch:

- Linux workspace tests: success;
- Clippy with `-D warnings`: success;
- assistant read-only/offline guard: success;
- cloud/local separation: success;
- Phase3B scaling guard: success;
- production gate remains closed: success;
- Windows local executor/import/rescan/inventory candidate/apply-gate regressions: success.

The baseline CI workflow was also extended to treat the new single-author live validation workflow as a cloud workflow and to assert its one-author/read-only safety properties.

## Live validation design

Workflow: `Phase3B single-author dual-source concurrency validation`

Run: `34142573475`

The test selects confirmed author `2-G` from the current registry because that query has non-empty JM and Pica results.

The workflow runs three paths against the same input state and author:

1. the existing serial Phase3B live-source path;
2. the new concurrent JM+Pica acquisition path;
3. replay of the concurrent observation tape through the existing deterministic Phase3B writer.

Acceptance requires:

- serial source scan complete with no source-error boundary;
- concurrent JM boundary `COMPLETE`;
- concurrent Pica boundary `COMPLETE`;
- actual elapsed overlap observed;
- concurrent observations equal serial observations after normalizing source/author/page/record IDs/detail IDs/errors;
- replay boundaries equal serial boundaries;
- replay catalog/pending/review/event summary equals serial summary;
- no inventory mutation, task completion, promotion, replacement, physical deletion, or production-enable authority.

## First live attempt: source response anomaly, fail-closed preserved

Attempt 1 did not pass final acceptance because one JM `/album` request returned HTTP 200 with a body that failed JSON parsing (`INVALID_JSON`).

The concurrent acquisition correctly marked the JM source boundary as `SOURCE_ERROR`, the replay remained incomplete, and the workflow failed. No parser relaxation, retry masking, unavailable inference, or safety-gate weakening was introduced to make the run green.

The code was rerun unchanged to distinguish a transient live-source response from a deterministic concurrency/state bug.

## Second live attempt: PASS

Attempt 2 of the same run, with unchanged implementation, passed every workflow step.

Observed source results:

- author: `2-G`;
- total physical requests: 21;
- JM requests: 10;
- Pica requests: 11, including login;
- JM boundary: `COMPLETE`;
- Pica boundary: `COMPLETE`;
- concurrent overlap observed: true;
- all physical requests in the successful attempt returned HTTP 200 / `OK`;
- serial catalog: 18;
- replay catalog: 18;
- serial complete: true;
- replay complete: true;
- normalized serial and concurrent source evidence: equal;
- serial and replay boundaries: equal;
- serial and replay business summary: equal.

Timing from the successful attempt:

- existing serial Phase3B wall time: 42,273 ms;
- JM concurrent source task: 22,175 ms;
- Pica concurrent source task: 24,538 ms;
- concurrent JM+Pica source wall time: 24,538 ms;
- acquisition probe total time including the preceding Pica login: 26,875 ms.

For this live sample, source acquisition wall time dropped from about 42.3 seconds to 24.5 seconds, roughly a 42% reduction (about 1.72x throughput for the same one-author source evidence). Including the Pica login performed before the concurrent join, total probe time was about 26.9 seconds, still materially below the serial baseline.

These timings are a single live sample, not a production throughput guarantee. The important acceptance result is that real JM and Pica network work overlapped while deterministic source evidence and Phase3B state semantics remained equivalent.

Artifact from the successful attempt:

- `phase3b-single-author-dual-source-34142573475-2`
- artifact ID `10026711463`

## Safety boundary

This stage adds no downstream authority.

All of the following remain false:

- durable state mutation by the acquisition probe;
- inventory mutation authorization;
- task completion authorization;
- staging-to-library promotion authorization;
- replacement authorization;
- physical delete authorization;
- production enablement authorization.

`production_enabled=false` remains unchanged, and the real local download executor remains frozen.

## Decision boundary

Single-author JM+Pica concurrency has now passed an actual live-source equivalence test. No multi-author concurrency is implemented by this stage.

The next step is intentionally left for a separate decision after reviewing this result. Possible later work includes integrating this acquisition architecture into the normal Phase3B runner and/or adding bounded multi-author concurrency, but neither is implied or authorized by this result document.
