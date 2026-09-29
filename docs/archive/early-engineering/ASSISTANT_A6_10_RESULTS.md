# Assistant A6.10 — image download authorization results

Status: **PASS candidate** on `assistant-a6-image-download-authorization`.

Baseline: `main@1b9b34a037ff91176dce6d4233fa3889e32667ae` (accepted A6.9).

## Goal

A6.10 defines the first gate that may positively authorize image-byte download and command-owned staging writes, while performing no download or filesystem mutation itself.

## Exact live-preflight binding

The gate requires the exact typed A6.9 `LiveSourcePreflightResult` in-process. A saved JSON preflight is not an accepted transferable capability.

Before granting image/staging authority, A6.10 requires:

- exact A6.9 schema;
- live metadata enumeration completed;
- A6.9 pre/post authorization marked stable;
- A6.9 pre/post current-state hashes equal;
- A6.9 pre/post gate-ledger hashes equal;
- command/task/work/revision/target/source/source-work binding equals the A6.6 request;
- all A6.9 image/staging/downstream authority flags still false.

The embedded A6.7 evidence is then revalidated from scratch, and the resulting proof must exactly equal the A6.9 embedded proof. A forged `preflight_hash` or forged scope proof therefore fails closed.

## Immediate current-generation recheck

After validating the live result, A6.10 executes the accepted A6.8 current-state/current-approval authorization again.

The current state/task-generation binding hash and current gate-ledger hash must still equal the hashes recorded at the end of A6.9. Any task or gate change after metadata preflight therefore invalidates image-download authority and forces a fresh preflight generation.

Covered negative cases include:

- user approval revoked after preflight;
- task revision changed after preflight;
- otherwise still-approved gate ledger changed after preflight;
- forged A6.7/A6.9 proof data;
- unsafe capability flags injected into the live result.

## Positive authority

A successful A6.10 authorization sets only:

- `image_download_authorized=true`;
- `staging_write_authorized=true`.

The write scope is fixed to `COMMAND_OWNED_STAGING_ONLY`, and `staging_subdir` remains the exact A6.2 namespace `commands/<command_id>`.

The authorization carries the exact accepted A6.7 `preflight_hash`, expected chapter count, and expected content-unit count so the later downloader can bind its work to the exact expected source scope.

## Non-transferable result

`ImageDownloadAuthorization` is deliberately `Serialize`-only and always has `reusable_permit=false`.

A future downloader must obtain it in-process from the current state/gate plus the typed A6.9 result immediately before starting its isolated staging operation. A6.10 intentionally provides no CLI that accepts a saved authorization object.

## Downstream authority remains closed

Every A6.10 result keeps these false:

- inventory mutation;
- task completion;
- promotion;
- replacement;
- physical deletion.

A6.10 itself creates no directory, writes no file, calls no source endpoint, and mutates no monitor state.

## Validation

Final code CI: `34086435518` — **SUCCESS** on code commit `04960186fde12aeef6b0784ccd1aaea33abce907`.

Passed:

- complete workspace regression suite;
- exact current typed A6.9 positive authorization case;
- exact A6.7 `preflight_hash` propagation;
- command-owned staging namespace assertion;
- gate-generation drift after preflight fail-closed case;
- revoked approval after preflight fail-closed case;
- changed task revision after preflight fail-closed case;
- forged preflight proof fail-closed case;
- unsafe A6.9 staging capability fail-closed case;
- all downstream mutation/promotion/delete flags remain false;
- Clippy with warnings denied;
- existing assistant offline/read-only regression;
- `production_enabled=false` guard.

## Safety boundary and next phase

A6.10 authorizes only image-byte download and writes below the exact command-owned staging namespace. It does not implement those operations itself.

The next safe phase is A6.11: define and validate the exact pinned JM/Pica media descriptors and deterministic staging artifact paths bound to A6.10/A6.7 `preflight_hash`, without yet allowing inventory/task/promotion/delete changes. The following execution phase can then perform actual media-byte download into isolated command staging and produce A6.5 completion evidence for A6.3/A6.4 verification.
